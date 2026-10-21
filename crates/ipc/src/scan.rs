//! scan_start / scan_cancel / scan_status（§8）。
//!
//! 扫描在工作线程中执行，进度与结果经 Tauri 事件推送：
//! - `scan://progress`（`ScanProgress`，每 50 目录一次）
//! - `scan://done`（`ScanReport`）
//! - `scan://error`（`AppError`）
//!
//! 单活动会话：第二次 `scan_start` 在运行中被拒（InvalidInput）。

use serde::Serialize;
use skillbox_domain::{AppError, AppResult};
use skillbox_engine::{scan, scan_roots, CancellationToken, PathScope, ScanReport, ScanRequest, ToolOverrides};
use std::sync::MutexGuard;
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, ScanSession};

/// 扫描状态快照（`scan_status` 返回）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusInfo {
    pub running: bool,
    pub started_at_ms: Option<i64>,
    pub report: Option<ScanReport>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn scan_start<R: tauri::Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> AppResult<()> {
    let mut session = lock_session(&state)?;
    if session.running {
        return Err(AppError::invalid_input("已有扫描正在进行中"));
    }
    let disabled = state.disabled_tools()?;
    let roots = scan_roots(
        state.fs.as_ref(),
        &PathScope::current(),
        &ToolOverrides::new(),
        &disabled,
        &state.central_root,
    );
    let token = CancellationToken::new();
    session.running = true;
    session.started_at_ms = Some(now_ms());
    session.cancel = Some(token.clone());
    session.report = None;
    session.error = None;
    drop(session);

    let req = ScanRequest::with_prod_ports(roots);
    let session_slot = state.scan.clone();
    std::thread::spawn(move || {
        let result = scan(req, token, |p| {
            let _ = app.emit("scan://progress", &p);
        });
        let mut session = match session_slot.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        session.running = false;
        session.cancel = None;
        match result {
            Ok(report) => {
                session.report = Some(report.clone());
                session.error = None;
                drop(session);
                let _ = app.emit("scan://done", &report);
            }
            Err(e) => {
                session.error = Some(e.to_string());
                drop(session);
                let _ = app.emit("scan://error", &e);
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn scan_cancel(state: State<'_, AppState>) -> AppResult<()> {
    let session = lock_session(&state)?;
    if !session.running {
        return Err(AppError::not_found("当前没有正在进行的扫描"));
    }
    if let Some(token) = &session.cancel {
        token.cancel();
    }
    Ok(())
}

#[tauri::command]
pub fn scan_status(state: State<'_, AppState>) -> AppResult<ScanStatusInfo> {
    let session = lock_session(&state)?;
    Ok(ScanStatusInfo {
        running: session.running,
        started_at_ms: session.started_at_ms,
        report: session.report.clone(),
        error: session.error.clone(),
    })
}

fn lock_session(state: &AppState) -> AppResult<MutexGuard<'_, ScanSession>> {
    state.scan.lock().map_err(|_| AppError::internal("扫描会话锁中毒"))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
