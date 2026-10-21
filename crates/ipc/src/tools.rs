//! tools_list / tools_set_enabled（§8）。
//!
//! 禁用状态持久化在 settings `tools.disabled`（JSON id 数组）；
//! 被禁用的工具不参与扫描与体检（engine::env 过滤）。

use std::collections::HashSet;

use serde::Serialize;
use skillbox_domain::{AppError, AppResult, ToolId};
use skillbox_engine::{tools as env_tools, PathScope, ToolInfo, ToolOverrides};
use tauri::State;

use crate::state::AppState;

/// `tools_list` 条目：环境快照（engine）+ 启用状态（settings）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDto {
    #[serde(flatten)]
    pub info: ToolInfo,
    pub enabled: bool,
}

fn env_snapshot(state: &AppState) -> Vec<ToolInfo> {
    env_tools(state.fs.as_ref(), &PathScope::current(), &ToolOverrides::new())
}

#[tauri::command]
pub fn tools_list(state: State<'_, AppState>) -> AppResult<Vec<ToolDto>> {
    let disabled: HashSet<String> = state.disabled_tools()?.into_iter().map(|t| t.0).collect();
    Ok(env_snapshot(&state)
        .into_iter()
        .map(|info| {
            let enabled = !disabled.contains(&info.id.0);
            ToolDto { info, enabled }
        })
        .collect())
}

#[tauri::command]
pub fn tools_set_enabled(
    state: State<'_, AppState>,
    tool_id: String,
    enabled: bool,
    dry_run: bool,
) -> AppResult<()> {
    let id = ToolId(tool_id);
    if !env_snapshot(&state).iter().any(|t| t.id == id) {
        return Err(AppError::not_found(format!("未知工具: {id}")));
    }
    let mut disabled = state.disabled_tools()?;
    if enabled {
        disabled.retain(|t| *t != id);
    } else if !disabled.contains(&id) {
        disabled.push(id);
    }
    if dry_run {
        return Ok(());
    }
    state.set_disabled_tools(&disabled)
}
