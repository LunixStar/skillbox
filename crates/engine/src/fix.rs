//! fix —— 逐项修复（TASKBOOK §6.3，K4）。
//!
//! 【硬约束】
//! - 禁止"一键修复全部"作为默认行为；`DuplicateName` 只提示，绝不自动
//!   挑选正本。
//! - 危险项（RealDir 收编 / ForeignLink 与悬空链接移除 / copy 副本更新）
//!   先 dry-run 影响清单（`FixOutcome::NeedsConfirmation` 的 reason 即
//!   影响面预览），`UserConfirmed` 逐个确认后才执行。
//! - 不信任 doctor 快照：执行前重新 inspect + reconcile，按 issue id
//!   匹配；问题已不存在 → `Skipped`。每个执行器入口再做执行期复查
//!   （TOCTOU 防护），绝不按快照动盘。
//! - 每次删除先入 trash（§5.3，v1 无硬删除）；写盘期间 mute 监听（RAII）。
//! - 悬空链接重建用 `NoClobber` 提交：先移走空链接，再向 Absent 路径
//!   部署——重建窗口内出现的任何真实内容都会被拒绝，消除 TOCTOU 竞争。
//!
//! 执行器分发表（DiscrepancyKind → 动作）：
//!
//! | 差异 | 策略 | 动作 |
//! |---|---|---|
//! | `DuplicateName` | 任意 | 恒 `Skipped`（人工挑正本） |
//! | `UnmanagedRealDir` | 需 UserConfirmed | adopt：搬入中央库 + 原位链接/副本 |
//! | `ForeignLink` | 需 UserConfirmed | 链接移入 trash |
//! | `BrokenLink`(RebuildLink) | 任意 | 重建链接（NoClobber 提交） |
//! | `BrokenLink`(RemoveLink) | 需 UserConfirmed | 悬空链接移入 trash |
//! | `MissingTarget` | 同请求策略 | 重新部署 |
//! | `UnrecordedDeployment` | 任意 | 补登记 target 记录 |
//! | `CopyDrift` | 需 UserConfirmed | 从正本更新副本 + 刷新 source_hash |

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillbox_domain::{AppResult, ErrorKind, ReplacePolicy, SkillId, SyncMode, ToolId};
use skillbox_ports::{EntryKind, IndexStore, LinkKind, WatchMute};

use crate::deploy::write::{cleanup_tmp, fresh_tmp, move_to_trash, stage_copy, stage_link, MuteGuard};
use crate::deploy::{deploy, DeployRequest, DeployTarget};
use crate::doctor::{hash_dir, inspect_disk, HealthContext};
use crate::reconcile::reconcile;
use crate::scan::{parse_frontmatter_head, SKILL_FILE, SKILL_HEAD_LIMIT};
use crate::types::{
    DeployReport, Discrepancy, DiscrepancyKind, FixOutcome, FixReport, FixResult, IndexView,
    IssueId, SkillRecord, SuggestedFix, TargetRecord, ToolDeployOutcome,
};

// ---------------------------------------------------------------------------
// 请求
// ---------------------------------------------------------------------------

/// 修复请求：显式选中的问题 id 列表 + 逐项上下文。
///
/// 【注意】本类型不可序列化（端口句柄随请求传递，同 `ScanRequest` 约定）。
pub struct FixRequest<'a> {
    /// 显式选中的问题 id（doctor 报告的 `DoctorIssue.id`）。
    pub items: Vec<IssueId>,
    pub policy: ReplacePolicy<'a>,
    pub ctx: HealthContext,
    pub trash_root: PathBuf,
    /// 索引写入端口；None = 只修磁盘、不碰索引（需要登记的动作将 Skipped）。
    pub index_store: Option<Arc<dyn IndexStore>>,
    pub watcher: Option<Arc<dyn WatchMute>>,
    /// dry-run：逐项返回将执行的动作与影响面（NeedsConfirmation），
    /// 零写盘、零索引写入（§8 写操作统一 dry_run 约束）。
    pub dry_run: bool,
}

impl fmt::Debug for FixRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixRequest")
            .field("items", &self.items)
            .field("policy", &self.policy)
            .field("ctx", &self.ctx)
            .field("trash_root", &self.trash_root)
            .field("index_store", &self.index_store.is_some())
            .field("watcher", &self.watcher.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 逐项修复：重新推算差异（K4）→ 按 id 匹配 → 逐项执行。
pub fn fix(req: FixRequest<'_>) -> AppResult<FixReport> {
    let disk = inspect_disk(&req.ctx, None)?;
    let live = reconcile(&disk, &req.ctx.index);
    let by_id: HashMap<String, &Discrepancy> =
        live.iter().map(|d| (d.issue_id().0, d)).collect();

    let mut results = Vec::with_capacity(req.items.len());
    let mut handled: HashSet<String> = HashSet::new();
    for item in &req.items {
        let outcome = if !handled.insert(item.0.clone()) {
            FixOutcome::Skipped { reason: "重复传入的问题 id，已忽略".to_string() }
        } else if let Some(d) = by_id.get(item.0.as_str()) {
            if req.dry_run {
                dry_run_outcome(d)
            } else {
                execute_item(&req, d)
            }
        } else {
            FixOutcome::Skipped { reason: "问题已不存在（磁盘现状已变化）".to_string() }
        };
        results.push(FixResult { issue_id: item.clone(), outcome });
    }
    Ok(FixReport { results })
}

// ---------------------------------------------------------------------------
// 分发与确认门
// ---------------------------------------------------------------------------

fn execute_item(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    match d.kind {
        // §6.3：同名只提示，绝不自动选正本——任何策略下都跳过。
        DiscrepancyKind::DuplicateName => FixOutcome::Skipped {
            reason: "同名目录需人工挑选正本，fix 不自动处理".to_string(),
        },
        DiscrepancyKind::UnmanagedRealDir => {
            if let Some(outcome) = confirm_gate(req, adopt_impact(d)) {
                return outcome;
            }
            adopt(req, d)
        }
        DiscrepancyKind::ForeignLink => {
            if let Some(outcome) = confirm_gate(req, remove_link_impact(d)) {
                return outcome;
            }
            remove_link_to_trash(req, d, false)
        }
        DiscrepancyKind::BrokenLink => match d.suggested_fix {
            // 匹配到正本的悬空链接：任何策略下都允许重建（链接无数据）。
            SuggestedFix::RebuildLink => rebuild_link(req, d),
            _ => {
                if let Some(outcome) = confirm_gate(req, remove_broken_impact(d)) {
                    return outcome;
                }
                remove_link_to_trash(req, d, true)
            }
        },
        DiscrepancyKind::MissingTarget => redeploy(req, d),
        DiscrepancyKind::UnrecordedDeployment => register_record(req, d),
        DiscrepancyKind::CopyDrift => {
            if let Some(outcome) = confirm_gate(req, update_copy_impact(d)) {
                return outcome;
            }
            update_copy(req, d)
        }
    }
}

/// dry-run 预览：逐项返回将执行的动作与影响面，零写盘、零索引写入。
fn dry_run_outcome(d: &Discrepancy) -> FixOutcome {
    match d.kind {
        DiscrepancyKind::DuplicateName => FixOutcome::Skipped {
            reason: "同名目录需人工挑选正本，fix 不自动处理".to_string(),
        },
        DiscrepancyKind::UnmanagedRealDir => FixOutcome::NeedsConfirmation { reason: adopt_impact(d) },
        DiscrepancyKind::ForeignLink => FixOutcome::NeedsConfirmation { reason: remove_link_impact(d) },
        DiscrepancyKind::BrokenLink if matches!(d.suggested_fix, SuggestedFix::RebuildLink) => {
            FixOutcome::NeedsConfirmation {
                reason: format!("将重建指向中央库正本的链接 {}", d.path.display()),
            }
        }
        DiscrepancyKind::BrokenLink => FixOutcome::NeedsConfirmation { reason: remove_broken_impact(d) },
        DiscrepancyKind::MissingTarget => FixOutcome::NeedsConfirmation {
            reason: format!("将重新部署缺失的目标 {}", d.path.display()),
        },
        DiscrepancyKind::UnrecordedDeployment => FixOutcome::NeedsConfirmation {
            reason: format!("将补登记部署记录 {}", d.path.display()),
        },
        DiscrepancyKind::CopyDrift => FixOutcome::NeedsConfirmation { reason: update_copy_impact(d) },
    }
}

/// 危险项确认门：非 `UserConfirmed` 一律 `NeedsConfirmation`
/// （reason = 影响面预览，即 dry-run）；确认后放行。
fn confirm_gate(req: &FixRequest<'_>, impact: String) -> Option<FixOutcome> {
    (!matches!(req.policy, ReplacePolicy::UserConfirmed))
        .then_some(FixOutcome::NeedsConfirmation { reason: impact })
}

fn adopt_impact(d: &Discrepancy) -> String {
    let paths = d
        .affected_paths()
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "将收编真实目录 {} 到中央库，并在原位放置链接或副本。影响路径：{paths}。此为危险项，需 UserConfirmed 逐个确认后重试",
        d.path.display()
    )
}

fn remove_link_impact(d: &Discrepancy) -> String {
    format!(
        "将把链接 {}（指向中央库之外）移入 trash，链接目标本身不动。此为危险项，需 UserConfirmed 逐个确认后重试",
        d.path.display()
    )
}

fn remove_broken_impact(d: &Discrepancy) -> String {
    format!(
        "将把悬空链接 {} 移入 trash（链接无数据，目标本就不存在）。此为危险项，需 UserConfirmed 逐个确认后重试",
        d.path.display()
    )
}

fn update_copy_impact(d: &Discrepancy) -> String {
    format!(
        "将用中央库正本覆盖 copy 副本 {}，当前副本内容移入 trash（可找回）。此为危险项，需 UserConfirmed 逐个确认后重试",
        d.path.display()
    )
}

// ---------------------------------------------------------------------------
// 执行器
// ---------------------------------------------------------------------------

/// adopt：真实目录整体搬入中央库，原位重建链接（回退 tracked copy），
/// 再登记索引。仅 `UserConfirmed` 后可达（confirm_gate）。
fn adopt(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    let Some(store) = req.index_store.as_ref() else {
        return FixOutcome::Skipped { reason: "收编需要索引登记（index_store 未提供）".to_string() };
    };
    let fs = req.ctx.fs.as_ref();
    // 执行期复查：仍必须是真实目录（TOCTOU）。
    let Ok(meta) = fs.symlink_metadata(&d.path) else {
        return FixOutcome::Skipped { reason: "state_changed：目标已不存在".to_string() };
    };
    if meta.kind != EntryKind::Dir {
        return FixOutcome::Skipped {
            reason: format!("state_changed：目标已不是真实目录（{:?}）", meta.kind),
        };
    }
    // 读 SKILL.md 头部解析 frontmatter（§7 约束 1：截断 8 KB）。
    let head = match fs.read_head(&d.path.join(SKILL_FILE), SKILL_HEAD_LIMIT) {
        Ok(bytes) => bytes,
        Err(e) => return FixOutcome::Failed { reason: format!("无法读取 SKILL.md: {e}") },
    };
    let (fm_name, description) = parse_frontmatter_head(&head);
    let dir_name = d
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // frontmatter name 优先；缺失回退目录名。不合规一律失败，不静默改名。
    let name = fm_name.unwrap_or(dir_name);
    if !valid_skill_name(&name) {
        return FixOutcome::Failed {
            reason: format!("skill 名不合规（需 ^[a-z0-9][a-z0-9-]{{1,63}}$）: {name}"),
        };
    }
    let dest = req.ctx.central_root.join(&name);
    if fs.exists(&dest) {
        return FixOutcome::Failed {
            reason: format!("中央库已存在同名目录: {}", dest.display()),
        };
    }

    let _mute = MuteGuard::new(req.watcher.as_ref());
    if let Err(e) = fs.create_dir_all(&req.ctx.central_root) {
        return FixOutcome::Failed { reason: format!("创建中央库目录失败: {e}") };
    }
    // 阶段一：整体搬入中央库（同卷 rename；失败零写入）。
    if let Err(e) = fs.rename(&d.path, &dest) {
        return FixOutcome::Failed { reason: format!("移入中央库失败（原目录未动）: {e}") };
    }
    // 阶段二：原位重建（链接优先，回退 tracked copy）。
    let tmp = match fresh_tmp(fs, req.ctx.clock.as_ref(), &d.path) {
        Ok(t) => t,
        Err(e) => return rollback_adopt(req, &dest, &d.path, format!("创建临时路径失败: {e}")),
    };
    let mode = match stage_link(req.ctx.linker.as_ref(), &dest, &tmp) {
        Ok(()) => SyncMode::Symlink,
        Err(_) => match stage_copy(fs, req.ctx.clock.as_ref(), &dest, &tmp) {
            Ok(()) => SyncMode::Copy,
            Err(e) => {
                cleanup_tmp(fs, req.ctx.linker.as_ref(), &tmp);
                return rollback_adopt(req, &dest, &d.path, format!("原位重建准备失败: {e}"));
            }
        },
    };
    if let Err(e) = fs.rename(&tmp, &d.path) {
        cleanup_tmp(fs, req.ctx.linker.as_ref(), &tmp);
        return rollback_adopt(req, &dest, &d.path, format!("原位重建提交失败: {e}"));
    }

    // 阶段三：登记索引。磁盘已收编；登记失败保留磁盘成果（磁盘优先，K4），
    // 报 Failed 让 doctor 下轮提示补登记。
    let content_hash =
        hash_dir(fs, req.ctx.linker.as_ref(), req.ctx.hasher.as_ref(), &dest);
    let skill_id = match store.insert_skill(
        &name,
        description.as_deref().unwrap_or(""),
        &skillbox_ports::OriginInfo::local(),
        &dest,
        content_hash.as_deref(),
    ) {
        Ok(id) => id,
        Err(e) => {
            return FixOutcome::Failed {
                reason: format!("收编完成但索引登记失败（磁盘已收编，doctor 将提示补登记）: {e}"),
            }
        }
    };
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    let source_hash = if mode == SyncMode::Copy { content_hash.as_deref() } else { None };
    if let Err(e) = store.upsert_target(skill_id, &tool, &d.path, mode, source_hash) {
        return FixOutcome::Failed { reason: format!("磁盘已收编，但部署记录登记失败: {e}") };
    }
    FixOutcome::Applied
}

/// adopt 失败回滚：收编目录搬回原位；原位被占用则留在中央库并说明。
fn rollback_adopt(req: &FixRequest<'_>, dest: &Path, original: &Path, reason: String) -> FixOutcome {
    let fs = req.ctx.fs.as_ref();
    let mut out = reason;
    if fs.exists(original) {
        out.push_str(&format!(
            "；原位已被占用，收编目录保留在中央库: {}",
            dest.display()
        ));
    } else {
        match fs.rename(dest, original) {
            Ok(()) => out.push_str("；已回滚原位"),
            Err(re) => out.push_str(&format!(
                "；回滚失败，收编目录保留在中央库: {}（{re}）",
                dest.display()
            )),
        }
    }
    FixOutcome::Failed { reason: out }
}

/// 悬空链接重建：复查仍悬空 → 正本必须存在 → 移走空链接 → NoClobber
/// 向 Absent 部署（窗口内出现真实内容会被拒绝）→ 刷新记录。
fn rebuild_link(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    let Some(skill_id) = d.skill_id else {
        return FixOutcome::Skipped { reason: "无法定位对应正本（差异缺少 skill_id）".to_string() };
    };
    let Some(skill) = find_skill(&req.ctx.index, skill_id) else {
        return FixOutcome::Skipped { reason: "索引中无该 skill，无法重建".to_string() };
    };
    let fs = req.ctx.fs.as_ref();
    // 执行期复查：仍必须是悬空链接（TOCTOU）。
    let Ok(kind) = req.ctx.linker.link_kind(&d.path) else {
        return FixOutcome::Skipped { reason: "state_changed：无法判定链接状态".to_string() };
    };
    if !matches!(kind, LinkKind::Symlink | LinkKind::Junction) || fs.exists(&d.path) {
        return FixOutcome::Skipped { reason: "state_changed：目标已不是悬空链接".to_string() };
    }
    // 正本必须存在，否则不碰悬空链接（宁可留给人工处理）。
    if !fs.exists(&skill.canonical_path) {
        return FixOutcome::Failed {
            reason: format!(
                "中央库正本不存在，拒绝重建（悬空链接未动）: {}",
                skill.canonical_path.display()
            ),
        };
    }
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    // 空链接不含数据：先整体入 trash，再向 Absent 路径 NoClobber 部署。
    {
        let _mute = MuteGuard::new(req.watcher.as_ref());
        if let Err(e) = move_to_trash(fs, req.ctx.clock.as_ref(), &req.trash_root, &d.path) {
            return FixOutcome::Failed { reason: format!("悬空链接移入 trash 失败（未动）: {e}") };
        }
    }
    let report = deploy(DeployRequest {
        skill_id,
        source: skill.canonical_path.clone(),
        targets: vec![DeployTarget { tool_id: tool, path: d.path.clone(), mode: SyncMode::Symlink }],
        // NoClobber：重建窗口内出现的任何真实内容（RealDir/RealFile/外链）
        // 都会被拒绝——不与用户数据竞争。
        policy: ReplacePolicy::NoClobber,
        trash_root: req.trash_root.clone(),
        fs: req.ctx.fs.clone(),
        clock: req.ctx.clock.clone(),
        linker: req.ctx.linker.clone(),
        watcher: req.watcher.clone(),
    });
    let (outcome, mode) = map_deploy(report);
    match outcome {
        FixOutcome::Applied => refresh_record(req, d, skill_id, mode),
        other => other,
    }
}

/// 链接移入 trash（ForeignLink / 悬空链接共用）。
/// `must_be_broken`：执行期复查链接悬空状态是否仍与 reconcile 一致。
fn remove_link_to_trash(req: &FixRequest<'_>, d: &Discrepancy, must_be_broken: bool) -> FixOutcome {
    let fs = req.ctx.fs.as_ref();
    let Ok(kind) = req.ctx.linker.link_kind(&d.path) else {
        return FixOutcome::Skipped { reason: "state_changed：无法判定链接状态".to_string() };
    };
    if matches!(kind, LinkKind::NotALink) {
        return FixOutcome::Skipped { reason: "state_changed：目标已不是链接".to_string() };
    }
    let broken = !fs.exists(&d.path);
    if broken != must_be_broken {
        return FixOutcome::Skipped { reason: "state_changed：链接状态已变化".to_string() };
    }
    let _mute = MuteGuard::new(req.watcher.as_ref());
    match move_to_trash(fs, req.ctx.clock.as_ref(), &req.trash_root, &d.path) {
        Ok(_) => FixOutcome::Applied,
        Err(e) => FixOutcome::Failed { reason: format!("移入 trash 失败: {e}") },
    }
}

/// MissingTarget：按记录模式重新部署（策略随请求）。
fn redeploy(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    let Some(skill_id) = d.skill_id else {
        return FixOutcome::Failed { reason: "内部错误：MissingTarget 缺少 skill_id".to_string() };
    };
    let Some(skill) = find_skill(&req.ctx.index, skill_id) else {
        return FixOutcome::Failed { reason: "索引中无该 skill，无法重新部署".to_string() };
    };
    let Some(record) = find_record(&req.ctx.index, d) else {
        return FixOutcome::Failed { reason: "索引中无该部署记录，无法重新部署".to_string() };
    };
    // copy 模式部署后必须刷新 source_hash；无索引存储时提前跳过（零写入）。
    if record.mode == SyncMode::Copy && req.index_store.is_none() {
        return FixOutcome::Skipped {
            reason: "copy 模式重部署需刷新记录 source_hash，未提供索引存储".to_string(),
        };
    }
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    let report = deploy(DeployRequest {
        skill_id,
        source: skill.canonical_path.clone(),
        targets: vec![DeployTarget { tool_id: tool, path: d.path.clone(), mode: record.mode }],
        policy: req.policy.clone(),
        trash_root: req.trash_root.clone(),
        fs: req.ctx.fs.clone(),
        clock: req.ctx.clock.clone(),
        linker: req.ctx.linker.clone(),
        watcher: req.watcher.clone(),
    });
    let (outcome, mode) = map_deploy(report);
    match outcome {
        FixOutcome::Applied => refresh_record(req, d, skill_id, mode),
        other => other,
    }
}

/// UnrecordedDeployment：补登记 target 记录（不动磁盘）。
fn register_record(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    let Some(store) = req.index_store.as_ref() else {
        return FixOutcome::Skipped { reason: "补登记需要索引存储（index_store 未提供）".to_string() };
    };
    let Some(skill_id) = d.skill_id else {
        return FixOutcome::Skipped {
            reason: "链接指向中央库但索引中无对应 skill，需先收编或入库".to_string(),
        };
    };
    // 执行期复查：仍是链接（TOCTOU）。
    let Ok(kind) = req.ctx.linker.link_kind(&d.path) else {
        return FixOutcome::Skipped { reason: "state_changed：无法判定链接状态".to_string() };
    };
    if !matches!(kind, LinkKind::Symlink | LinkKind::Junction) {
        return FixOutcome::Skipped { reason: "state_changed：目标已不是链接".to_string() };
    }
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    match store.upsert_target(skill_id, &tool, &d.path, SyncMode::Symlink, None) {
        Ok(()) => FixOutcome::Applied,
        Err(e) => FixOutcome::Failed { reason: format!("补登记失败: {e}") },
    }
}

/// CopyDrift：从正本更新副本（仅 UserConfirmed 后可达）+ 刷新 source_hash。
fn update_copy(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
    let Some(skill_id) = d.skill_id else {
        return FixOutcome::Failed { reason: "内部错误：CopyDrift 缺少 skill_id".to_string() };
    };
    let Some(skill) = find_skill(&req.ctx.index, skill_id) else {
        return FixOutcome::Failed { reason: "索引中无该 skill，无法更新副本".to_string() };
    };
    let Some(record) = find_record(&req.ctx.index, d) else {
        return FixOutcome::Failed { reason: "索引中无该部署记录，无法更新副本".to_string() };
    };
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    let report = deploy(DeployRequest {
        skill_id,
        source: skill.canonical_path.clone(),
        targets: vec![DeployTarget { tool_id: tool, path: d.path.clone(), mode: record.mode }],
        // confirm_gate 后必为 UserConfirmed（授权覆盖当前副本目录）。
        policy: req.policy.clone(),
        trash_root: req.trash_root.clone(),
        fs: req.ctx.fs.clone(),
        clock: req.ctx.clock.clone(),
        linker: req.ctx.linker.clone(),
        watcher: req.watcher.clone(),
    });
    let (outcome, mode) = map_deploy(report);
    match outcome {
        FixOutcome::Applied => refresh_record(req, d, skill_id, mode),
        other => other,
    }
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn tool_of(d: &Discrepancy) -> Result<ToolId, FixOutcome> {
    d.tool_id.clone().ok_or_else(|| {
        FixOutcome::Failed { reason: "内部错误：该差异缺少 tool_id".to_string() }
    })
}

fn find_skill(index: &IndexView, id: SkillId) -> Option<&SkillRecord> {
    index.skills.iter().find(|s| s.id == id)
}

fn find_record<'a>(index: &'a IndexView, d: &Discrepancy) -> Option<&'a TargetRecord> {
    let tool_id = d.tool_id.as_ref()?;
    let skill_id = d.skill_id?;
    index
        .records
        .iter()
        .find(|r| r.skill_id == skill_id && &r.tool_id == tool_id && r.path == d.path)
}

/// `^[a-z0-9][a-z0-9-]{1,63}$`（§9.3）：总长 2..=64，首字符小写字母/数字，
/// 其余小写字母/数字/连字符。install / doc 校验共用。
pub(crate) fn valid_skill_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 2 || bytes.len() > 64 {
        return false;
    }
    let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    first_ok
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// 部署结果 → fix 结果。返回 `(outcome, 实际部署模式)`；
/// `AlreadyUpToDate` 视为已修复（无需刷新记录）。
fn map_deploy(report: AppResult<DeployReport>) -> (FixOutcome, Option<SyncMode>) {
    match report {
        Ok(r) => match r.results.into_iter().next() {
            Some(tr) => match tr.outcome {
                ToolDeployOutcome::Deployed { mode } => (FixOutcome::Applied, Some(mode)),
                ToolDeployOutcome::AlreadyUpToDate => (FixOutcome::Applied, None),
                ToolDeployOutcome::NeedsConfirmation { reason } => {
                    (FixOutcome::NeedsConfirmation { reason }, None)
                }
                ToolDeployOutcome::Failed { reason } => (FixOutcome::Failed { reason }, None),
            },
            None => (FixOutcome::Failed { reason: "部署报告为空（内部错误）".to_string() }, None),
        },
        // 授权拒绝（如窗口内出现真实内容）：交回确认门语义。
        Err(e) if e.kind == ErrorKind::TargetConflict => {
            let mut reason = format!("重新部署被拒绝: {e}");
            if let Some(details) = &e.details {
                for c in &details.conflicts {
                    reason.push_str(&format!("\n- {}: {}", c.path.display(), c.reason));
                }
            }
            reason.push_str("\n需 UserConfirmed 授权后重试");
            (FixOutcome::NeedsConfirmation { reason }, None)
        }
        Err(e) => (FixOutcome::Failed { reason: format!("重新部署失败: {e}") }, None),
    }
}

/// 部署成功后刷新 target 记录（copy 模式必须刷新 source_hash；
/// 无存储时 copy 报 Failed——磁盘已修复，记录与磁盘脱节的缺口交给 doctor）。
fn refresh_record(
    req: &FixRequest<'_>,
    d: &Discrepancy,
    skill_id: SkillId,
    mode: Option<SyncMode>,
) -> FixOutcome {
    // AlreadyUpToDate：磁盘与记录本就一致，无需刷新。
    let Some(mode) = mode else { return FixOutcome::Applied };
    let Some(store) = req.index_store.as_ref() else {
        if mode == SyncMode::Copy {
            return FixOutcome::Failed {
                reason: "部署为 copy 模式，需刷新记录 source_hash，但未提供索引存储".to_string(),
            };
        }
        return FixOutcome::Applied;
    };
    let tool = match tool_of(d) {
        Ok(t) => t,
        Err(o) => return o,
    };
    let source_hash = if mode == SyncMode::Copy {
        let Some(skill) = find_skill(&req.ctx.index, skill_id) else {
            return FixOutcome::Failed { reason: "无法定位正本以刷新 source_hash".to_string() };
        };
        let Some(hash) = hash_dir(
            req.ctx.fs.as_ref(),
            req.ctx.linker.as_ref(),
            req.ctx.hasher.as_ref(),
            &skill.canonical_path,
        ) else {
            return FixOutcome::Failed { reason: "正本内容哈希计算失败，未能刷新记录".to_string() };
        };
        Some(hash)
    } else {
        None
    };
    match store.upsert_target(skill_id, &tool, &d.path, mode, source_hash.as_deref()) {
        Ok(()) => FixOutcome::Applied,
        Err(e) => FixOutcome::Failed { reason: format!("磁盘已修复，但记录刷新失败: {e}") },
    }
}

#[cfg(test)]
mod tests;
