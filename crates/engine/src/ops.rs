//! ops —— ipc 编排层（TASKBOOK §8 deploy / library_remove 组的引擎入口）。
//!
//! 职责：把"读侧快照（IndexView）+ 写侧端口（IndexStore）+ 盘上原语
//! （deploy / undeploy / trash）"组装成面向命令的完整流程，路径拼接与
//! 状态判定在本层完成，ipc 只做 DTO 转换（分层规则：状态判定下沉引擎）。
//!
//! - [`deploy_to_tools`]：部署到一组工具（覆盖授权 / dry_run 预览 /
//!   成功后刷新索引 target 记录，source_hash 约定同 fix：copy 记
//!   content_hash，symlink 记 None）。
//! - [`revoke`]：按工具取消部署（空列表 = 全部记录）；移除成功的
//!   记录同步从索引清理；正本不动。
//! - [`remove_skill`]：`Undeploy`（仅撤部署）/ `Delete`（撤部署 +
//!   正本入 trash + 索引行清理）。Delete 遇 undeploy 失败立即中止，
//!   正本保持原位。
//! - [`deploy_set_mode`]：切换单工具部署模式（先校验现状属于我们
//!   ——Absent / 指向本源的链接 / 带标记副本——再以用户确认级授权重部署）。
//!
//! 所有预览（dry_run）零写盘、零索引写入。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{
    AppError, AppResult, PathConflict, ReplacePolicy, SkillId, SyncMode, TargetState, ToolId,
};
use skillbox_ports::{Clock, FileSystem, IndexStore, Linker, WatchMute};

use crate::deploy::classify::{classify_target, is_our_copy};
use crate::deploy::policy::{conflict_reason, overwrite_authorized};
use crate::deploy::write::{move_to_trash, validate_source};
use crate::deploy::{
    deploy, undeploy, DeployRequest, DeployTarget, ToolUndeployOutcome, UndeployReport,
    UndeployRequest, UndeployTarget,
};
use crate::types::{
    DeployReport, IndexView, SkillRecord, TargetRecord, ToolDeployOutcome, ToolDeployResult,
};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// 上下文 / 参数
// ---------------------------------------------------------------------------

/// 编排上下文：端口与目录注入（不可序列化，同 `InstallContext` 约定）。
pub struct OpsContext {
    pub central_root: PathBuf,
    pub trash_root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    pub index: Arc<dyn IndexStore>,
    pub watcher: Option<Arc<dyn WatchMute>>,
}

/// 单个工具的部署目录（`deploy_to_tools` 入参）。
#[derive(Debug, Clone)]
pub struct ToolDeployDir {
    pub tool_id: ToolId,
    /// 工具全局 skills 根；目标路径 = `dir / <skill-name>`。
    pub dir: PathBuf,
}

/// `remove_skill` 模式：`Undeploy` 仅撤部署（正本与索引行保留）；
/// `Delete` 撤部署 + 正本入 trash + 索引行清理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoveMode {
    Undeploy,
    Delete,
}

// ---------------------------------------------------------------------------
// 结果类型
// ---------------------------------------------------------------------------

/// 部署结果：真实执行（`DeployReport`）或 dry_run 预览。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeployOutcome {
    Applied(DeployReport),
    Preview(DeployPreview),
}

/// dry_run 部署预览：逐工具给出将要发生的动作（零写盘）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployPreview {
    pub skill_id: SkillId,
    pub results: Vec<ToolDeployPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDeployPreview {
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub action: DeployAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeployAction {
    /// 将部署 / 替换为该模式。
    Deploy { mode: SyncMode },
    /// 已是指向本源的链接且请求链接模式，无需动作。
    AlreadyUpToDate,
    /// 未授权覆盖（需 overwrite = true 重试）。
    Conflict { reason: String },
}

/// 取消部署结果：真实执行或 dry_run 预览。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevokeOutcome {
    Applied(RevokeReport),
    Preview(RevokePreview),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeReport {
    pub undeploy: UndeployReport,
    /// 索引中已清理记录的工具（Removed / already_absent 的）。
    pub tools_cleared: Vec<ToolId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokePreview {
    pub skill_id: SkillId,
    pub results: Vec<ToolRevokePreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRevokePreview {
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub action: RevokeAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevokeAction {
    /// 将移入 trash（指向本源的链接 / 带标记副本）。
    Remove,
    /// 目标本就不存在（幂等终态），仅清理索引记录。
    AlreadyAbsent,
    /// 拒绝自动处理（用户数据），需手工确认。
    Conflict { reason: String },
}

/// 移除 skill 结果：真实执行或 dry_run 预览。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoveSkillOutcome {
    Applied(RemoveSkillReport),
    Preview(RemoveSkillPreview),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSkillReport {
    pub undeploy: UndeployReport,
    pub tools_cleared: Vec<ToolId>,
    /// Delete 模式：正本移入 trash 后的路径（正本本就不存在时为 None）。
    pub central_trashed_to: Option<PathBuf>,
    /// Delete 模式为 true（索引行已清理）。
    pub index_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSkillPreview {
    pub revoke: RevokePreview,
    pub central_path: PathBuf,
    /// true = Delete 模式（正本将入 trash）。
    pub will_delete_central: bool,
    /// true = Delete 模式（索引行将清理）。
    pub will_delete_index: bool,
}

/// 模式切换结果：真实执行或 dry_run 预览。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetModeOutcome {
    Applied(DeployReport),
    Preview(SetModePreview),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModePreview {
    pub skill_id: SkillId,
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub from: SyncMode,
    pub to: SyncMode,
}

// ---------------------------------------------------------------------------
// deploy_to_tools
// ---------------------------------------------------------------------------

/// 部署 skill 到一组工具。`overwrite = true` 时以用户确认级授权覆盖
/// （ForeignLink / RealDir / RealFile）；默认 NoClobber，冲突以
/// `TargetConflict` 整体拒绝（零写入）。dry_run 只预览。
pub fn deploy_to_tools(
    skill_id: SkillId,
    dirs: &[ToolDeployDir],
    mode: SyncMode,
    overwrite: bool,
    dry_run: bool,
    index: &IndexView,
    ctx: &OpsContext,
) -> AppResult<DeployOutcome> {
    let skill = find_skill(index, skill_id)?;
    validate_source(ctx.fs.as_ref(), &skill.canonical_path)?;
    let policy = policy_of(overwrite);
    let targets: Vec<DeployTarget> = dirs
        .iter()
        .map(|d| DeployTarget {
            tool_id: d.tool_id.clone(),
            path: d.dir.join(&skill.name),
            mode,
        })
        .collect();

    if dry_run {
        let mut results = Vec::with_capacity(targets.len());
        for target in &targets {
            let state = classify_target(
                ctx.fs.as_ref(),
                ctx.linker.as_ref(),
                &skill.canonical_path,
                &target.path,
            )?;
            let action = if state == TargetState::LinkToSource && mode == SyncMode::Symlink {
                DeployAction::AlreadyUpToDate
            } else if overwrite_authorized(state, &policy) {
                DeployAction::Deploy { mode }
            } else {
                DeployAction::Conflict { reason: conflict_reason(state) }
            };
            results.push(ToolDeployPreview {
                tool_id: target.tool_id.clone(),
                path: target.path.clone(),
                action,
            });
        }
        return Ok(DeployOutcome::Preview(DeployPreview { skill_id, results }));
    }

    let report = deploy(DeployRequest {
        skill_id,
        source: skill.canonical_path.clone(),
        targets,
        policy,
        trash_root: ctx.trash_root.clone(),
        fs: ctx.fs.clone(),
        clock: ctx.clock.clone(),
        linker: ctx.linker.clone(),
        watcher: ctx.watcher.clone(),
    })?;
    refresh_records(skill_id, &skill.content_hash, &report.results, ctx.index.as_ref())?;
    Ok(DeployOutcome::Applied(report))
}

// ---------------------------------------------------------------------------
// revoke
// ---------------------------------------------------------------------------

/// 取消部署：`tools` 为空 = 全部记录工具。移除成功（含 already_absent）
/// 的工具记录同步从索引清理；正本不动（取消部署 ≠ 删除，§10）。
pub fn revoke(
    skill_id: SkillId,
    tools: &[ToolId],
    dry_run: bool,
    index: &IndexView,
    ctx: &OpsContext,
) -> AppResult<RevokeOutcome> {
    let skill = find_skill(index, skill_id)?;
    let records = records_of(index, skill_id, tools);
    if dry_run {
        let mut results = Vec::with_capacity(records.len());
        for r in &records {
            results.push(ToolRevokePreview {
                tool_id: r.tool_id.clone(),
                path: r.path.clone(),
                action: revoke_action(ctx, &skill.canonical_path, r)?,
            });
        }
        return Ok(RevokeOutcome::Preview(RevokePreview { skill_id, results }));
    }

    let report = undeploy_targets(skill_id, &skill.canonical_path, &records, ctx)?;
    let mut tools_cleared = Vec::new();
    for result in &report.results {
        if is_clearable(&result.outcome) {
            ctx.index.delete_target(skill_id, &result.tool_id)?;
            tools_cleared.push(result.tool_id.clone());
        }
    }
    Ok(RevokeOutcome::Applied(RevokeReport { undeploy: report, tools_cleared }))
}

// ---------------------------------------------------------------------------
// remove_skill
// ---------------------------------------------------------------------------

/// 移除 skill（`library_remove`）：`Undeploy` / `Delete` 两级语义（§10）。
/// Delete 在任一 undeploy 失败时中止（正本与索引行原样保留）。
pub fn remove_skill(
    skill_id: SkillId,
    mode: RemoveMode,
    dry_run: bool,
    index: &IndexView,
    ctx: &OpsContext,
) -> AppResult<RemoveSkillOutcome> {
    let skill = find_skill(index, skill_id)?;
    let records = records_of(index, skill_id, &[]);
    let will_delete = mode == RemoveMode::Delete;
    if dry_run {
        let mut results = Vec::with_capacity(records.len());
        for r in &records {
            results.push(ToolRevokePreview {
                tool_id: r.tool_id.clone(),
                path: r.path.clone(),
                action: revoke_action(ctx, &skill.canonical_path, r)?,
            });
        }
        return Ok(RemoveSkillOutcome::Preview(RemoveSkillPreview {
            revoke: RevokePreview { skill_id, results },
            central_path: skill.canonical_path.clone(),
            will_delete_central: will_delete,
            will_delete_index: will_delete,
        }));
    }

    let report = undeploy_targets(skill_id, &skill.canonical_path, &records, ctx)?;
    let mut tools_cleared = Vec::new();
    for result in &report.results {
        if is_clearable(&result.outcome) {
            tools_cleared.push(result.tool_id.clone());
        }
    }
    if will_delete && report.results.iter().any(|r| is_failure(&r.outcome)) {
        // 中止 Delete：不碰正本、不删索引行（已成功的 undeploy 保持）。
        return Err(AppError::internal(
            "部分工具取消部署失败，已中止删除（正本未动，可重试）".to_string(),
        ));
    }

    let mut central_trashed_to = None;
    if will_delete {
        for tool in &tools_cleared {
            ctx.index.delete_target(skill_id, tool)?;
        }
        if ctx.fs.exists(&skill.canonical_path) {
            let dest = move_to_trash(
                ctx.fs.as_ref(),
                ctx.clock.as_ref(),
                &ctx.trash_root,
                &skill.canonical_path,
            )?;
            central_trashed_to = Some(dest);
        }
        ctx.index.delete_skill(skill_id)?;
        return Ok(RemoveSkillOutcome::Applied(RemoveSkillReport {
            undeploy: report,
            tools_cleared,
            central_trashed_to,
            index_deleted: true,
        }));
    }

    for tool in &tools_cleared {
        ctx.index.delete_target(skill_id, tool)?;
    }
    Ok(RemoveSkillOutcome::Applied(RemoveSkillReport {
        undeploy: report,
        tools_cleared,
        central_trashed_to: None,
        index_deleted: false,
    }))
}

// ---------------------------------------------------------------------------
// deploy_set_mode
// ---------------------------------------------------------------------------

/// 切换单工具部署模式。现状必须属于我们（Absent / 指向本源的链接 /
/// 带标记副本），否则 `TargetConflict`；随后以用户确认级授权重部署
/// （旧目标整体入 trash，§5.3）。
pub fn deploy_set_mode(
    skill_id: SkillId,
    tool: ToolId,
    mode: SyncMode,
    dry_run: bool,
    index: &IndexView,
    ctx: &OpsContext,
) -> AppResult<SetModeOutcome> {
    let skill = find_skill(index, skill_id)?;
    let record = index
        .records
        .iter()
        .find(|r| r.skill_id == skill_id && r.tool_id == tool)
        .ok_or_else(|| {
            AppError::not_found(format!(
                "skill {} 在工具 {tool} 上没有部署记录",
                skill.name
            ))
        })?;
    let state = classify_target(
        ctx.fs.as_ref(),
        ctx.linker.as_ref(),
        &skill.canonical_path,
        &record.path,
    )?;
    match state {
        TargetState::Absent
        | TargetState::LinkToSource => {}
        TargetState::RealDir if is_our_copy(ctx.fs.as_ref(), &skill.canonical_path, &record.path) => {
        }
        _ => {
            return Err(AppError::target_conflict(
                "目标现状不属于本 skill 的部署（可能是用户数据），拒绝切换模式",
                vec![PathConflict {
                    path: record.path.clone(),
                    reason: conflict_reason(state),
                }],
            ));
        }
    }
    if dry_run {
        return Ok(SetModeOutcome::Preview(SetModePreview {
            skill_id,
            tool_id: tool,
            path: record.path.clone(),
            from: record.mode,
            to: mode,
        }));
    }

    let target = DeployTarget { tool_id: tool, path: record.path.clone(), mode };
    let report = deploy(DeployRequest {
        skill_id,
        source: skill.canonical_path.clone(),
        targets: vec![target],
        policy: ReplacePolicy::UserConfirmed,
        trash_root: ctx.trash_root.clone(),
        fs: ctx.fs.clone(),
        clock: ctx.clock.clone(),
        linker: ctx.linker.clone(),
        watcher: ctx.watcher.clone(),
    })?;
    refresh_records(skill_id, &skill.content_hash, &report.results, ctx.index.as_ref())?;
    Ok(SetModeOutcome::Applied(report))
}

// ---------------------------------------------------------------------------
// 内部
// ---------------------------------------------------------------------------

fn find_skill(index: &IndexView, skill_id: SkillId) -> AppResult<&SkillRecord> {
    index
        .skills
        .iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| AppError::not_found(format!("skill 不存在: {skill_id}")))
}

/// 取 skill 的部署记录；`tools` 为空 = 全部。保持索引顺序。
fn records_of(index: &IndexView, skill_id: SkillId, tools: &[ToolId]) -> Vec<TargetRecord> {
    index
        .records
        .iter()
        .filter(|r| {
            r.skill_id == skill_id && (tools.is_empty() || tools.contains(&r.tool_id))
        })
        .cloned()
        .collect()
}

fn policy_of(overwrite: bool) -> ReplacePolicy<'static> {
    if overwrite { ReplacePolicy::UserConfirmed } else { ReplacePolicy::NoClobber }
}

/// 部署成功后刷新索引 target 记录（copy 记 content_hash、symlink 记 None，
/// 约定同 fix）；Failed 的目标不写记录。
fn refresh_records(
    skill_id: SkillId,
    content_hash: &Option<String>,
    results: &[ToolDeployResult],
    index: &dyn IndexStore,
) -> AppResult<()> {
    for r in results {
        let mode = match &r.outcome {
            ToolDeployOutcome::Deployed { mode } => Some(*mode),
            // AlreadyUpToDate 只在「已是指向本源的链接且请求链接模式」出现。
            ToolDeployOutcome::AlreadyUpToDate => Some(SyncMode::Symlink),
            ToolDeployOutcome::Failed { .. } | ToolDeployOutcome::NeedsConfirmation { .. } => None,
        };
        let Some(mode) = mode else { continue };
        let source_hash = if mode == SyncMode::Copy { content_hash.as_deref() } else { None };
        index.upsert_target(skill_id, &r.tool_id, &r.path, mode, source_hash)?;
    }
    Ok(())
}

/// 逐记录预览动作（revoke / remove_skill 的 dry_run 共用）。
fn revoke_action(
    ctx: &OpsContext,
    source: &Path,
    record: &TargetRecord,
) -> AppResult<RevokeAction> {
    let state = classify_target(ctx.fs.as_ref(), ctx.linker.as_ref(), source, &record.path)?;
    Ok(match state {
        TargetState::Absent => RevokeAction::AlreadyAbsent,
        TargetState::LinkToSource => RevokeAction::Remove,
        TargetState::RealDir if is_our_copy(ctx.fs.as_ref(), source, &record.path) => {
            RevokeAction::Remove
        }
        _ => RevokeAction::Conflict { reason: conflict_reason(state) },
    })
}

/// 组装 UndeployRequest 并执行（记录为空时返回空报告，不触盘）。
fn undeploy_targets(
    skill_id: SkillId,
    source: &Path,
    records: &[TargetRecord],
    ctx: &OpsContext,
) -> AppResult<UndeployReport> {
    if records.is_empty() {
        return Ok(UndeployReport {
            skill_id,
            results: Vec::new(),
        });
    }
    undeploy(UndeployRequest {
        skill_id,
        source: source.to_path_buf(),
        targets: records
            .iter()
            .map(|r| UndeployTarget { tool_id: r.tool_id.clone(), path: r.path.clone() })
            .collect(),
        trash_root: ctx.trash_root.clone(),
        fs: ctx.fs.clone(),
        clock: ctx.clock.clone(),
        linker: ctx.linker.clone(),
        watcher: ctx.watcher.clone(),
    })
}

/// undeploy 结果可否清理索引记录（Removed / already_absent）。
fn is_clearable(outcome: &ToolUndeployOutcome) -> bool {
    match outcome {
        ToolUndeployOutcome::Removed => true,
        ToolUndeployOutcome::Skipped { reason } => reason == "already_absent",
        ToolUndeployOutcome::Failed { .. } => false,
    }
}

/// undeploy 结果是否为失败（Delete 模式中止判定）。
fn is_failure(outcome: &ToolUndeployOutcome) -> bool {
    matches!(outcome, ToolUndeployOutcome::Failed { .. })
}
