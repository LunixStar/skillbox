//! library_*：中央库读写（§8，8 个命令）。
//!
//! 写命令全部支持 `dry_run`（§8 硬约束）：dry_run 只预览影响面，零写盘。

use std::path::PathBuf;

use skillbox_domain::{AppError, AppResult, SkillId, SkillMeta};
use skillbox_engine::{
    doc_read, doc_write, install, remove_skill, rescan_after_doc_save, validate_meta, validate_text,
    DocCheckReport, DocWriteOutcome, InstallOutcome, InstallRequest, InstallSource, RemoveMode,
    RemoveSkillOutcome,
};
use skillbox_ports::OriginInfo;
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

/// 编辑保存（`.bak` keep 1；错误拒绝写入，告警随结果返回）。
/// 保存成功后触发该 skill 的**增量重扫**（description / content_hash 刷索引，
/// 不做全量扫描）；重扫失败不影响保存结果，经 `indexError` 说明。
#[tauri::command]
pub fn library_doc_write(
    state: State<'_, AppState>,
    skill_id: SkillId,
    contents: String,
    dry_run: bool,
) -> AppResult<DocWriteOutcome> {
    let meta = skill_meta(&state, skill_id)?;
    let dir = meta.canonical_path.clone();
    let mut outcome = doc_write(
        state.fs.as_ref(),
        None,
        &dir,
        &contents,
        Some(&meta.name),
        dry_run,
    )?;
    if !dry_run {
        let origin = meta_origin(&meta);
        match rescan_after_doc_save(
            state.fs.as_ref(),
            state.linker.as_ref(),
            state.hasher.as_ref(),
            state.index_store.as_ref(),
            skill_id,
            &dir,
            &origin,
        ) {
            Ok(()) => outcome.index_refreshed = true,
            Err(err) => outcome.index_error = Some(err.message),
        }
    }
    Ok(outcome)
}

/// 元数据校验：`contents` 给定时校验编辑器中的文本（实时校验，零写盘），
/// 否则校验盘上内容。结果带行号（行内标记用）。
#[tauri::command]
pub fn library_doc_validate(
    state: State<'_, AppState>,
    skill_id: SkillId,
    contents: Option<String>,
) -> AppResult<DocCheckReport> {
    let meta = skill_meta(&state, skill_id)?;
    let dir = meta.canonical_path;
    match contents {
        Some(text) => Ok(validate_text(state.fs.as_ref(), &dir, &text, Some(&meta.name))),
        None => validate_meta(state.fs.as_ref(), &dir, Some(&meta.name)),
    }
}

/// skill 正本目录（store 记录解析；doc 命令共用）。
fn skill_dir(state: &AppState, skill_id: SkillId) -> AppResult<PathBuf> {
    Ok(skill_meta(state, skill_id)?.canonical_path)
}

/// skill 完整元数据（doc 校验需要登记名做 name_mismatch 告警）。
fn skill_meta(state: &AppState, skill_id: SkillId) -> AppResult<SkillMeta> {
    state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{skill_id} 不存在")))
}

/// DB 行 → engine 来源信息（增量重扫原样回写，来源不因编辑改变）。
fn meta_origin(meta: &SkillMeta) -> OriginInfo {
    OriginInfo {
        kind: meta.origin_kind,
        ref_path: meta.origin_ref.clone(),
        subpath: meta.origin_subpath.clone(),
        revision: meta.origin_revision.clone(),
    }
}
