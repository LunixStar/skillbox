//! deploy / undeploy 单测：真实文件系统（tempfile）+ 固定时钟 + 注入失败点。
//!
//! 覆盖 §5 全部硬约束，重点是：
//! - [`recorded_symlink_never_deletes_real_dir`]（关键测试）：DB 记录只
//!   代表历史意图，绝不授权删除磁盘上的真实目录（§5.2 硬约束）。
//! - 写入原子性：失败路径不留下部分写入的目标、tmp 清理干净（§5.3）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use skillbox_domain::{
    AppError, AppResult, ErrorKind, ReplacePolicy, SkillId, SyncMode, TargetState, ToolId,
};
use skillbox_ports::{
    Clock, EntryMeta, FileSystem, ProdFileSystem, ProdLinker, WatchMute,
};

use super::{
    deploy, undeploy, overwrite_authorized, DeployRequest, DeployTarget, SourceMarker,
    UndeployRequest, UndeployTarget, ToolDeployOutcome, ToolUndeployOutcome,
    SOURCE_MARKER_FILE,
};

/// 固定时钟：trash 槽位 / marker 时间戳确定可断言。
const NOW_MS: i64 = 1_700_000_000_000;

struct FixedClock;

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        NOW_MS
    }
}

struct TestEnv {
    root: tempfile::TempDir,
    source: PathBuf,
    agent_a: PathBuf,
    trash: PathBuf,
    fs: Arc<dyn FileSystem>,
    clock: Arc<dyn Clock>,
    linker: Arc<dyn skillbox_ports::Linker>,
}

fn setup_env() -> TestEnv {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("central").join("skill-a");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("SKILL.md"), b"---\nname: skill-a\n---\nbody").unwrap();
    std::fs::write(source.join("run.sh"), b"echo skill-a").unwrap();
    let agent_a = root.path().join("agents").join("agentA").join("skills");
    std::fs::create_dir_all(&agent_a).unwrap();
    let trash = root.path().join("trash");
    std::fs::create_dir_all(&trash).unwrap();
    TestEnv {
        root,
        source,
        agent_a,
        trash,
        fs: Arc::new(ProdFileSystem),
        clock: Arc::new(FixedClock),
        linker: Arc::new(ProdLinker),
    }
}

impl TestEnv {
    /// 任意 agent 的目标路径（`agents/<agent>/skills/skill-a`）。
    fn target_path(&self, agent: &str) -> PathBuf {
        self.root.path().join("agents").join(agent).join("skills").join("skill-a")
    }

    fn deploy_req<'a>(
        &self,
        policy: ReplacePolicy<'a>,
        targets: Vec<DeployTarget>,
    ) -> DeployRequest<'a> {
        DeployRequest {
            skill_id: SkillId(7),
            source: self.source.clone(),
            targets,
            policy,
            trash_root: self.trash.clone(),
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            linker: self.linker.clone(),
            watcher: None,
        }
    }

    fn undeploy_req(&self, targets: Vec<UndeployTarget>) -> UndeployRequest {
        UndeployRequest {
            skill_id: SkillId(7),
            source: self.source.clone(),
            targets,
            trash_root: self.trash.clone(),
            fs: self.fs.clone(),
            clock: self.clock.clone(),
            linker: self.linker.clone(),
            watcher: None,
        }
    }
}

/// 链接模式目标（Symlink = 链接优先 + 回退链）。
fn dt(env: &TestEnv, agent: &str) -> DeployTarget {
    DeployTarget {
        tool_id: ToolId(agent.into()),
        path: env.target_path(agent),
        mode: SyncMode::Symlink,
    }
}

/// 副本模式目标。
fn dt_copy(env: &TestEnv, agent: &str) -> DeployTarget {
    DeployTarget {
        tool_id: ToolId(agent.into()),
        path: env.target_path(agent),
        mode: SyncMode::Copy,
    }
}

fn ut(env: &TestEnv, agent: &str) -> UndeployTarget {
    UndeployTarget { tool_id: ToolId(agent.into()), path: env.target_path(agent) }
}

/// 构造"别人的链接"：Windows 用 junction（无权限要求），Unix 用 symlink。
fn make_foreign_link(env: &TestEnv, link: &Path) {
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    let elsewhere = env.root.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("not-ours.txt"), b"foreign").unwrap();
    #[cfg(windows)]
    env.linker.junction(&elsewhere, link).unwrap();
    #[cfg(not(windows))]
    env.linker.symlink_dir(&elsewhere, link).unwrap();
}

/// 在 write 上注入失败的文件系统（其余全部委托 ProdFileSystem）。
struct FailWriteFs(ProdFileSystem);

impl FileSystem for FailWriteFs {
    fn exists(&self, path: &Path) -> bool {
        self.0.exists(path)
    }
    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta> {
        self.0.symlink_metadata(path)
    }
    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>> {
        self.0.read_dir(path)
    }
    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
        self.0.read_head(path, max_bytes)
    }
    fn read_to_string(&self, path: &Path) -> AppResult<String> {
        self.0.read_to_string(path)
    }
    fn write(&self, _path: &Path, _contents: &[u8]) -> AppResult<()> {
        Err(AppError::io("注入失败：write 不可用"))
    }
    fn create_dir_all(&self, path: &Path) -> AppResult<()> {
        self.0.create_dir_all(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.0.rename(from, to)
    }
    fn copy_file(&self, from: &Path, to: &Path) -> AppResult<u64> {
        self.0.copy_file(from, to)
    }
    fn copy_dir_all(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.0.copy_dir_all(from, to)
    }
    fn remove_dir(&self, path: &Path) -> AppResult<()> {
        self.0.remove_dir(path)
    }
    fn remove_dir_all(&self, path: &Path) -> AppResult<()> {
        self.0.remove_dir_all(path)
    }
    fn remove_file(&self, path: &Path) -> AppResult<()> {
        self.0.remove_file(path)
    }
    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf> {
        self.0.canonicalize(path)
    }
    fn is_local_volume(&self, path: &Path) -> bool {
        self.0.is_local_volume(path)
    }
}

/// 计数型监听：验证 mute / unmute 成对（§5.3）。
#[derive(Default)]
struct RecordingWatch {
    muted: AtomicI32,
    unmuted: AtomicI32,
}

impl WatchMute for RecordingWatch {
    fn mute(&self) {
        self.muted.fetch_add(1, Ordering::SeqCst);
    }
    fn unmute(&self) {
        self.unmuted.fetch_add(1, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// §5.2 授权决策表
// ---------------------------------------------------------------------------

/// 【关键测试】§5.2 硬约束：DB 记录 mode=symlink，磁盘却是真实目录。
/// `Recorded { mode: "symlink" }` 绝不授权删除 RealDir——DB 记录只代表
/// 历史意图，不代表磁盘现状。目标必须原样保留并返回 TargetConflict。
#[test]
fn recorded_symlink_never_deletes_real_dir() {
    let env = setup_env();
    let target = env.target_path("agentA");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("user-data.txt"), b"precious").unwrap();

    let err = deploy(env.deploy_req(
        ReplacePolicy::Recorded { mode: "symlink" },
        vec![dt(&env, "agentA")],
    ))
    .unwrap_err();

    assert_eq!(err.kind, ErrorKind::TargetConflict, "必须是 TargetConflict: {err}");
    // 真实目录与用户文件必须原样保留。
    let meta = std::fs::symlink_metadata(&target).unwrap();
    assert!(meta.is_dir(), "目标必须仍是真实目录（lstat 语义）");
    assert_eq!(std::fs::read(target.join("user-data.txt")).unwrap(), b"precious");
    // conflicts 必须指着那个目录说话（§2.3）。
    let details = err.details.expect("TargetConflict 必须携带 details");
    assert_eq!(details.conflicts.len(), 1);
    assert_eq!(details.conflicts[0].path, target);
    assert!(!details.conflicts[0].reason.is_empty());
}

/// §5.1/§5.2 决策表逐格钉死：5 状态 × 4 策略变体 = 20 格。
#[test]
fn overwrite_authorized_matches_decision_table() {
    let cases: Vec<(TargetState, ReplacePolicy<'_>, bool)> = vec![
        (TargetState::Absent, ReplacePolicy::NoClobber, true),
        (TargetState::Absent, ReplacePolicy::Recorded { mode: "symlink" }, true),
        (TargetState::Absent, ReplacePolicy::Recorded { mode: "copy" }, true),
        (TargetState::Absent, ReplacePolicy::UserConfirmed, true),
        (TargetState::LinkToSource, ReplacePolicy::NoClobber, true),
        (TargetState::LinkToSource, ReplacePolicy::Recorded { mode: "symlink" }, true),
        (TargetState::LinkToSource, ReplacePolicy::Recorded { mode: "copy" }, true),
        (TargetState::LinkToSource, ReplacePolicy::UserConfirmed, true),
        (TargetState::ForeignLink, ReplacePolicy::NoClobber, false),
        (TargetState::ForeignLink, ReplacePolicy::Recorded { mode: "symlink" }, false),
        (TargetState::ForeignLink, ReplacePolicy::Recorded { mode: "copy" }, false),
        (TargetState::ForeignLink, ReplacePolicy::UserConfirmed, true),
        // 【硬约束】Recorded{mode:"symlink"} 绝不授权删除 RealDir。
        (TargetState::RealDir, ReplacePolicy::NoClobber, false),
        (TargetState::RealDir, ReplacePolicy::Recorded { mode: "symlink" }, false),
        (TargetState::RealDir, ReplacePolicy::Recorded { mode: "copy" }, false),
        (TargetState::RealDir, ReplacePolicy::UserConfirmed, true),
        (TargetState::RealFile, ReplacePolicy::NoClobber, false),
        (TargetState::RealFile, ReplacePolicy::Recorded { mode: "symlink" }, false),
        (TargetState::RealFile, ReplacePolicy::Recorded { mode: "copy" }, false),
        (TargetState::RealFile, ReplacePolicy::UserConfirmed, true),
    ];
    for (state, policy, expected) in cases {
        assert_eq!(
            overwrite_authorized(state, &policy),
            expected,
            "{state:?} × {policy:?} 应为 {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// deploy
// ---------------------------------------------------------------------------

#[test]
fn absent_deploys_in_symlink_and_copy_modes() {
    let env = setup_env();

    // 链接模式（symlink → junction 平台回退后 mode 仍记 Symlink，§5.2）。
    let report =
        deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap();
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.results[0].outcome, ToolDeployOutcome::Deployed { mode: SyncMode::Symlink });
    let link = env.target_path("agentA");
    assert_eq!(
        env.fs.canonicalize(&link).unwrap(),
        env.fs.canonicalize(&env.source).unwrap(),
        "链接必须解析到中央库正本"
    );
    assert!(link.join("SKILL.md").exists());

    // 副本模式。
    let report =
        deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt_copy(&env, "agentB")])).unwrap();
    assert_eq!(
        report.results[0].outcome,
        ToolDeployOutcome::Deployed { mode: SyncMode::Copy }
    );
    let copy = env.target_path("agentB");
    assert!(std::fs::symlink_metadata(&copy).unwrap().is_dir(), "副本必须是真实目录");
    assert!(copy.join("SKILL.md").exists());
    assert!(copy.join("run.sh").exists());
}

#[test]
fn link_to_source_redeploy_is_idempotent() {
    let env = setup_env();
    deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap();

    // 重复部署：目标已是指向本源的链接 → AlreadyUpToDate，不再写盘。
    let report =
        deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap();
    assert_eq!(report.results[0].outcome, ToolDeployOutcome::AlreadyUpToDate);
    // trash 未出现任何东西（无删除动作）。
    assert_eq!(std::fs::read_dir(&env.trash).unwrap().count(), 0);
    assert!(env.target_path("agentA").join("SKILL.md").exists());
}

#[test]
fn foreign_link_rejected_under_noclobber_allowed_under_user_confirmed() {
    let env = setup_env();
    let target = env.target_path("agentA");
    make_foreign_link(&env, &target);
    let elsewhere = env.root.path().join("elsewhere");

    // NoClobber：拒绝，外源链接原样保留。
    let err =
        deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    let still = env.linker.read_link(&target).unwrap();
    assert_eq!(
        env.fs.canonicalize(&still).unwrap(),
        env.fs.canonicalize(&elsewhere).unwrap(),
        "外源链接必须原样保留"
    );

    // UserConfirmed：允许覆盖；旧链接进 trash，新链接指向正本。
    let report =
        deploy(env.deploy_req(ReplacePolicy::UserConfirmed, vec![dt(&env, "agentA")])).unwrap();
    assert_eq!(
        report.results[0].outcome,
        ToolDeployOutcome::Deployed { mode: SyncMode::Symlink }
    );
    assert_eq!(
        env.fs.canonicalize(&target).unwrap(),
        env.fs.canonicalize(&env.source).unwrap()
    );
    // 被替换的外源链接可在 trash 找回（v1 无硬删除）。
    assert!(env.trash.join(NOW_MS.to_string()).join("skill-a").exists(), "旧链接必须在 trash 中");
    assert!(elsewhere.join("not-ours.txt").exists(), "外源目标内容不受影响");
}

#[test]
fn copy_mode_writes_source_marker() {
    let env = setup_env();
    deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt_copy(&env, "agentA")])).unwrap();

    let marker_path = env.target_path("agentA").join(SOURCE_MARKER_FILE);
    let raw = std::fs::read_to_string(&marker_path).unwrap();
    let marker: SourceMarker = serde_json::from_str(&raw).unwrap();
    assert_eq!(marker.source, env.source, "marker 必须记录中央库正本路径");
    assert_eq!(marker.written_at_ms, NOW_MS);
}

/// 注入 write 失败：部署必须在提交前失败，不留部分写入，tmp 清理干净（§5.3）。
#[test]
fn failed_copy_deploy_leaves_no_partial_target() {
    let env = setup_env();
    let target = env.target_path("agentA");

    let req = DeployRequest {
        fs: Arc::new(FailWriteFs(ProdFileSystem)),
        ..env.deploy_req(ReplacePolicy::NoClobber, vec![dt_copy(&env, "agentA")])
    };
    let report = deploy(req).unwrap();

    match &report.results[0].outcome {
        ToolDeployOutcome::Failed { reason } => assert!(!reason.is_empty()),
        other => panic!("期望 Failed，实际 {other:?}"),
    }
    assert!(!target.exists(), "失败部署不得留下部分写入的目标");
    // tmp 必须被清理。
    let leftovers: Vec<_> = std::fs::read_dir(&env.agent_a)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".skillbox-deploy-"))
        .collect();
    assert!(leftovers.is_empty(), "临时路径必须清理: {leftovers:?}");
    // 正本不受影响。
    assert!(env.source.join("SKILL.md").exists());
}

#[test]
fn deploy_missing_source_is_not_found() {
    let env = setup_env();
    let mut req =
        env.deploy_req(ReplacePolicy::UserConfirmed, vec![dt(&env, "agentA")]);
    req.source = env.root.path().join("central").join("no-such-skill");
    let err = deploy(req).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(!env.target_path("agentA").exists(), "源缺失时不得写目标");
}

/// §5.3：写盘前 mute、写盘后 unmute；成功 / 失败 / 幂等 / 冲突路径全部成对。
#[test]
fn deploy_mutes_and_unmutes_watcher_balanced() {
    let env = setup_env();
    let watch = Arc::new(RecordingWatch::default());

    // 两个真实写盘目标（链接 + 副本）各 mute/unmute 一次。
    let mut req = env.deploy_req(
        ReplacePolicy::NoClobber,
        vec![dt(&env, "agentA"), dt_copy(&env, "agentB")],
    );
    req.watcher = Some(watch.clone());
    deploy(req).unwrap();
    assert_eq!(watch.muted.load(Ordering::SeqCst), 2);
    assert_eq!(watch.unmuted.load(Ordering::SeqCst), 2);

    // 幂等目标不写盘 → 不 mute。
    let mut req = env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")]);
    req.watcher = Some(watch.clone());
    deploy(req).unwrap(); // AlreadyUpToDate
    assert_eq!(watch.muted.load(Ordering::SeqCst), 2, "幂等跳过不得触发 mute");

    // 失败路径同样成对（FailWriteFs 注入 write 失败）。
    let req = DeployRequest {
        fs: Arc::new(FailWriteFs(ProdFileSystem)),
        watcher: Some(watch.clone()),
        ..env.deploy_req(ReplacePolicy::NoClobber, vec![dt_copy(&env, "agentC")])
    };
    let report = deploy(req).unwrap();
    assert!(matches!(report.results[0].outcome, ToolDeployOutcome::Failed { .. }));
    assert_eq!(watch.muted.load(Ordering::SeqCst), 3);
    assert_eq!(watch.unmuted.load(Ordering::SeqCst), 3);

    // 冲突路径：阶段一整体拒绝，一次 mute 都不能有。
    std::fs::create_dir_all(env.target_path("agentD")).unwrap();
    let mut req = env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentD")]);
    req.watcher = Some(watch.clone());
    assert!(deploy(req).is_err());
    assert_eq!(watch.muted.load(Ordering::SeqCst), 3, "冲突拒绝不得触发 mute");
}

// ---------------------------------------------------------------------------
// undeploy
// ---------------------------------------------------------------------------

#[test]
fn undeploy_removes_our_link_and_keeps_source() {
    let env = setup_env();
    deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap();

    let report = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap();
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.results[0].outcome, ToolUndeployOutcome::Removed);
    assert!(!env.target_path("agentA").exists(), "链接必须已移除");
    // 正本仍在中央库（取消部署 ≠ 删除 Skill，§10）。
    assert!(env.source.join("SKILL.md").exists());
    assert!(env.source.join("run.sh").exists());
    // 移除的链接在 trash 可找回。
    assert!(env.trash.join(NOW_MS.to_string()).join("skill-a").exists());
}

#[test]
fn undeploy_rejects_real_dir_and_foreign_link_and_reports_absent() {
    let env = setup_env();

    // 真实目录：拒绝且原样保留。
    let real = env.target_path("agentA");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("mine.txt"), b"keep").unwrap();
    let err = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    assert!(real.join("mine.txt").exists(), "真实目录必须原样保留");

    // 外源链接：拒绝且原样保留。
    make_foreign_link(&env, &env.target_path("agentB"));
    let err = undeploy(env.undeploy_req(vec![ut(&env, "agentB")])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    assert!(
        env.target_path("agentB").join("not-ours.txt").exists(),
        "外源链接必须原样保留"
    );

    // 本就不存在：AlreadyAbsent（幂等成功，非错误）。
    std::fs::remove_dir_all(&real).unwrap();
    let report = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap();
    assert_eq!(report.results[0].outcome, ToolUndeployOutcome::AlreadyAbsent);
}
