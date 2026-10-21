//! library_*：中央库读写（§8，8 个命令）。
//!
//! 写命令全部支持 `dry_run`（§8 硬约束）：dry_run 只预览影响面，零写盘。

use std::path::PathBuf;

use skillbox_domain::{AppError, AppResult, SkillId, SkillMeta};
use skillbox_engine::{
    doc_read, doc_write, install, remove_skill, validate_meta, DocCheckReport, DocWriteOutcome,
    InstallOutcome, InstallRequest, InstallSource, RemoveMode, RemoveSkillOutcome,
};
use skillbox_store::TargetRow;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn library_list(state: State<'_, AppState>) -> AppResult<Vec<SkillMeta>> {
    state.store.list_skills()
}

/// 全部部署意图记录（skill → agent 角标"已部署"状态的数据源）。
#[tauri::command]
pub fn library_targets(state: State<'_, AppState>) -> AppResult<Vec<TargetRow>> {
    state.store.list_targets()
}

#[tauri::command]
pub fn library_get(state: State<'_, AppState>, skill_id: SkillId) -> AppResult<SkillMeta> {
    state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{skill_id} 不存在")))
}

#[tauri::command]
pub fn library_install_local(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
    keep_in_place: bool,
    dry_run: bool,
) -> AppResult<InstallOutcome> {
    let req = InstallRequest {
        source: InstallSource::Local { path: PathBuf::from(path) },
        name,
        keep_in_place,
    };
    install(req, dry_run, &state.install_context())
}

#[tauri::command]
pub fn library_install_git(
    state: State<'_, AppState>,
    url: String,
    subpath: Option<String>,
    revision: Option<String>,
    name: Option<String>,
    dry_run: bool,
) -> AppResult<InstallOutcome> {
    let req = InstallRequest {
        source: InstallSource::Git { url, subpath, revision },
        name,
        keep_in_place: false,
    };
    install(req, dry_run, &state.install_context())
}

/// `library_remove`（§8：remove 需 mode: undeploy | delete）。
#[tauri::command]
pub fn library_remove(
    state: State<'_, AppState>,
    skill_id: SkillId,
    mode: RemoveMode,
    dry_run: bool,
) -> AppResult<RemoveSkillOutcome> {
    let index = state.index_view()?;
    let ctx = state.ops_context();
    remove_skill(skill_id, mode, dry_run, &index, &ctx)
}

#[tauri::command]
pub fn library_doc_read(state: State<'_, AppState>, skill_id: SkillId) -> AppResult<String> {
    let dir = skill_dir(&state, skill_id)?;
    doc_read(state.fs.as_ref(), &dir)
}

#[tauri::command]
pub fn library_doc_write(
    state: State<'_, AppState>,
    skill_id: SkillId,
    contents: String,
    dry_run: bool,
) -> AppResult<DocWriteOutcome> {
    let dir = skill_dir(&state, skill_id)?;
    doc_write(state.fs.as_ref(), None, &dir, &contents, dry_run)
}

#[tauri::command]
pub fn library_doc_validate(
    state: State<'_, AppState>,
    skill_id: SkillId,
) -> AppResult<DocCheckReport> {
    let dir = skill_dir(&state, skill_id)?;
    validate_meta(state.fs.as_ref(), &dir)
}

/// skill 正本目录（store 记录解析；doc 命令共用）。
fn skill_dir(state: &AppState, skill_id: SkillId) -> AppResult<PathBuf> {
    Ok(state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{skill_id} 不存在")))?
        .canonical_path)
}
