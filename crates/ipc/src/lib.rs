//! # skillbox-ipc
//!
//! Tauri 命令层（TASKBOOK §8）。只做参数校验 → 组装 engine 上下文 →
//! 调 engine → 序列化，无业务逻辑。
//!
//! 契约：
//! - 所有返回结构 `Serialize` + `#[serde(rename_all = "camelCase")]`
//!   （engine / domain 已保证，ipc 只透传或做 camelCase DTO）。
//! - 写操作统一支持 `dry_run: bool`（§8 硬约束）；dry_run 只预览，零写盘。
//! - `TargetConflict` 错误携带 `details.conflicts: [{path, reason}]`。
//! - `system_open_path` 只接受后端生成的路径 id（防路径穿越）。
//! - ipc 不直接调 adapters 拼路径，环境解析一律走 engine（§2.1）。
//!
//! 命令面（§8，共 32 个 = 上限）：
//! ```text
//! scan    : start / cancel / status                    (3)
//! library : list / targets / get / install_local / install_git / remove
//!           / doc_read / doc_write / doc_validate      (9)
//! deploy  : apply / revoke / set_mode                  (3)
//! tools   : list / set_enabled                         (2)
//! doctor  : run / fix                                  (2)
//! panels  : security_scan / security_report / cost_report (3)
//! system  : open_path / logs_export / cache_inspect / cache_clear
//!           / trash_list / trash_restore               (6)
//! settings: get / set                                  (2)
//! updates : check / apply                              (2)
//! ```
//!
//! 占位命令（Phase 4 实装，结构化"尚未实现"错误）：
//! `security_scan` / `security_report` / `cost_report` / `system_logs_export`
//! / `updates_check` / `updates_apply`。

mod deploy;
mod doctor;
mod library;
mod panels;
mod scan;
mod settings;
mod state;
mod system;
mod tools;
mod updates;

pub use state::{AppState, AppStateInputs};

// app（组合根）经 ipc 取环境类型，保持 app 只依赖 ipc（§2.1 依赖方向）。
pub use skillbox_engine::PathScope;

/// 注册全部 IPC 命令（由 app 的 Builder 调用）。
/// 调用前 app 必须已 `manage(AppState)`。
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        // scan（3）
        scan::scan_start,
        scan::scan_cancel,
        scan::scan_status,
        // library（9）
        library::library_list,
        library::library_targets,
        library::library_get,
        library::library_install_local,
        library::library_install_git,
        library::library_remove,
        library::library_doc_read,
        library::library_doc_write,
        library::library_doc_validate,
        // deploy（3）
        deploy::deploy_apply,
        deploy::deploy_revoke,
        deploy::deploy_set_mode,
        // tools（2）
        tools::tools_list,
        tools::tools_set_enabled,
        // doctor（2）
        doctor::doctor_run,
        doctor::doctor_fix,
        // 专业面板（3）
        panels::security_scan,
        panels::security_report,
        panels::cost_report,
        // system（6）
        system::system_open_path,
        system::system_logs_export,
        system::system_cache_inspect,
        system::system_cache_clear,
        system::system_trash_list,
        system::system_trash_restore,
        // settings（2）
        settings::settings_get,
        settings::settings_set,
        // updates（2）
        updates::updates_check,
        updates::updates_apply,
    ])
}
