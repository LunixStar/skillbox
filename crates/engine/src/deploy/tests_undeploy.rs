//! undeploy 单测：移除范围（本源链接 + 带标记副本）、执行期重分类
//! （TOCTOU 防护）、无标记真实目录拒绝（§5 / §10）。
//!
//! 测试设施（TestEnv / 固定时钟 / 目标构造器）共享自 `deploy/tests.rs`。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use skillbox_domain::{AppResult, ErrorKind, ReplacePolicy};
use skillbox_ports::{EntryMeta, FileSystem, ProdFileSystem};

use super::classify::{SourceMarker, SOURCE_MARKER_FILE};
use super::tests::{dt, dt_copy, make_foreign_link, setup_env, ut, NOW_MS};
use super::{deploy, undeploy, ToolUndeployOutcome, UndeployRequest};

/// 平台通用的链接删除：Windows `fs::remove_dir` 对目录 symlink 与
/// junction（reparse point）都有效；Unix 目录 symlink 用 `remove_file`。
/// 不走 `Linker::remove_link`，避免穿透 [`SwapAfterPrecheckFs`] 的钩子。
fn remove_link_platform(link: &Path) {
    #[cfg(windows)]
    std::fs::remove_dir(link).expect("删除链接失败");
    #[cfg(not(windows))]
    std::fs::remove_file(link).expect("删除链接失败");
}

/// 在 precheck 与执行之间"偷换目标"的文件系统：对指定路径的第 2 次
/// `symlink_metadata`（precheck 分类第 1 次、执行期重分类第 2 次）先把
/// 链接换成带用户数据的真实目录再放行，模拟 TOCTOU 竞态。
struct SwapAfterPrecheckFs {
    inner: ProdFileSystem,
    link: PathBuf,
    classify_calls: AtomicU32,
}

impl SwapAfterPrecheckFs {
    fn maybe_swap(&self, path: &Path) {
        if path != self.link {
            return;
        }
        if self.classify_calls.fetch_add(1, Ordering::SeqCst) != 1 {
            return; // 只在第 2 次分类（执行期）触发。
        }
        remove_link_platform(&self.link);
        std::fs::create_dir_all(&self.link).expect("构造偷换目录失败");
        std::fs::write(self.link.join("user-swapped.txt"), b"precious").expect("写入失败");
    }
}

impl FileSystem for SwapAfterPrecheckFs {
    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta> {
        self.maybe_swap(path);
        self.inner.symlink_metadata(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>> {
        self.inner.read_dir(path)
    }
    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
        self.inner.read_head(path, max_bytes)
    }
    fn read_to_string(&self, path: &Path) -> AppResult<String> {
        self.inner.read_to_string(path)
    }
    fn write(&self, path: &Path, contents: &[u8]) -> AppResult<()> {
        self.inner.write(path, contents)
    }
    fn create_dir_all(&self, path: &Path) -> AppResult<()> {
        self.inner.create_dir_all(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.inner.rename(from, to)
    }
    fn copy_file(&self, from: &Path, to: &Path) -> AppResult<u64> {
        self.inner.copy_file(from, to)
    }
    fn copy_dir_all(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.inner.copy_dir_all(from, to)
    }
    fn remove_dir(&self, path: &Path) -> AppResult<()> {
        self.inner.remove_dir(path)
    }
    fn remove_dir_all(&self, path: &Path) -> AppResult<()> {
        self.inner.remove_dir_all(path)
    }
    fn remove_file(&self, path: &Path) -> AppResult<()> {
        self.inner.remove_file(path)
    }
    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf> {
        self.inner.canonicalize(path)
    }
    fn is_local_volume(&self, path: &Path) -> bool {
        self.inner.is_local_volume(path)
    }
}

// ---------------------------------------------------------------------------
// 移除范围与幂等
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

    // 本就不存在：Skipped { already_absent }（幂等终态，非失败）。
    std::fs::remove_dir_all(&real).unwrap();
    let report = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap();
    assert_eq!(
        report.results[0].outcome,
        ToolUndeployOutcome::Skipped { reason: "already_absent".to_string() }
    );
}

// ---------------------------------------------------------------------------
// TOCTOU / 副本移除 / 伪造标记
// ---------------------------------------------------------------------------

/// 【TOCTOU 关键场景】precheck 看到的是指向本源的链接，执行前被换成
/// 带用户数据的真实目录：必须 `Skipped { state_changed }`，绝不按快照
/// 删除，偷换的目录原样保留，trash 一无所获。
#[test]
fn undeploy_skips_when_target_swapped_after_precheck() {
    let env = setup_env();
    deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt(&env, "agentA")])).unwrap();

    let req = UndeployRequest {
        fs: Arc::new(SwapAfterPrecheckFs {
            inner: ProdFileSystem,
            link: env.target_path("agentA"),
            classify_calls: AtomicU32::new(0),
        }),
        ..env.undeploy_req(vec![ut(&env, "agentA")])
    };
    let report = undeploy(req).unwrap();

    assert_eq!(
        report.results[0].outcome,
        ToolUndeployOutcome::Skipped { reason: "state_changed".to_string() }
    );
    // 偷换进来的真实目录必须原样保留。
    assert!(env.target_path("agentA").join("user-swapped.txt").exists());
    assert_eq!(
        std::fs::read_to_string(env.target_path("agentA").join("user-swapped.txt")).unwrap(),
        "precious"
    );
    // trash 里什么都没有（没有任何删除动作发生）。
    assert_eq!(std::fs::read_dir(&env.trash).unwrap().count(), 0);
}

/// 带来源标记的副本（含用户改动 CopyDrift）可以 undeploy：整个副本进
/// trash，用户改动可找回（v1 无硬删除，§5.3）。
#[test]
fn undeploy_moves_edited_copy_to_trash() {
    let env = setup_env();
    deploy(env.deploy_req(ReplacePolicy::NoClobber, vec![dt_copy(&env, "agentA")])).unwrap();
    // 用户在副本里写了自己的东西（CopyDrift）。
    std::fs::write(env.target_path("agentA").join("user-edit.txt"), b"my tweak").unwrap();

    let report = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap();
    assert_eq!(report.results[0].outcome, ToolUndeployOutcome::Removed);
    assert!(!env.target_path("agentA").exists(), "副本必须已整体移除");

    // 用户改动随副本进 trash，可找回。
    let trashed = env.trash.join(NOW_MS.to_string()).join("skill-a");
    assert!(trashed.join("user-edit.txt").exists(), "用户改动必须在 trash 中");
    assert!(trashed.join("SKILL.md").exists());
    assert!(trashed.join(SOURCE_MARKER_FILE).exists());
}

/// 无标记 / 伪造标记（source 指向别的 skill）的真实目录：一律拒绝且
/// 原样保留——磁盘标记是删除的唯一授权证据（K4，§5.2）。
#[test]
fn undeploy_rejects_real_dir_without_our_marker() {
    let env = setup_env();

    // 无标记的真实目录：拒绝。
    let real = env.target_path("agentA");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("mine.txt"), b"keep").unwrap();
    let err = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    assert!(real.join("mine.txt").exists(), "无标记目录必须原样保留");

    // 伪造标记（source 指向别的 skill）：同样拒绝。
    let forged = SourceMarker {
        source: env.root.path().join("central").join("other-skill"),
        written_at_ms: NOW_MS,
    };
    std::fs::write(
        real.join(SOURCE_MARKER_FILE),
        serde_json::to_vec_pretty(&forged).unwrap(),
    )
    .unwrap();
    let err = undeploy(env.undeploy_req(vec![ut(&env, "agentA")])).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    assert!(real.join("mine.txt").exists(), "伪造标记的目录必须原样保留");
    assert!(real.join(SOURCE_MARKER_FILE).exists());
}
