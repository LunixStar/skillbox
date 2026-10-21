//! system_*：路径打开 / 缓存 / 回收站（§8，6 个命令）。
//!
//! 【硬约束】`system_open_path` 只接受后端生成的路径 id
//! （`central` / `trash` / `cache` / `logs` / `skill:<id>` / `tool:<id>`），
//! 前端不得自由传路径（防路径穿越）。

use std::path::{Path, PathBuf};

use skillbox_domain::{AppError, AppResult, SkillId, ToolId};
use skillbox_engine::{
    cache_clear, cache_inspect, tool_deploy_dir, trash_list, trash_restore, CacheClearReport,
    CacheEntry, PathScope, TrashEntry, TrashRestoreOutcome, ToolOverrides,
};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn system_open_path(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let path = resolve_path_id(&state, &id)?;
    if !state.fs.exists(&path) {
        return Err(AppError::not_found(format!("路径不存在: {}", path.display())));
    }
    open_in_file_manager(&path)
}

#[tauri::command]
pub fn system_logs_export() -> AppResult<()> {
    Err(AppError::internal("尚未实现：日志导出（Phase 4）"))
}

#[tauri::command]
pub fn system_cache_inspect(state: State<'_, AppState>) -> AppResult<Vec<CacheEntry>> {
    cache_inspect(state.fs.as_ref(), &state.cache_root)
}

/// `names = None` 清全部；指定则只清列出的条目（任一不存在整体拒绝）。
#[tauri::command]
pub fn system_cache_clear(
    state: State<'_, AppState>,
    names: Option<Vec<String>>,
    dry_run: bool,
) -> AppResult<CacheClearReport> {
    cache_clear(
        state.fs.as_ref(),
        state.linker.as_ref(),
        None,
        &state.cache_root,
        names.as_deref(),
        dry_run,
    )
}

#[tauri::command]
pub fn system_trash_list(state: State<'_, AppState>) -> AppResult<Vec<TrashEntry>> {
    trash_list(state.fs.as_ref(), &state.trash_root)
}

/// 恢复 trash 条目；v1 仅支持恢复到中央库（`dest = "central"`）。
#[tauri::command]
pub fn system_trash_restore(
    state: State<'_, AppState>,
    slot: String,
    name: String,
    dest: String,
    dry_run: bool,
) -> AppResult<TrashRestoreOutcome> {
    if dest != "central" {
        return Err(AppError::invalid_input("v1 仅支持恢复到中央库（dest = \"central\"）"));
    }
    trash_restore(
        state.fs.as_ref(),
        None,
        &state.trash_root,
        &slot,
        &name,
        &state.central_root,
        dry_run,
    )
}

/// 路径 id → 绝对路径（只接受后端生成的 id，§8 硬约束）。
fn resolve_path_id(state: &AppState, id: &str) -> AppResult<PathBuf> {
    match id {
        "central" => return Ok(state.central_root.clone()),
        "trash" => return Ok(state.trash_root.clone()),
        "cache" => return Ok(state.cache_root.clone()),
        "logs" => return Ok(state.logs_dir.clone()),
        _ => {}
    }
    if let Some(skill) = id.strip_prefix("skill:") {
        let sid: i64 = skill
            .parse()
            .map_err(|_| AppError::invalid_input(format!("非法的 skill id: {skill:?}")))?;
        let meta = state
            .store
            .get_skill(SkillId(sid))?
            .ok_or_else(|| AppError::not_found(format!("skill #{sid} 不存在")))?;
        return Ok(meta.canonical_path);
    }
    if let Some(tool) = id.strip_prefix("tool:") {
        if tool.is_empty() {
            return Err(AppError::invalid_input("路径 id 缺少工具名: \"tool:\""));
        }
        return tool_deploy_dir(
            &ToolId(tool.to_string()),
            state.fs.as_ref(),
            &PathScope::current(),
            &ToolOverrides::new(),
        );
    }
    Err(AppError::invalid_input(format!(
        "未知的路径 id: {id:?}（只接受 central / trash / cache / logs / skill:<id> / tool:<id>）"
    )))
}

/// 用系统文件管理器打开目录（跨平台）。
fn open_in_file_manager(path: &Path) -> AppResult<()> {
    let spawned = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(path).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(path).spawn()
    };
    spawned
        .map(|_| ())
        .map_err(|e| AppError::io(format!("打开路径失败: {e}")))
}
