//! install —— 入库（TASKBOOK §3 / K5）。
//!
//! 【硬约束（K5）】入库与部署永不合并：本模块只写中央库
//! `~/.skillbox/skills/`，绝不触碰任何 agent 目录（部署走 deploy 模块）。
//!
//! - 名称解析顺序：请求覆盖名 → frontmatter `name` → 来源目录名
//!   （git 为 URL 仓库名）；最终必须满足 `^[a-z0-9][a-z0-9-]{1,63}$`（§9.3）。
//! - keep-in-place（首启登记）：正本留在原位，`canonical_path` 指向首次
//!   发现位置，不复制。
//! - git：克隆到 cache 临时目录，可选 `revision` 检出后记录 HEAD；
//!   `subpath` 定位 monorepo 子目录；克隆体保留在 cache（updates 复用，
//!   cache_clear 可清）。
//! - dry_run：完整验证（来源 / 名称 / 中央库冲突），**零写盘、零索引写入**。
//! - 写入纪律（§5.3 同款）：tmp 暂存 → 同卷原子 rename 提交；写盘期间
//!   mute 监听（RAII）。
//! - Archive：v1 未实现，返回 `InvalidInput` 明确报错（不 todo!() 崩溃）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppError, AppResult, PathConflict, SkillId};
use skillbox_ports::{
    Clock, EntryKind, FileSystem, GitClient, Hasher, IndexStore, Linker, OriginInfo, WatchMute,
};

use crate::deploy::write::{cleanup_tmp, fresh_tmp, MuteGuard};
use crate::doctor::hash_dir;
use crate::fix::valid_skill_name;
use crate::scan::{parse_frontmatter_head, SKILL_FILE, SKILL_HEAD_LIMIT};
use crate::types::{InstallRequest, InstallSource};

// ---------------------------------------------------------------------------
// 上下文 / 结果
// ---------------------------------------------------------------------------

/// 入库上下文：端口与目录注入（不可序列化，同 `ScanRequest` 约定）。
pub struct InstallContext {
    /// 中央库 skills 根（`~/.skillbox/skills`）。
    pub central_root: PathBuf,
    /// cache 根（`~/.skillbox/cache`）：git 克隆落点。
    pub cache_root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub git: Arc<dyn GitClient>,
    pub hasher: Arc<dyn Hasher>,
    pub linker: Arc<dyn Linker>,
    pub index: Arc<dyn IndexStore>,
    pub watcher: Option<Arc<dyn WatchMute>>,
}

/// 入库结果。`dry_run = true` 时 `skill_id` 恒为 None（未写索引）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    /// 新分配的 skill id；dry_run 为 None。
    pub skill_id: Option<SkillId>,
    /// 最终名称（已过 `^[a-z0-9][a-z0-9-]{1,63}$` 校验）。
    pub name: String,
    /// 正本路径（keep-in-place 时为来源原位）。
    pub canonical_path: PathBuf,
    pub dry_run: bool,
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 入库：只写中央库，不碰任何 agent 目录（K5 硬约束）。
pub fn install(req: InstallRequest, dry_run: bool, ctx: &InstallContext) -> AppResult<InstallOutcome> {
    match &req.source {
        InstallSource::Local { path } => install_local(&req, path, dry_run, ctx),
        InstallSource::Git { url, subpath, revision } => {
            install_git(&req, url, subpath.as_deref(), revision.as_deref(), dry_run, ctx)
        }
        InstallSource::Archive { .. } => {
            Err(AppError::invalid_input("暂不支持压缩包安装（v1 仅支持本地目录与 git）"))
        }
    }
}

/// 本地目录入库：keep_in_place = 原位登记；否则复制进中央库。
fn install_local(
    req: &InstallRequest,
    path: &Path,
    dry_run: bool,
    ctx: &InstallContext,
) -> AppResult<InstallOutcome> {
    validate_source_dir(ctx.fs.as_ref(), path, "本地来源")?;
    let (fm_name, description) = read_frontmatter(ctx.fs.as_ref(), path);
    let name = resolve_name(req.name.as_deref(), fm_name, dir_name_of(path)?)?;

    let canonical = if req.keep_in_place {
        path.to_path_buf()
    } else {
        ctx.central_root.join(&name)
    };
    // 复制入库前冲突预检：中央库同名目录已存在 → 整体拒绝（零写盘）。
    if !req.keep_in_place && ctx.fs.exists(&canonical) {
        return Err(AppError::target_conflict(
            "中央库已存在同名 skill，拒绝覆盖（未做任何写盘）",
            vec![PathConflict { path: canonical, reason: "中央库同名目录已存在".to_string() }],
        ));
    }
    if dry_run {
        return Ok(InstallOutcome {
            skill_id: None,
            name,
            canonical_path: canonical,
            dry_run: true,
        });
    }

    let _mute = MuteGuard::new(ctx.watcher.as_ref());
    if !req.keep_in_place {
        // 写入纪律：tmp 暂存 → 同卷原子 rename 提交（§5.3）。
        ctx.fs.create_dir_all(&ctx.central_root)?;
        let tmp = fresh_tmp(ctx.fs.as_ref(), ctx.clock.as_ref(), &canonical)?;
        if let Err(e) = ctx.fs.copy_dir_all(path, &tmp) {
            cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
            return Err(e);
        }
        if let Err(e) = ctx.fs.rename(&tmp, &canonical) {
            cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
            return Err(AppError::io(format!("入库提交失败: {e}")));
        }
    }

    let content_hash =
        hash_dir(ctx.fs.as_ref(), ctx.linker.as_ref(), ctx.hasher.as_ref(), &canonical);
    let skill_id = ctx.index.insert_skill(
        &name,
        &description,
        &OriginInfo::local(),
        &canonical,
        content_hash.as_deref(),
    )?;
    Ok(InstallOutcome { skill_id: Some(skill_id), name, canonical_path: canonical, dry_run: false })
}

/// git 入库：克隆 → （可选检出 revision）→ 复制进中央库 → 记录 origin。
fn install_git(
    req: &InstallRequest,
    url: &str,
    subpath: Option<&str>,
    revision: Option<&str>,
    dry_run: bool,
    ctx: &InstallContext,
) -> AppResult<InstallOutcome> {
    if url.trim().is_empty() {
        return Err(AppError::invalid_input("git url 不能为空"));
    }
    if !ctx.git.is_available() {
        return Err(AppError::git("git 不可用（未安装或不在 PATH）"));
    }
    // dry_run：不克隆不写盘。名称 = 覆盖名 → URL 仓库名（预览值）。
    if dry_run {
        let name = resolve_name(req.name.as_deref(), None, repo_name_of(url)?)?;
        let canonical_path = ctx.central_root.join(&name);
        return Ok(InstallOutcome { skill_id: None, name, canonical_path, dry_run: true });
    }

    // 克隆到 cache（fresh_tmp 保证目录唯一；克隆体保留供 updates 复用）。
    ctx.fs.create_dir_all(&ctx.cache_root)?;
    let clone_dir = fresh_tmp(ctx.fs.as_ref(), ctx.clock.as_ref(), &ctx.cache_root.join("clone"))?;
    // 显式 trait 调用：避免与 `Arc::clone` 同名歧义。
    if let Err(e) = GitClient::clone(ctx.git.as_ref(), url, &clone_dir, None) {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
        return Err(e);
    }
    let git_result = install_git_cloned(req, url, subpath, revision, &clone_dir, ctx);
    if git_result.is_err() {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &clone_dir);
    }
    git_result
}

/// 克隆完成后的后半程：定位子路径 → 检出 → 名称解析 → 复制入库。
/// 失败时由调用方清理 `clone_dir`。
fn install_git_cloned(
    req: &InstallRequest,
    url: &str,
    subpath: Option<&str>,
    revision: Option<&str>,
    clone_dir: &Path,
    ctx: &InstallContext,
) -> AppResult<InstallOutcome> {
    if let Some(rev) = revision {
        ctx.git.checkout(clone_dir, rev)?;
    }
    let head = ctx.git.head_revision(clone_dir)?;
    let repo_dir = match subpath {
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
    let (fm_name, description) = read_frontmatter(ctx.fs.as_ref(), &repo_dir);
    let name = resolve_name(req.name.as_deref(), fm_name, repo_name_of(url)?)?;

    let canonical = ctx.central_root.join(&name);
    if ctx.fs.exists(&canonical) {
        return Err(AppError::target_conflict(
            "中央库已存在同名 skill，拒绝覆盖（未做任何写盘）",
            vec![PathConflict { path: canonical, reason: "中央库同名目录已存在".to_string() }],
        ));
    }

    let _mute = MuteGuard::new(ctx.watcher.as_ref());
    ctx.fs.create_dir_all(&ctx.central_root)?;
    let tmp = fresh_tmp(ctx.fs.as_ref(), ctx.clock.as_ref(), &canonical)?;
    if let Err(e) = ctx.fs.copy_dir_all(&repo_dir, &tmp) {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
        return Err(e);
    }
    if let Err(e) = ctx.fs.rename(&tmp, &canonical) {
        cleanup_tmp(ctx.fs.as_ref(), ctx.linker.as_ref(), &tmp);
        return Err(AppError::io(format!("入库提交失败: {e}")));
    }

    let content_hash =
        hash_dir(ctx.fs.as_ref(), ctx.linker.as_ref(), ctx.hasher.as_ref(), &canonical);
    let origin = OriginInfo::git(url, subpath.map(str::to_string), Some(head));
    let skill_id =
        ctx.index.insert_skill(&name, &description, &origin, &canonical, content_hash.as_deref())?;
    Ok(InstallOutcome { skill_id: Some(skill_id), name, canonical_path: canonical, dry_run: false })
}

// ---------------------------------------------------------------------------
// 共用小工具（pub(crate) 供 doc / system 复用）
// ---------------------------------------------------------------------------

/// 来源目录校验：存在且为真实目录（拒绝链接 / 文件，避免歧义正本）。
pub(crate) fn validate_source_dir(fs: &dyn FileSystem, path: &Path, what: &str) -> AppResult<()> {
    if !fs.exists(path) {
        return Err(AppError::not_found(format!("{what}不存在: {}", path.display())));
    }
    match fs.symlink_metadata(path)?.kind {
        EntryKind::Dir => Ok(()),
        EntryKind::Symlink => Err(AppError::invalid_input(format!(
            "{what}是符号链接，拒绝作为正本: {}",
            path.display()
        ))),
        _ => Err(AppError::invalid_input(format!("{what}不是目录: {}", path.display()))),
    }
}

/// 读 SKILL.md frontmatter：`(frontmatter name, description)`。
/// 读取失败（无 SKILL.md / IO 错误）返回 `(None, "")`——目录名兜底仍在。
pub(crate) fn read_frontmatter(
    fs: &dyn FileSystem,
    dir: &Path,
) -> (Option<String>, String) {
    let head = fs.read_head(&dir.join(SKILL_FILE), SKILL_HEAD_LIMIT).ok();
    match head {
        Some(bytes) => {
            let (name, description) = parse_frontmatter_head(&bytes);
            (name, description.unwrap_or_default())
        }
        None => (None, String::new()),
    }
}

/// 名称解析：覆盖名 → frontmatter → 兜底；最终必须合法（§9.3）。
pub(crate) fn resolve_name(
    override_name: Option<&str>,
    fm_name: Option<String>,
    fallback: String,
) -> AppResult<String> {
    let name = override_name
        .map(str::to_string)
        .or(fm_name)
        .unwrap_or(fallback);
    if !valid_skill_name(&name) {
        return Err(AppError::invalid_input(format!(
            "skill 名称不合法（需 ^[a-z0-9][a-z0-9-]{{1,63}}$）: {name}"
        )));
    }
    Ok(name)
}

/// 目录名兜底（无 frontmatter name 时）。
pub(crate) fn dir_name_of(path: &Path) -> AppResult<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::invalid_input(format!("无法从路径取目录名: {}", path.display())))
}

/// URL 仓库名兜底：去尾 `/` 与 `.git` 后取最后一段。
pub(crate) fn repo_name_of(url: &str) -> AppResult<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let tail = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    if tail.is_empty() {
        Err(AppError::invalid_input("无法从 URL 推断仓库名，请显式指定 name"))
    } else {
        Ok(tail.to_string())
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
