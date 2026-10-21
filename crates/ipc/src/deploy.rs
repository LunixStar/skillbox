//! deploy_apply / deploy_revoke / deploy_set_mode（§8）。
//!
//! 工具 id → 部署目录的解析经 engine::env（ipc 不拼路径，§2.1）。

use skillbox_domain::{AppError, AppResult, SkillId, SyncMode, ToolId};
use skillbox_engine::{
    deploy_set_mode as engine_set_mode, deploy_to_tools, is_read_only_source, revoke,
    tool_deploy_dir, DeployOutcome, PathScope, RevokeOutcome, SetModeOutcome, ToolDeployDir,
    ToolOverrides,
};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn deploy_apply(
    state: State<'_, AppState>,
    skill_id: SkillId,
    tools: Vec<String>,
    mode: SyncMode,
    overwrite: bool,
    dry_run: bool,
) -> AppResult<DeployOutcome> {
    reject_read_only(&state, skill_id)?;
    let dirs = resolve_dirs(&state, &tools)?;
    let index = state.index_view()?;
    let ctx = state.ops_context();
    deploy_to_tools(skill_id, &dirs, mode, overwrite, dry_run, &index, &ctx)
}

/// `tools` 为空 = 全部已记录工具（引擎语义）。
#[tauri::command]
pub fn deploy_revoke(
    state: State<'_, AppState>,
    skill_id: SkillId,
    tools: Vec<String>,
    dry_run: bool,
) -> AppResult<RevokeOutcome> {
    let tools: Vec<ToolId> = tools.into_iter().map(ToolId).collect();
    let index = state.index_view()?;
    let ctx = state.ops_context();
    revoke(skill_id, &tools, dry_run, &index, &ctx)
}

#[tauri::command]
pub fn deploy_set_mode(
    state: State<'_, AppState>,
    skill_id: SkillId,
    tool: String,
    mode: SyncMode,
    dry_run: bool,
) -> AppResult<SetModeOutcome> {
    reject_read_only(&state, skill_id)?;
    let tool = ToolId(tool);
    let index = state.index_view()?;
    let ctx = state.ops_context();
    engine_set_mode(skill_id, tool, mode, dry_run, &index, &ctx)
}

/// §4.3 硬约束守卫：`extra_scan_dirs`（插件缓存）内的 skill 只允许发现与
/// 登记，**不允许部署 / 改模式**（`deploy_revoke` 收尾清理不受限）。
fn reject_read_only(state: &AppState, skill_id: SkillId) -> AppResult<()> {
    let meta = state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{} 不存在", skill_id.0)))?;
    if is_read_only_source(&meta.canonical_path, &PathScope::current()) {
        return Err(AppError::invalid_input(
            "该 skill 来自插件缓存（extra_scan_dirs，只读来源），不允许部署",
        ));
    }
    Ok(())
}

/// 工具 id → 部署目录（engine::env 解析；未知工具 / 无全局目录 → 错误）。
fn resolve_dirs(state: &AppState, tools: &[String]) -> AppResult<Vec<ToolDeployDir>> {
    if tools.is_empty() {
        return Err(AppError::invalid_input("至少指定一个目标工具"));
    }
    let scope = PathScope::current();
    let overrides = ToolOverrides::new();
    let mut dirs = Vec::with_capacity(tools.len());
    for tool in tools {
        let tool_id = ToolId(tool.clone());
        let dir = tool_deploy_dir(&tool_id, state.fs.as_ref(), &scope, &overrides)?;
        dirs.push(ToolDeployDir { tool_id, dir });
    }
    Ok(dirs)
}
