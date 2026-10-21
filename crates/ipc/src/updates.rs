//! updates_check / updates_apply（§8；Phase 4 实装，先占位）。

use skillbox_domain::{AppError, AppResult};

#[tauri::command]
pub fn updates_check() -> AppResult<()> {
    Err(AppError::internal("尚未实现：更新检查（Phase 4）"))
}

#[tauri::command]
pub fn updates_apply(dry_run: bool) -> AppResult<()> {
    let _ = dry_run;
    Err(AppError::internal("尚未实现：应用更新（Phase 4）"))
}
