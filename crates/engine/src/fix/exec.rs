//! fix —— 执行器（从 `fix.rs` 拆分，单文件 ≤600 行约束）。
//!
//! 执行期复查（TOCTOU 防护）是每个执行器的入口纪律：绝不按 doctor
//! 快照动盘；状态已变化 → `Skipped`，绝不强推。

use std::path::Path;

use skillbox_domain::{AppResult, ErrorKind, ReplacePolicy, SkillId, SyncMode, ToolId};
use skillbox_ports::{EntryKind, LinkKind};

use crate::deploy::write::{cleanup_tmp, fresh_tmp, move_to_trash, stage_copy, stage_link, MuteGuard};
use crate::deploy::{deploy, DeployRequest, DeployTarget};
use crate::doctor::hash_dir;
use crate::scan::{parse_frontmatter_head, SKILL_FILE, SKILL_HEAD_LIMIT};
use crate::types::{
    DeployReport, Discrepancy, DiscrepancyKind, FixOutcome, IndexView, SkillRecord, SuggestedFix,
    TargetRecord, ToolDeployOutcome,
};

use super::{
    adopt_impact, confirm_gate, remove_broken_impact, remove_link_impact, update_copy_impact,
    valid_skill_name, FixRequest,
};

pub(super) fn execute_item(req: &FixRequest<'_>, d: &Discrepancy) -> FixOutcome {
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
