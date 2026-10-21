//! install 单测（FakeGit / 计数 FS 注入，不联网）。

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

