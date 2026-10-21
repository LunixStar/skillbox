//! updates_check / updates_apply（§8 / §9.4 溯源·更新）。
//!
//! 命令 = 读索引行 → 组装 `UpdateTarget` → 调 engine::update。
//! git 来源：重新克隆比对 revision，应用前展示 diff 摘要；
//! 本地来源："重新导入"刷新自身登记；压缩包：v1 不支持（诚实上报）。

use skillbox_domain::{AppError, AppResult, SkillId};
use skillbox_engine::{
    update_apply, update_check, UpdateCheckOutcome, UpdateContext, UpdateOutcome, UpdateTarget,
};
use skillbox_ports::OriginInfo;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn updates_check(
    state: State<'_, AppState>,
    skill_id: SkillId,
) -> AppResult<UpdateCheckOutcome> {
    let target = load_target(&state, skill_id)?;
    let ctx: UpdateContext = state.update_context();
    update_check(&target, &ctx)
}

#[tauri::command]
pub fn updates_apply(
    state: State<'_, AppState>,
    skill_id: SkillId,
    dry_run: bool,
) -> AppResult<UpdateOutcome> {
    let target = load_target(&state, skill_id)?;
    let ctx: UpdateContext = state.update_context();
    update_apply(&target, dry_run, &ctx)
}

/// 索引行 → engine `UpdateTarget`（ipc 读侧转换）。
fn load_target(state: &AppState, skill_id: SkillId) -> AppResult<UpdateTarget> {
    let meta = state
        .store
        .get_skill(skill_id)?
        .ok_or_else(|| AppError::not_found(format!("skill #{} 不存在", skill_id.0)))?;
    let origin = OriginInfo {
        kind: meta.origin_kind,
        ref_path: meta.origin_ref,
        subpath: meta.origin_subpath,
        revision: meta.origin_revision,
    };
    Ok(UpdateTarget {
        skill_id: meta.id,
        name: meta.name,
        description: meta.description,
        origin,
        canonical_path: meta.canonical_path,
        content_hash: meta.content_hash,
    })
}
