//! deploy / undeploy 执行（§5）：precheck 全有或全无，执行期逐目标
//! 重分类（TOCTOU 防护），绝不按 precheck 快照删除。

use std::path::Path;

use skillbox_domain::{AppError, AppResult, PathConflict, SyncMode, TargetState};

use crate::types::{DeployReport, ToolDeployOutcome, ToolDeployResult};

use super::classify::{classify_target, is_our_copy};
use super::policy::{conflict_reason, overwrite_authorized};
use super::write::{
    cleanup_tmp, fresh_tmp, move_to_trash, stage_copy, stage_link, validate_source, MuteGuard,
};
use super::{
    DeployRequest, DeployTarget, ToolUndeployOutcome, ToolUndeployResult, UndeployReport,
    UndeployRequest, UndeployTarget,
};

/// 部署：把中央库正本以链接（symlink → junction → tracked copy 回退链）
/// 写入指定 agent 目录（§5）。
///
/// - 授权拒绝全有或全无：冲突在写盘前收集并以 `TargetConflict` 返回，
///   携带逐路径 `conflicts` 清单，本调用零写入。
/// - 逐工具失败只影响该工具（`ToolDeployOutcome::Failed`），其余照常。
pub fn deploy(req: DeployRequest<'_>) -> AppResult<DeployReport> {
    validate_source(req.fs.as_ref(), &req.source)?;

    // 阶段一：分类 + 授权（全部冲突都发生在任何写盘之前）。
    let mut classified = Vec::with_capacity(req.targets.len());
    let mut conflicts = Vec::new();
    for target in &req.targets {
        let state =
            classify_target(req.fs.as_ref(), req.linker.as_ref(), &req.source, &target.path)?;
        if !overwrite_authorized(state, &req.policy) {
            conflicts.push(PathConflict {
                path: target.path.clone(),
                reason: conflict_reason(state),
            });
        }
        classified.push((target, state));
    }
    if !conflicts.is_empty() {
        return Err(AppError::target_conflict(
            "目标路径存在未授权覆盖的内容，已整体拒绝（未做任何写盘）",
            conflicts,
        ));
    }

    // 阶段二：逐目标执行（执行期重分类，见 deploy_one）。
    let mut results = Vec::with_capacity(classified.len());
    for (target, precheck_state) in classified {
        let outcome = deploy_one(&req, target, precheck_state);
        results.push(ToolDeployResult {
            tool_id: target.tool_id.clone(),
            path: target.path.clone(),
            outcome,
        });
    }
    Ok(DeployReport { skill_id: req.skill_id, results })
}

/// 单目标执行：执行期重分类（TOCTOU 防护）→ 幂等检查 → 再授权 →
/// mute → tmp 准备 → 旧目标进 trash → 原子提交。
///
/// precheck 快照只作对照：现状超出授权范围 → `Failed`（不写盘）；
/// 重试会走完整 precheck，对真实冲突给出 `TargetConflict`。
fn deploy_one(
    req: &DeployRequest<'_>,
    target: &DeployTarget,
    precheck_state: TargetState,
) -> ToolDeployOutcome {
    // TOCTOU 防护：写盘前重新分类，以现状为准（§5.2 授权基于现状）。
    let state = match classify_target(
        req.fs.as_ref(),
        req.linker.as_ref(),
        &req.source,
        &target.path,
    ) {
        Ok(s) => s,
        Err(e) => {
            return ToolDeployOutcome::Failed { reason: format!("执行时目标分类失败: {e}") };
        }
    };

    // 幂等：目标已是指向本源的链接且请求的就是链接模式。
    if state == TargetState::LinkToSource && target.mode == SyncMode::Symlink {
        return ToolDeployOutcome::AlreadyUpToDate;
    }

    // precheck 后目标被换成超出授权的内容 → 拒写（不按快照删）。
    if !overwrite_authorized(state, &req.policy) {
        return ToolDeployOutcome::Failed {
            reason: format!(
                "目标状态已变化（state_changed: {precheck_state:?} -> {state:?}），未写入"
            ),
        };
    }

    let _mute = MuteGuard::new(req.watcher.as_ref());

    let tmp = match fresh_tmp(req.fs.as_ref(), req.clock.as_ref(), &target.path) {
        Ok(t) => t,
        Err(e) => return ToolDeployOutcome::Failed { reason: format!("创建临时路径失败: {e}") },
    };

    // 阶段一：全部可失败的重活先落在 tmp 上（旧目标尚未被触动）。
    let staged_mode = match target.mode {
        SyncMode::Symlink => {
            match stage_link(req.linker.as_ref(), &req.source, &tmp) {
                Ok(()) => SyncMode::Symlink,
                Err(_) => {
                    // 回退链末端：tracked copy（§5.2）。
                    match stage_copy(req.fs.as_ref(), req.clock.as_ref(), &req.source, &tmp) {
                        Ok(()) => SyncMode::Copy,
                        Err(e) => {
                            cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                            return ToolDeployOutcome::Failed {
                                reason: format!("部署准备失败: {e}"),
                            };
                        }
                    }
                }
            }
        }
        SyncMode::Copy => match stage_copy(req.fs.as_ref(), req.clock.as_ref(), &req.source, &tmp)
        {
            Ok(()) => SyncMode::Copy,
            Err(e) => {
                cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                return ToolDeployOutcome::Failed { reason: format!("部署准备失败: {e}") };
            }
        },
    };

    // 阶段二：旧目标整体进 trash（v1 无硬删除，§5.3）。
    let trashed = if state == TargetState::Absent {
        None
    } else {
        match move_to_trash(
            req.fs.as_ref(),
            req.clock.as_ref(),
            &req.trash_root,
            &target.path,
        ) {
            Ok(dest) => Some(dest),
            Err(e) => {
                cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                return ToolDeployOutcome::Failed {
                    reason: format!("旧目标暂存 trash 失败（目标未动）: {e}"),
                };
            }
        }
    };

    // 阶段三：同卷原子 rename 提交（§5.3）。
    if let Err(e) = req.fs.rename(&tmp, &target.path) {
        cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
        let mut reason = format!("提交失败: {e}");
        // 已暂存的旧目标尽力回滚到原位（trash 副本保留，双保险）。
        if let Some(dest) = &trashed {
            match req.fs.rename(dest, &target.path) {
                Ok(()) => reason.push_str("；旧目标已回滚原位"),
                Err(re) => reason.push_str(&format!(
                    "；旧目标回滚失败，仍在 trash: {}（{re}）",
                    dest.display()
                )),
            }
        }
        return ToolDeployOutcome::Failed { reason };
    }

    ToolDeployOutcome::Deployed { mode: staged_mode }
}

/// 取消部署：从指定 agent 目录移除指向本源的链接或带来源标记的副本，
/// 正本仍在中央库（取消部署 ≠ 删除 Skill，§10）。
///
/// - 移除范围：指向 `source` 的链接；`.skillbox-source.json` 标记一致的
///   tracked-copy 副本（用户在副本里的改动随副本整体进 trash，可找回）。
/// - `ForeignLink` / `RealFile` / 无标记 `RealDir` 一律 `TargetConflict`
///   拒绝（全有或全无，零写盘）。
/// - 执行期逐目标重分类（TOCTOU 防护）：现状与 precheck 预期不符 →
///   `Skipped { state_changed }`；目标已消失 → `Skipped { already_absent }`
///   （幂等终态）。绝不按 precheck 快照删除。
pub fn undeploy(req: UndeployRequest) -> AppResult<UndeployReport> {
    // 阶段一：分类 + 拒绝判定（全有或全无，写盘前完成）。
    let mut classified = Vec::with_capacity(req.targets.len());
    let mut conflicts = Vec::new();
    for target in &req.targets {
        let state =
            classify_target(req.fs.as_ref(), req.linker.as_ref(), &req.source, &target.path)?;
        match state {
            TargetState::Absent | TargetState::LinkToSource => {}
            TargetState::RealDir => {
                // 带来源标记的 tracked-copy 副本可移除（用户改动随副本
                // 进 trash）；无标记 = 用户目录，拒绝。
                if !is_our_copy(req.fs.as_ref(), &req.source, &target.path) {
                    conflicts.push(PathConflict {
                        path: target.path.clone(),
                        reason: "真实目录且无我们的来源标记（可能含用户数据），拒绝移除"
                            .to_string(),
                    });
                }
            }
            TargetState::ForeignLink | TargetState::RealFile => {
                conflicts.push(PathConflict {
                    path: target.path.clone(),
                    reason: format!(
                        "{}；取消部署只移除指向本源的链接与带标记副本",
                        conflict_reason(state)
                    ),
                });
            }
        }
        classified.push((target, state));
    }
    if !conflicts.is_empty() {
        return Err(AppError::target_conflict(
            "取消部署只允许移除指向本源的链接与带来源标记的副本；以下目标拒绝自动处理（未做任何写盘）",
            conflicts,
        ));
    }

    // 阶段二：逐目标执行（执行期重分类，绝不按 precheck 快照删除）。
    let mut results = Vec::with_capacity(classified.len());
    for (target, expected) in classified {
        let outcome = undeploy_one(&req, target, expected);
        results.push(ToolUndeployResult {
            tool_id: target.tool_id.clone(),
            path: target.path.clone(),
            outcome,
        });
    }
    Ok(UndeployReport { skill_id: req.skill_id, results })
}

/// 单目标执行：执行期重分类，现状与 precheck 预期一致才动盘。
fn undeploy_one(
    req: &UndeployRequest,
    target: &UndeployTarget,
    expected: TargetState,
) -> ToolUndeployOutcome {
    let current = match classify_target(
        req.fs.as_ref(),
        req.linker.as_ref(),
        &req.source,
        &target.path,
    ) {
        Ok(s) => s,
        Err(e) => {
            return ToolUndeployOutcome::Failed { reason: format!("执行时目标分类失败: {e}") };
        }
    };

    // 目标已消失：幂等终态，不算失败（undeploy 可安全重放）。
    if current == TargetState::Absent {
        return ToolUndeployOutcome::Skipped { reason: "already_absent".to_string() };
    }

    // 现状与 precheck 预期不符：绝不按快照删除（TOCTOU 防护）。
    if current != expected {
        return ToolUndeployOutcome::Skipped { reason: "state_changed".to_string() };
    }

    match expected {
        TargetState::LinkToSource => remove_target_to_trash(req, &target.path),
        TargetState::RealDir => {
            // precheck 已验过标记；执行时标记可能又被动过，重验一次。
            if !is_our_copy(req.fs.as_ref(), &req.source, &target.path) {
                return ToolUndeployOutcome::Skipped { reason: "state_changed".to_string() };
            }
            remove_target_to_trash(req, &target.path)
        }
        // 阶段一已排除（Absent 在上面提前返回）；防御性兜底。
        TargetState::Absent | TargetState::ForeignLink | TargetState::RealFile => {
            ToolUndeployOutcome::Failed {
                reason: "内部错误：阶段一后不应出现不可移除状态".to_string(),
            }
        }
    }
}

/// mute → 整体移入 trash → 解除 mute（v1 无硬删除，可找回）。
fn remove_target_to_trash(req: &UndeployRequest, path: &Path) -> ToolUndeployOutcome {
    let _mute = MuteGuard::new(req.watcher.as_ref());
    match move_to_trash(req.fs.as_ref(), req.clock.as_ref(), &req.trash_root, path) {
        Ok(_) => ToolUndeployOutcome::Removed,
        Err(e) => ToolUndeployOutcome::Failed { reason: format!("移入 trash 失败: {e}") },
    }
}
