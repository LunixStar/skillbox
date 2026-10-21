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
mod tests {
    use super::*;
    use crate::test_env::{CountingFs, FixedClock};
    use skillbox_ports::{MemoryIndexStore, ProdFileSystem, ProdHasher, ProdLinker};

    struct InstallTestEnv {
        root: tempfile::TempDir,
        ctx: InstallContext,
        store: Arc<MemoryIndexStore>,
    }

    impl InstallTestEnv {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let store = Arc::new(MemoryIndexStore::new());
            let ctx = InstallContext {
                central_root: root.path().join("central"),
                cache_root: root.path().join("cache"),
                fs: Arc::new(ProdFileSystem),
                clock: Arc::new(FixedClock),
                git: Arc::new(FakeGit::unavailable()),
                hasher: Arc::new(ProdHasher),
                linker: Arc::new(ProdLinker),
                index: store.clone(),
                watcher: None,
            };
            InstallTestEnv { root, ctx, store }
        }

        /// 准备一个本地来源 skill 目录。
        fn make_source(&self, dir: &str, frontmatter: Option<&str>) -> PathBuf {
            let dir = self.root.path().join(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let content = frontmatter.unwrap_or("---\nname: demo-skill\ndescription: a demo\n---\nbody");
            std::fs::write(dir.join("SKILL.md"), content).unwrap();
            dir
        }

        fn local_req(path: &Path) -> InstallRequest {
            InstallRequest {
                source: InstallSource::Local { path: path.to_path_buf() },
                name: None,
                keep_in_place: false,
            }
        }
    }

    /// 假 git：把预置目录当"远端仓库"，clone = 递归复制。
    struct FakeGit {
        repo: Option<PathBuf>,
        revision: String,
    }

    impl FakeGit {
        fn unavailable() -> Self {
            FakeGit { repo: None, revision: String::new() }
        }
        fn from_dir(repo: PathBuf) -> Self {
            FakeGit { repo: Some(repo), revision: "r1".to_string() }
        }
    }

    impl GitClient for FakeGit {
        fn is_available(&self) -> bool {
            true
        }
        fn clone(&self, _url: &str, dest: &Path, _depth: Option<u32>) -> AppResult<()> {
            let repo = self.repo.as_ref().expect("FakeGit 未配置仓库");
            ProdFileSystem.copy_dir_all(repo, dest)
        }
        fn fetch(&self, _repo: &Path) -> AppResult<()> {
            Ok(())
        }
        fn head_revision(&self, _repo: &Path) -> AppResult<String> {
            Ok(self.revision.clone())
        }
        fn diff_summary(&self, _repo: &Path, _from: &str, _to: &str) -> AppResult<String> {
            Ok("diff".to_string())
        }
        fn checkout(&self, _repo: &Path, _revision: &str) -> AppResult<()> {
            Ok(())
        }
    }

    #[test]
    fn local_copy_installs_into_central() {
        let env = InstallTestEnv::new();
        let source = env.make_source("incoming", None);
        let outcome = install(InstallTestEnv::local_req(&source), false, &env.ctx).unwrap();

        assert_eq!(outcome.name, "demo-skill");
        assert!(!outcome.dry_run);
        let canonical = env.ctx.central_root.join("demo-skill");
        assert_eq!(outcome.canonical_path, canonical);
        assert!(canonical.join("SKILL.md").is_file(), "正本必须复制进中央库");
        let id = outcome.skill_id.expect("实跑必须分配 id");
        let skill = &env.store.skills()[0];
        assert_eq!(skill.id, id);
        assert_eq!(skill.name, "demo-skill");
        assert_eq!(skill.description, "a demo");
        assert_eq!(skill.canonical_path, canonical);
        assert!(skill.content_hash.is_some(), "入库即计算内容哈希");
        assert_eq!(skill.origin.kind, skillbox_domain::OriginKind::Local);
    }

    #[test]
    fn local_dry_run_zero_writes_and_zero_index() {
        let mut env = InstallTestEnv::new();
        let source = env.make_source("incoming", None);
        let (counting, writes) = CountingFs::wrap(env.ctx.fs.clone());
        env.ctx.fs = counting;

        let outcome = install(InstallTestEnv::local_req(&source), true, &env.ctx).unwrap();
        assert!(outcome.dry_run);
        assert_eq!(outcome.skill_id, None);
        assert_eq!(outcome.name, "demo-skill");
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0, "dry_run 必须零写盘");
        assert!(env.store.skills().is_empty(), "dry_run 不写索引");
        assert!(!env.ctx.central_root.join("demo-skill").exists(), "dry_run 不建正本");
    }

    #[test]
    fn keep_in_place_registers_without_copy() {
        let env = InstallTestEnv::new();
        let source = env.make_source("incoming", None);
        let mut req = InstallTestEnv::local_req(&source);
        req.keep_in_place = true;
        let outcome = install(req, false, &env.ctx).unwrap();

        assert_eq!(outcome.canonical_path, source, "keep-in-place 正本留在原位");
        assert!(source.join("SKILL.md").is_file());
        assert!(!env.ctx.central_root.join("demo-skill").exists(), "keep-in-place 不复制进中央库");
        assert_eq!(env.store.skills()[0].canonical_path, source);
    }

    #[test]
    fn name_falls_back_to_dir_name_without_frontmatter() {
        let env = InstallTestEnv::new();
        let source = env.make_source("fallback-name", Some("no frontmatter here"));
        let outcome = install(InstallTestEnv::local_req(&source), false, &env.ctx).unwrap();
        assert_eq!(outcome.name, "fallback-name");
    }

    #[test]
    fn invalid_name_rejected_before_any_write() {
        let env = InstallTestEnv::new();
        let source = env.make_source("incoming", Some("---\nname: Bad Name!\ndescription: x\n---\n"));
        // frontmatter 名称非法且无覆盖名 → 拒绝。
        let err = install(InstallTestEnv::local_req(&source), false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
        assert!(env.store.skills().is_empty());
        // 覆盖名非法 → 同样拒绝。
        let mut req = InstallTestEnv::local_req(&source);
        req.name = Some("Also Bad".into());
        let err = install(req, false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    }

    #[test]
    fn central_conflict_rejected_with_details() {
        let env = InstallTestEnv::new();
        let source = env.make_source("incoming", None);
        let pre = env.ctx.central_root.join("demo-skill");
        std::fs::create_dir_all(&pre).unwrap();
        std::fs::write(pre.join("SKILL.md"), "old").unwrap();

        let err = install(InstallTestEnv::local_req(&source), false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::TargetConflict);
        let details = err.details.expect("TargetConflict 必须携带 details");
        assert_eq!(details.conflicts.len(), 1);
        assert_eq!(details.conflicts[0].path, pre);
        // 原中央库内容未被覆盖。
        assert_eq!(std::fs::read_to_string(pre.join("SKILL.md")).unwrap(), "old");
    }

    #[test]
    fn missing_or_non_dir_source_rejected() {
        let env = InstallTestEnv::new();
        let missing = env.root.path().join("nope");
        let err = install(InstallTestEnv::local_req(&missing), false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);

        let file = env.root.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        let err = install(InstallTestEnv::local_req(&file), false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    }

    #[test]
    fn archive_rejected_explicitly() {
        let env = InstallTestEnv::new();
        let req = InstallRequest {
            source: InstallSource::Archive { path: PathBuf::from("/x.zip"), subpath: None },
            name: None,
            keep_in_place: false,
        };
        let err = install(req, false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
        assert!(err.message.contains("暂不支持"));
    }

    #[test]
    fn git_install_clones_and_records_revision() {
        let mut env = InstallTestEnv::new();
        // 预置"远端仓库"：根下 skills/ 子目录带 SKILL.md。
        let repo = env.root.path().join("remote");
        std::fs::create_dir_all(repo.join("skills")).unwrap();
        std::fs::write(
            repo.join("skills").join("SKILL.md"),
            "---\nname: git-skill\ndescription: from git\n---\nbody",
        )
        .unwrap();
        env.ctx.git = Arc::new(FakeGit::from_dir(repo.clone()));

        let req = InstallRequest {
            source: InstallSource::Git {
                url: "https://example.com/repo.git".into(),
                subpath: Some("skills".into()),
                revision: None,
            },
            name: None,
            keep_in_place: false,
        };
        let outcome = install(req, false, &env.ctx).unwrap();
        assert_eq!(outcome.name, "git-skill");
        let canonical = env.ctx.central_root.join("git-skill");
        assert!(canonical.join("SKILL.md").is_file(), "子路径内容必须复制进中央库");

        let skill = &env.store.skills()[0];
        assert_eq!(skill.origin.kind, skillbox_domain::OriginKind::Git);
        assert_eq!(skill.origin.ref_path.as_deref(), Some("https://example.com/repo.git"));
        assert_eq!(skill.origin.subpath.as_deref(), Some("skills"));
        assert_eq!(skill.origin.revision.as_deref(), Some("r1"));
        // 克隆体保留在 cache（updates 复用）。
        assert!(env.ctx.cache_root.is_dir());
    }

    #[test]
    fn git_dry_run_zero_writes() {
        let mut env = InstallTestEnv::new();
        env.ctx.git = Arc::new(FakeGit::unavailable());
        let (counting, writes) = CountingFs::wrap(env.ctx.fs.clone());
        env.ctx.fs = counting;

        let req = InstallRequest {
            source: InstallSource::Git {
                url: "https://example.com/my-skill.git".into(),
                subpath: None,
                revision: None,
            },
            name: None,
            keep_in_place: false,
        };
        let outcome = install(req, true, &env.ctx).unwrap();
        assert_eq!(outcome.name, "my-skill", "dry_run 名称取 URL 仓库名（预览值）");
        assert_eq!(outcome.skill_id, None);
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0, "git dry_run 不克隆不写盘");
    }

    #[test]
    fn git_subpath_missing_rejected() {
        let mut env = InstallTestEnv::new();
        let repo = env.root.path().join("remote");
        std::fs::create_dir_all(&repo).unwrap();
        env.ctx.git = Arc::new(FakeGit::from_dir(repo));

        let req = InstallRequest {
            source: InstallSource::Git {
                url: "https://example.com/repo.git".into(),
                subpath: Some("no-such-dir".into()),
                revision: None,
            },
            name: None,
            keep_in_place: false,
        };
        let err = install(req, false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
        assert!(env.store.skills().is_empty());
    }

    #[test]
    fn empty_url_rejected() {
        let env = InstallTestEnv::new();
        let req = InstallRequest {
            source: InstallSource::Git { url: "  ".into(), subpath: None, revision: None },
            name: None,
            keep_in_place: false,
        };
        let err = install(req, false, &env.ctx).unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    }
}
