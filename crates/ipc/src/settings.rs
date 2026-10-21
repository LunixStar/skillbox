//! settings_get / settings_set（§8）。
//!
//! 约定 key（v1）：`tools.disabled`（工具禁用列表，`tools_set_enabled` 维护）。
//! 其余 key 由前端自由使用。

use skillbox_domain::AppResult;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>, key: String) -> AppResult<Option<String>> {
    state.store.get_setting(&key)
}

#[tauri::command]
pub fn settings_set(
    state: State<'_, AppState>,
    key: String,
    value: String,
    dry_run: bool,
) -> AppResult<()> {
    if dry_run {
        return Ok(());
    }
    state.store.set_setting(&key, &value)
}
