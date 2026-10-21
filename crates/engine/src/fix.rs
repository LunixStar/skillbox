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
//!   （TOCTOU 防护），绝不按快照动盘（执行器见 `fix/exec.rs`）。
//! - 每次删除先入 trash（§5.3，v1 无硬删除）；写盘期间 mute 监听（RAII）。
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

mod exec;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use skillbox_domain::{AppResult, ReplacePolicy};
use skillbox_ports::{IndexStore, WatchMute};

use crate::doctor::{inspect_disk, HealthContext};
use crate::reconcile::reconcile;
use crate::types::{
    Discrepancy, DiscrepancyKind, FixOutcome, FixReport, FixResult, IssueId, SuggestedFix,
};

use exec::execute_item;

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
// dry-run 预览与确认门
// ---------------------------------------------------------------------------

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
// 名称校验（install / doc 共用）
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests;
