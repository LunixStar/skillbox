//! # skillbox-ipc
//!
//! Tauri 命令层（TASKBOOK §8）。Phase 0 只建骨架；32 个命令在 Phase 2 实现。
//!
//! 契约：
//! - 只做参数校验 → 调 engine → 序列化，无业务逻辑。
//! - 所有返回结构 `Serialize` + `#[serde(rename_all = "camelCase")]`。
//! - 写操作统一支持 `dry_run: bool`；`TargetConflict` 必须携带冲突路径。
//! - `system.open_path` 只接受后端生成的绝对路径（防路径穿越）。
//! - ipc 不许直接调 adapters 拼路径，必须走 engine（§2.1）。
//!
//! 约定命令面（§8，共 31 个 ≤ 32 上限）：
//! ```text
//! scan    : start / cancel / status                    (3)
//! library : list / get / install_local / install_git / remove   (5)
//! deploy  : apply / revoke / set_mode                  (3)
//! tools   : list / set_enabled                         (2)
//! doctor  : run / fix                                  (2)
//! doc     : read / write / validate_meta               (3)
//! security: scan / report                              (2)
//! cost    : report                                     (1)
//! system  : open_path / logs_export / cache_inspect / cache_clear / trash_list / trash_restore (6)
//! settings: get / set                                  (2)
//! updates : check / apply                              (2)
//! ```

/// 注册全部 IPC 命令（Phase 2 填充；由 app 的 Builder 调用）。
pub fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    // Phase 2：builder.invoke_handler(tauri::generate_handler![...])
    builder
}
