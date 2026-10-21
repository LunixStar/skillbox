//! update 单测（FakeGit 注入，不联网）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillbox_domain::OriginKind;
use skillbox_ports::{
    FileSystem, GitClient, MemoryIndexStore, OriginInfo, ProdFileSystem, ProdHasher, ProdLinker,
};

use super::{update_apply, update_check, UpdateOutcome, UpdateStatus, UpdateTarget};
use crate::doctor::hash_dir;
use crate::test_env::FixedClock;
use skillbox_domain::{AppResult, SkillId};

struct UpdateTestEnv {
    root: tempfile::TempDir,
    ctx: super::UpdateContext,
    store: Arc<MemoryIndexStore>,
}

impl UpdateTestEnv {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryIndexStore::new());
        let ctx = super::UpdateContext {
            central_root: root.path().join("central"),
            cache_root: root.path().join("cache"),
            trash_root: root.path().join("trash"),
            fs: Arc::new(ProdFileSystem),
            clock: Arc::new(FixedClock),
            git: Arc::new(FakeGit::unavailable()),
            hasher: Arc::new(ProdHasher),
            linker: Arc::new(ProdLinker),
            index: store.clone(),
            watcher: None,
        };
        UpdateTestEnv { root, ctx, store }
    }

    /// 在中央库造正本并登记索引，返回待更新快照。
    fn make_skill(&self, name: &str, origin: OriginInfo, description: &str) -> UpdateTarget {
        let canonical = self.ctx.central_root.join(name);
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::write(
            canonical.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\nold body"),
        )
        .unwrap();
        let content_hash = hash_dir(
            self.ctx.fs.as_ref(),
            self.ctx.linker.as_ref(),
            self.ctx.hasher.as_ref(),
            &canonical,
        );
        let skill_id = self
            .ctx
            .index
            .insert_skill(name, description, &origin, &canonical, content_hash.as_deref())
            .unwrap();
        UpdateTarget {
            skill_id,
            name: name.to_string(),
            description: description.to_string(),
            origin,
            canonical_path: canonical,
            content_hash,
        }
    }
}

/// 假 git：把预置目录当"远端仓库"，clone = 递归复制。
struct FakeGit {
    repo: Option<PathBuf>,
    revision: String,
    diff: String,
}

impl FakeGit {
    fn unavailable() -> Self {
        FakeGit { repo: None, revision: String::new(), diff: String::new() }
    }
    fn from_dir(repo: PathBuf, revision: &str) -> Self {
        FakeGit { repo: Some(repo), revision: revision.to_string(), diff: "1 file changed".into() }
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
        Ok(self.diff.clone())
    }
    fn checkout(&self, _repo: &Path, _revision: &str) -> AppResult<()> {
        Ok(())
    }
}

fn skill_row(store: &MemoryIndexStore, id: SkillId) -> skillbox_ports::StoredSkill {
    store.skills().into_iter().find(|s| s.id == id).expect("skill 行必须存在")
}

// ---------------------------------------------------------------------------
// git 来源
// ---------------------------------------------------------------------------

/// 造一个"远端仓库"目录。
fn make_repo(env: &UpdateTestEnv, body: &str) -> PathBuf {
    let repo = env.root.path().join("remote");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("SKILL.md"), body).unwrap();
    repo
}

#[test]
fn git_check_up_to_date() {
    let mut env = UpdateTestEnv::new();
    let repo = make_repo(&env, "---\nname: demo-skill\ndescription: d\n---\nbody");
    let origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r1"));

    let outcome = update_check(&target, &env.ctx).unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpToDate);
    assert_eq!(outcome.current_revision.as_deref(), Some("r1"));
    assert_eq!(outcome.new_revision.as_deref(), Some("r1"));
}

#[test]
fn git_check_update_available_with_diff() {
    let mut env = UpdateTestEnv::new();
    let repo = make_repo(&env, "---\nname: demo-skill\ndescription: d\n---\nbody");
    let origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r2"));

    let outcome = update_check(&target, &env.ctx).unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpdateAvailable);
    assert_eq!(outcome.current_revision.as_deref(), Some("r1"));
    assert_eq!(outcome.new_revision.as_deref(), Some("r2"));
    assert_eq!(outcome.diff_summary.as_deref(), Some("1 file changed"));
}

#[test]
fn git_check_without_recorded_revision_reports_available() {
    let mut env = UpdateTestEnv::new();
    let repo = make_repo(&env, "body");
    let origin = OriginInfo::git("https://example.com/repo.git", None, None);
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r9"));

    let outcome = update_check(&target, &env.ctx).unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpdateAvailable);
    assert!(outcome.note.as_deref().unwrap().contains("无法证明"));
}

#[test]
fn git_apply_dry_run_zero_writes_then_apply_swaps_canonical() {
    let mut env = UpdateTestEnv::new();
    // 预置"远端仓库"（新版内容）。
    let repo = env.root.path().join("remote");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("SKILL.md"), "---\nname: demo-skill\ndescription: d2\n---\nnew body")
        .unwrap();
    let origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r2"));

    // dry_run：预览通过，正本与索引原样。
    let preview = update_apply(&target, true, &env.ctx).unwrap();
    assert!(matches!(preview, UpdateOutcome::Preview(ref r) if r.revision.as_deref() == Some("r2")));
    assert!(target.canonical_path.join("SKILL.md").is_file());
    assert!(std::fs::read_to_string(target.canonical_path.join("SKILL.md")).unwrap().contains("old body"));
    assert_eq!(skill_row(&env.store, target.skill_id).origin.revision.as_deref(), Some("r1"));

    // 实跑：正本换新、旧版入 trash、索引刷新。
    let applied = update_apply(&target, false, &env.ctx).unwrap();
    assert!(matches!(applied, UpdateOutcome::Applied(ref r) if r.revision.as_deref() == Some("r2")));
    let body = std::fs::read_to_string(target.canonical_path.join("SKILL.md")).unwrap();
    assert!(body.contains("new body"), "正本必须是远端新内容");
    assert!(
        env.ctx.trash_root
            .join(crate::test_env::NOW_MS.to_string())
            .join("demo-skill")
            .join("SKILL.md")
            .is_file(),
        "旧正本必须整体进 trash"
    );
    let row = skill_row(&env.store, target.skill_id);
    assert_eq!(row.origin.revision.as_deref(), Some("r2"));
    assert_eq!(row.description, "d2", "frontmatter description 必须刷新");
    assert!(row.content_hash.is_some());
    assert_ne!(row.content_hash, target.content_hash, "内容哈希必须更新");
}

#[test]
fn git_apply_up_to_date_no_writes() {
    let mut env = UpdateTestEnv::new();
    let repo = make_repo(&env, "body");
    let origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r1"));

    let outcome = update_apply(&target, false, &env.ctx).unwrap();
    assert_eq!(outcome, UpdateOutcome::UpToDate);
    assert!(std::fs::read_to_string(target.canonical_path.join("SKILL.md")).unwrap().contains("old body"));
}

#[test]
fn git_apply_missing_url_rejected() {
    let env = UpdateTestEnv::new();
    let mut origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    origin.ref_path = None;
    let target = env.make_skill("demo-skill", origin, "d");
    let err = update_apply(&target, false, &env.ctx).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
}

#[test]
fn git_apply_subpath_missing_rejected() {
    let mut env = UpdateTestEnv::new();
    let repo = env.root.path().join("remote");
    std::fs::create_dir_all(&repo).unwrap();
    let origin =
        OriginInfo::git("https://example.com/repo.git", Some("no-such-dir".into()), Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r2"));

    let err = update_apply(&target, false, &env.ctx).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    assert!(std::fs::read_to_string(target.canonical_path.join("SKILL.md")).unwrap().contains("old body"), "正本不得被破坏");
}

// ---------------------------------------------------------------------------
// 本地来源：重新导入
// ---------------------------------------------------------------------------

#[test]
fn local_reimport_refreshes_metadata_when_drifted() {
    let env = UpdateTestEnv::new();
    let target = env.make_skill("demo-skill", OriginInfo::local(), "d");

    // 索引与正本一致 → UpToDate。
    let outcome = update_check(&target, &env.ctx).unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpToDate);
    assert_eq!(update_apply(&target, false, &env.ctx).unwrap(), UpdateOutcome::UpToDate);

    // 正本被外部改动（description 变化）→ 可重新导入。
    std::fs::write(
        target.canonical_path.join("SKILL.md"),
        "---\nname: demo-skill\ndescription: d2\n---\nchanged body",
    )
    .unwrap();
    let outcome = update_check(&target, &env.ctx).unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpdateAvailable);

    let preview = update_apply(&target, true, &env.ctx).unwrap();
    assert!(matches!(preview, UpdateOutcome::Preview(_)));

    let applied = update_apply(&target, false, &env.ctx).unwrap();
    assert!(matches!(applied, UpdateOutcome::Applied(_)));
    let row = skill_row(&env.store, target.skill_id);
    assert_eq!(row.description, "d2");
    let fresh_hash = hash_dir(env.ctx.fs.as_ref(), env.ctx.linker.as_ref(), env.ctx.hasher.as_ref(), &target.canonical_path);
    assert_eq!(row.content_hash, fresh_hash, "内容哈希必须刷新为现场值");
    assert_ne!(row.content_hash, target.content_hash, "与入库时旧哈希必须不同");
}

#[test]
fn local_missing_canonical_rejected() {
    let env = UpdateTestEnv::new();
    let target = env.make_skill("demo-skill", OriginInfo::local(), "d");
    std::fs::remove_dir_all(&target.canonical_path).unwrap();
    let err = update_check(&target, &env.ctx).unwrap_err();
    assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
}

// ---------------------------------------------------------------------------
// 压缩包 / git 不可用
// ---------------------------------------------------------------------------

#[test]
fn archive_unsupported_in_check_and_apply() {
    let env = UpdateTestEnv::new();
    let mut origin = OriginInfo::local();
    origin.kind = OriginKind::Archive;
    origin.ref_path = Some("D:/x.zip".into());
    let target = env.make_skill("demo-skill", origin, "d");

    let check = update_check(&target, &env.ctx).unwrap();
    assert_eq!(check.status, UpdateStatus::Unsupported);
    assert!(check.note.unwrap().contains("压缩包"));
    assert!(matches!(
        update_apply(&target, false, &env.ctx).unwrap(),
        UpdateOutcome::Unsupported { .. }
    ));
}

#[test]
fn git_apply_old_copy_lands_in_trash_exactly_once() {
    let mut env = UpdateTestEnv::new();
    let repo = make_repo(&env, "---\nname: demo-skill\ndescription: d\n---\nnew");
    let origin = OriginInfo::git("https://example.com/repo.git", None, Some("r1".into()));
    let target = env.make_skill("demo-skill", origin, "d");
    env.ctx.git = Arc::new(FakeGit::from_dir(repo, "r2"));

    let outcome = update_apply(&target, false, &env.ctx).unwrap();
    assert!(matches!(outcome, UpdateOutcome::Applied(_)));
    let trash_entries: Vec<_> = std::fs::read_dir(&env.ctx.trash_root).unwrap().collect();
    assert_eq!(trash_entries.len(), 1, "旧正本恰好一份进 trash");
}
