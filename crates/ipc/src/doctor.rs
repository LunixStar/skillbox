//! doctor_run / doctor_fix（§8）。
//!
//! 体检上下文（目标 = 已安装且未禁用工具）由 engine::env 组装；
//! `doctor_fix` 逐项修复，`dry_run` 逐项预览影响面（零写盘）。

use skillbox_domain::{AppError, AppResult, ReplacePolicy, ToolId};
use skillbox_engine::{
    doctor, fix, health_context, DoctorOpts, DoctorReport, FixReport, FixRequest, IssueId,
    PathScope, Severity, ToolOverrides,
};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn doctor_run(
    state: State<'_, AppState>,
    tools: Option<Vec<String>>,
    min_severity: Option<Severity>,
) -> AppResult<DoctorReport> {
    let ctx = health_context(state.health_inputs()?, &PathScope::current(), &ToolOverrides::new());
    let opts = DoctorOpts {
        ctx,
        tools: tools.map(|ts| ts.into_iter().map(ToolId).collect()),
        min_severity,
    };
    doctor(opts)
}

/// 逐项修复。`items` = doctor 报告的问题 id；`confirmed = true` 才放行
/// 危险项（adopt / ForeignLink 移除 / copy 更新，§6.3）。
#[tauri::command]
pub fn doctor_fix(
    state: State<'_, AppState>,
    items: Vec<String>,
    confirmed: bool,
    dry_run: bool,
) -> AppResult<FixReport> {
    if items.is_empty() {
        return Err(AppError::invalid_input("至少选择一个要修复的问题 id"));
    }
    let ctx = health_context(state.health_inputs()?, &PathScope::current(), &ToolOverrides::new());
    let req = FixRequest {
        items: items.into_iter().map(IssueId).collect(),
        policy: if confirmed { ReplacePolicy::UserConfirmed } else { ReplacePolicy::NoClobber },
        ctx,
        trash_root: state.trash_root.clone(),
        index_store: Some(state.index_store.clone()),
        watcher: None,
        dry_run,
    };
    fix(req)
}
