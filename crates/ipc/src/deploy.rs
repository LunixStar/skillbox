//! deploy_apply / deploy_revoke / deploy_set_mode（§8）。
//!
//! 工具 id → 部署目录的解析经 engine::env（ipc 不拼路径，§2.1）。

use skillbox_domain::{AppError, AppResult, SkillId, SyncMode, ToolId};
use skillbox_engine::{
    deploy_set_mode as engine_set_mode, deploy_to_tools, revoke, tool_deploy_dir, DeployOutcome,
    PathScope, RevokeOutcome, SetModeOutcome, ToolDeployDir, ToolOverrides,
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
    let tool = ToolId(tool);
    let index = state.index_view()?;
    let ctx = state.ops_context();
    engine_set_mode(skill_id, tool, mode, dry_run, &index, &ctx)
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
