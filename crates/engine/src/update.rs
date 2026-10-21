//! update —— skill 更新（TASKBOOK §9.4 溯源/更新）。
//!
//! - git 来源：重新克隆 → 取远端 HEAD → 与索引记录的 revision 比对；
//!   有更新时给出 diff 摘要（更新前预览）。应用 = 新内容 tmp 暂存 →
//!   旧正本整体移入 trash → 同卷原子 rename 提交 → 刷新索引行
//!   （description / origin_revision / content_hash；名称与正本位置不变）。
//!   符号链接目标随正本自动生效；copy 目标会滞后（doctor 的 CopyDrift
//!   负责，诚实上报不隐瞒）。
//! - 本地来源："重新导入"语义 = 重算正本内容哈希 + 重读 frontmatter
//!   description，与索引记录比对；应用 = 刷新索引元数据
//!   （`OriginInfo::local` 不记来源路径，本地更新即刷新自身登记）。
//! - 压缩包来源：v1 未实现，`Unsupported`（明确上报，不 todo!() 崩溃）。
//! - dry_run：完整验证与预览，**零写盘、零索引写入**（克隆到 cache 的
//!   读侧拉取除外，结束后即清理）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppError, AppResult, OriginKind, SkillId};
use skillbox_ports::{Clock, FileSystem, GitClient, Hasher, IndexStore, Linker, OriginInfo, WatchMute};

use crate::deploy::write::{cleanup_tmp, fresh_tmp, move_to_trash, MuteGuard};
use crate::doctor::hash_dir;
use crate::install::{read_frontmatter, validate_source_dir};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// 上下文 / 结果
// ---------------------------------------------------------------------------

/// 更新上下文：端口与目录注入（不可序列化，同 `InstallContext` 约定）。
pub struct UpdateContext {
    /// 中央库 skills 根（`~/.skillbox/skills`）。
    pub central_root: PathBuf,
    /// cache 根：git 克隆落点（用后即清）。
    pub cache_root: PathBuf,
    /// trash 根：旧正本整体移入（v1 无硬删除）。
    pub trash_root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub git: Arc<dyn GitClient>,
    pub hasher: Arc<dyn Hasher>,
    pub linker: Arc<dyn Linker>,
    pub index: Arc<dyn IndexStore>,
    pub watcher: Option<Arc<dyn WatchMute>>,
}

/// 待更新的 skill 行快照（ipc 从 skills 表读出组装；engine 不读库）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTarget {
    pub skill_id: SkillId,
    pub name: String,
    pub description: String,
    pub origin: OriginInfo,
    pub canonical_path: PathBuf,
    pub content_hash: Option<String>,
}

/// 检查结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatus {
    /// 已是最新（无更新可做）。
    UpToDate,
    /// 有更新可用（git 远端新 revision / 本地正本与索引不一致）。
    UpdateAvailable,
    /// 该来源不支持更新（v1：压缩包）。
    Unsupported,
}

/// 更新检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckOutcome {
    pub status: UpdateStatus,
    /// 索引记录的当前 revision（git）；其他来源为 None。
    pub current_revision: Option<String>,
    /// 远端最新 revision（git）；其他来源为 None。
    pub new_revision: Option<String>,
    /// 两个 revision 间的 diff 摘要（更新前预览，§9.4）；不可得时 None。
    pub diff_summary: Option<String>,
    /// 补充说明（Unsupported 原因 / 无法比对的原因等）。
    pub note: Option<String>,
}

/// 更新应用报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    /// 应用后的来源 revision（git）；本地来源为 None。
    pub revision: Option<String>,
    /// diff 摘要（dry_run 预览用；实跑为 None）。
    pub diff_summary: Option<String>,
}

/// 更新应用结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateOutcome {
    /// 已应用（正本已换新 / 索引已刷新）。
    Applied(UpdateReport),
    /// 已是最新，无事可做（未做任何写盘）。
    UpToDate,
    /// dry_run 预览（验证通过，未写盘未写索引）。
    Preview(UpdateReport),
    /// 该来源不支持更新。
    Unsupported { reason: String },
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 更新检查：只读远端与索引比对，零写盘。
pub fn update_check(target: &UpdateTarget, ctx: &UpdateContext) -> AppResult<UpdateCheckOutcome> {
    match target.origin.kind {
        OriginKind::Archive => Ok(UpdateCheckOutcome {
            status: UpdateStatus::Unsupported,
            current_revision: None,
            new_revision: None,
            diff_summary: None,
            note: Some("压缩包来源暂不支持更新（v1 仅支持 git 与本地来源）".to_string()),
        }),
        OriginKind::Local => check_local(target, ctx),
        OriginKind::Git => check_git(target, ctx),
    }
}

/// 更新应用：git 换正本 + 刷索引；本地"重新导入"刷索引。
/// dry_run = 完整验证与预览，零写盘零索引写入。
pub fn update_apply(
    target: &UpdateTarget,
    dry_run: bool,
    ctx: &UpdateContext,
) -> AppResult<UpdateOutcome> {
    match target.origin.kind {
        OriginKind::Archive => Ok(UpdateOutcome::Unsupported {
            reason: "压缩包来源暂不支持更新（v1 仅支持 git 与本地来源）".to_string(),
        }),
        OriginKind::Local => apply_local(target, dry_run, ctx),
        OriginKind::Git => apply_git(target, dry_run, ctx),
    }
}

// ---------------------------------------------------------------------------
// 本地来源：重新导入语义
// ---------------------------------------------------------------------------

fn check_local(target: &UpdateTarget, ctx: &UpdateContext) -> AppResult<UpdateCheckOutcome> {
    validate_source_dir(ctx.fs.as_ref(), &target.canonical_path, "本地正本")?;
    let fresh = fresh_state(target, ctx);
    let changed = fresh
        .as_ref()
        .is_none_or(|s| s.content_hash != target.content_hash || s.description != target.description);
    Ok(UpdateCheckOutcome {
        status: if changed { UpdateStatus::UpdateAvailable } else { UpdateStatus::UpToDate },
        current_revision: None,
        new_revision: None,
        diff_summary: None,
        note: if changed { Some("本地正本与索引记录不一致，可重新导入刷新登记".to_string()) } else { None },
    })
}

fn apply_local(target: &UpdateTarget, dry_run: bool, ctx: &UpdateContext) -> AppResult<UpdateOutcome> {
    validate_source_dir(ctx.fs.as_ref(), &target.canonical_path, "本地正本")?;
    let Some(state) = fresh_state(target, ctx) else {
        return Err(AppError::io("无法读取正本内容哈希".to_string()));
    };
    if state.content_hash == target.content_hash && state.description == target.description {
        return Ok(UpdateOutcome::UpToDate);
    }
    if dry_run {
        return Ok(UpdateOutcome::Preview(UpdateReport { revision: None, diff_summary: None }));
    }
    ctx.index.refresh_skill(
        target.skill_id,
        &state.description,
        &OriginInfo::local(),
        state.content_hash.as_deref(),
    )?;
    Ok(UpdateOutcome::Applied(UpdateReport { revision: None, diff_summary: None }))
}

/// 正本现场状态：内容哈希 + frontmatter description。
struct LocalState {
    content_hash: Option<String>,
    description: String,
}

fn fresh_state(target: &UpdateTarget, ctx: &UpdateContext) -> Option<LocalState> {
    let (_, description) = read_frontmatter(ctx.fs.as_ref(), &target.canonical_path);
    let content_hash = hash_dir(
        ctx.fs.as_ref(),
        ctx.linker.as_ref(),
        ctx.hasher.as_ref(),
        &target.canonical_path,
    );
    Some(LocalState { content_hash, description })
}

// ---------------------------------------------------------------------------
// git 来源：重新克隆 → 比对 revision → diff 摘要 / 换正本
// ---------------------------------------------------------------------------

fn check_git(target: &UpdateTarget, ctx: &UpdateContext) -> AppResult<UpdateCheckOutcome> {
    let url = git_url(target)?;
    let Some((head, clone_dir)) = clone_head(url, ctx)? else {
        return Ok(UpdateCheckOutcome {
            status: UpdateStatus::Unsupported,
            current_revision: None,
            new_revision: None,
            diff_summary: None,
            note: Some("git 不可用（未安装或不在 PATH）".to_string()),
        });
    };
    let outcome = check_git_head(target, &head, &clone_dir, ctx);
    cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
    outcome
}

/// 克隆就绪后的检查后半程（HEAD 已取得；克隆体由调用方清理）。
fn check_git_head(
    target: &UpdateTarget,
    head: &str,
    clone_dir: &Path,
    ctx: &UpdateContext,
) -> AppResult<UpdateCheckOutcome> {
    let current = target.origin.revision.clone();
    let (status, diff, note) = match &current {
        None => (
            UpdateStatus::UpdateAvailable,
            None,
            Some("索引未记录来源 revision，无法证明已是最新".to_string()),
        ),
        Some(cur) if cur == head => (UpdateStatus::UpToDate, None, None),
        Some(cur) => (
            UpdateStatus::UpdateAvailable,
            GitClient::diff_summary(ctx.git.as_ref(), clone_dir, cur, head).ok(),
            None,
        ),
    };
    Ok(UpdateCheckOutcome {
        status,
        current_revision: current,
        new_revision: Some(head.to_string()),
        diff_summary: diff,
        note,
    })
}

fn apply_git(target: &UpdateTarget, dry_run: bool, ctx: &UpdateContext) -> AppResult<UpdateOutcome> {
    let url = git_url(target)?;
    let Some((head, clone_dir)) = clone_head(url, ctx)? else {
        return Ok(unsupported_git());
    };
    let result = apply_git_cloned(target, url, &head, &clone_dir, dry_run, ctx);
    cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
    result
}

/// 克隆完成后的后半程：定位子路径 → 验证 → 预览或换正本 + 刷索引。
fn apply_git_cloned(
    target: &UpdateTarget,
    url: &str,
    head: &str,
    clone_dir: &Path,
    dry_run: bool,
    ctx: &UpdateContext,
) -> AppResult<UpdateOutcome> {
    if target.origin.revision.as_deref() == Some(head) {
        return Ok(UpdateOutcome::UpToDate);
    }
    let repo_dir = match target.origin.subpath.as_deref() {
        Some(sp) => {
            let d = clone_dir.join(sp);
            if !ctx.fs.exists(&d) {
                return Err(AppError::invalid_input(format!("仓库内不存在子路径: {sp}")));
            }
            d
        }
        None => clone_dir.to_path_buf(),
    };
    validate_source_dir(ctx.fs.as_ref(), &repo_dir, "git 来源")?;
    let (_, description) = read_frontmatter(ctx.fs.as_ref(), &repo_dir);
    let diff = match target.origin.revision.as_deref() {
        Some(cur) => GitClient::diff_summary(ctx.git.as_ref(), clone_dir, cur, head).ok(),
        None => None,
    };
    if dry_run {
        return Ok(UpdateOutcome::Preview(UpdateReport {
            revision: Some(head.to_string()),
            diff_summary: diff,
        }));
    }

    // 写入纪律（§5.3 同款）：tmp 暂存 → 旧正本入 trash → 同卷原子 rename。
    let _mute = MuteGuard::new(ctx.watcher.as_ref());
    let canonical = &target.canonical_path;
    let tmp = fresh_tmp(ctx.fs.as_ref(), ctx.clock.as_ref(), canonical)?;
    if let Err(e) = ctx.fs.copy_dir_all(&repo_dir, &tmp) {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
        return Err(e);
    }
    let trashed = match move_to_trash(ctx.fs.as_ref(), ctx.clock.as_ref(), &ctx.trash_root, canonical)
    {
        Ok(dest) => dest,
        Err(e) => {
            cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
            return Err(e);
        }
    };
    if let Err(e) = ctx.fs.rename(&tmp, canonical) {
        // 提交失败：尽力把旧正本从 trash 拉回原位（绝不丢数据）。
        if ctx.fs.rename(&trashed, canonical).is_err() {
            return Err(AppError::io(format!(
                "更新提交失败且回滚失败（数据未丢失）：新内容暂存于 {}，旧正本在 trash {}：{e}",
                tmp.display(),
                trashed.display()
            )));
        }
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
        return Err(AppError::io(format!("更新提交失败，旧正本已恢复原位：{e}")));
    }

    let content_hash =
        hash_dir(ctx.fs.as_ref(), ctx.linker.as_ref(), ctx.hasher.as_ref(), canonical);
    let origin = OriginInfo::git(url, target.origin.subpath.clone(), Some(head.to_string()));
    ctx.index
        .refresh_skill(target.skill_id, &description, &origin, content_hash.as_deref())?;
    Ok(UpdateOutcome::Applied(UpdateReport { revision: Some(head.to_string()), diff_summary: None }))
}

fn git_url(target: &UpdateTarget) -> AppResult<&str> {
    let url = target
        .origin
        .ref_path
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .ok_or_else(|| AppError::invalid_input("git 来源缺少仓库 URL"))?;
    Ok(url)
}

/// 克隆到 cache 临时目录并取 HEAD。`Ok(None)` = git 不可用（Unsupported）。
/// 调用方负责用后清理返回的克隆目录。
fn clone_head(url: &str, ctx: &UpdateContext) -> AppResult<Option<(String, PathBuf)>> {
    if !ctx.git.is_available() {
        return Ok(None);
    }
    ctx.fs.create_dir_all(&ctx.cache_root)?;
    let clone_dir =
        fresh_tmp(ctx.fs.as_ref(), ctx.clock.as_ref(), &ctx.cache_root.join("update"))?;
    if let Err(e) = GitClient::clone(ctx.git.as_ref(), url, &clone_dir, None) {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
        return Err(e);
    }
    match ctx.git.head_revision(&clone_dir) {
        Ok(head) => Ok(Some((head, clone_dir))),
        Err(e) => {
            cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
            Err(e)
        }
    }
}

fn unsupported_git() -> UpdateOutcome {
    UpdateOutcome::Unsupported { reason: "git 不可用（未安装或不在 PATH）".to_string() }
}
