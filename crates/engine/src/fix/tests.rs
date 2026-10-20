//! fix 单测：逐项修复的确认门、执行器与 TOCTOU 防护。
//!
//! 关键约束逐条钉死：
//! - `duplicate_name_never_auto_fixed_even_when_confirmed`：DuplicateName
//!   任何策略都跳过（§6.3 禁止自动选正本）。
//! - 危险项（adopt / ForeignLink / 悬空移除 / copy 更新）NoClobber 下
//!   一律 NeedsConfirmation 且磁盘零改动。
//! - `fix_uses_live_disk_state_not_doctor_snapshot`：fix 重新推算差异，
//!   不信任 doctor 快照（K4）。

use std::path::Path;

use skillbox_domain::{ReplacePolicy, SyncMode};
use skillbox_ports::LinkKind;

use super::{fix, FixRequest};
use crate::doctor::{doctor, DoctorOpts};
use crate::test_env::HealthTestEnv;
use crate::types::{
    DiscrepancyKind, DoctorReport, FixOutcome, FixReport, FixResult, IssueId,
};

fn run_doctor(env: &HealthTestEnv, tools: &[&str]) -> DoctorReport {
    doctor(DoctorOpts { ctx: env.ctx(tools), tools: None, min_severity: None }).unwrap()
}

/// 经 doctor 找到（kind @ path）的问题 id——确保与磁盘现状一致。
fn issue_id(env: &HealthTestEnv, tools: &[&str], kind: DiscrepancyKind, path: &Path) -> IssueId {
    let report = run_doctor(env, tools);
    report
        .issues
        .iter()
        .find(|i| i.discrepancy.kind == kind && i.discrepancy.path == path)
        .map(|i| i.id.clone())
        .unwrap_or_else(|| panic!("未找到 {kind:?} @ {}: {:?}", path.display(), report.issues))
}

fn fix_req<'a>(
    env: &HealthTestEnv,
    policy: ReplacePolicy<'a>,
    items: Vec<IssueId>,
    tools: &[&str],
) -> FixRequest<'a> {
    FixRequest {
        items,
        policy,
        ctx: env.ctx(tools),
        trash_root: env.trash.clone(),
        index_store: Some(env.store.clone()),
        watcher: None,
    }
}

fn single(report: &FixReport) -> &FixResult {
    assert_eq!(report.results.len(), 1, "应恰好一项: {:?}", report.results);
    &report.results[0]
}

// ---------------------------------------------------------------------------
// DuplicateName：只提示
// ---------------------------------------------------------------------------

#[test]
fn duplicate_name_never_auto_fixed_even_when_confirmed() {
    let env = HealthTestEnv::new();
    for (tool, body) in [("claude", "A"), ("codex", "B")] {
        let stray = env.target_path(tool, "stray");
        std::fs::create_dir_all(&stray).unwrap();
        std::fs::write(
            stray.join("SKILL.md"),
            format!("---\nname: stray-{body}\n---\n{body}").as_bytes(),
        )
        .unwrap();
    }
    let path = env.target_path("claude", "stray");
    let id = issue_id(&env, &["claude", "codex"], DiscrepancyKind::DuplicateName, &path);
    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["claude", "codex"]))
        .unwrap();
    match &single(&report).outcome {
        FixOutcome::Skipped { reason } => assert!(reason.contains("人工"), "{reason}"),
        other => panic!("DuplicateName 任何策略都应跳过: {other:?}"),
    }
    assert!(env.target_path("claude", "stray").join("SKILL.md").is_file());
    assert!(env.target_path("codex", "stray").join("SKILL.md").is_file());
}

// ---------------------------------------------------------------------------
// adopt（UnmanagedRealDir）
// ---------------------------------------------------------------------------

#[test]
fn unmanaged_real_dir_adopt_needs_confirmation_then_applies() {
    let env = HealthTestEnv::new();
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray-skill\ndescription: my skill\n---\nbody").unwrap();
    std::fs::write(stray.join("run.sh"), b"echo stray").unwrap();
    let id = issue_id(&env, &["claude"], DiscrepancyKind::UnmanagedRealDir, &stray);

    // NoClobber：dry-run 影响清单，磁盘零改动。
    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id.clone()], &["claude"])).unwrap();
    match &single(&report).outcome {
        FixOutcome::NeedsConfirmation { reason } => {
            assert!(reason.contains("收编") && reason.contains("UserConfirmed"), "影响面预览: {reason}");
        }
        other => panic!("应 NeedsConfirmation: {other:?}"),
    }
    assert!(stray.join("SKILL.md").is_file(), "确认前磁盘不动");

    // UserConfirmed：收编完成。
    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);

    // 中央库正本（frontmatter name 优先），原位解析到正本。
    let canonical = env.central_root.join("stray-skill");
    assert!(canonical.join("SKILL.md").is_file());
    assert!(stray.join("SKILL.md").is_file(), "原位内容可达（链接或副本）");
    assert_eq!(
        env.fs.canonicalize(&stray).unwrap(),
        env.fs.canonicalize(&canonical).unwrap(),
        "原位应解析到中央库正本"
    );
    // 索引登记：1 skill + 1 target。
    let skills = env.store.skills();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "stray-skill");
    assert_eq!(skills[0].description, "my skill");
    let targets = env.store.targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].mode, SyncMode::Symlink);
    assert_eq!(targets[0].path, stray);
    // 收编后 doctor 不再报该目录。
    assert!(
        run_doctor(&env, &["claude"]).issues.iter().all(|i| i.discrepancy.path != stray),
        "{:?}",
        run_doctor(&env, &["claude"]).issues
    );
}

#[test]
fn adopt_without_index_store_is_skipped() {
    let env = HealthTestEnv::new();
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray\n---\nbody").unwrap();
    let id = issue_id(&env, &["claude"], DiscrepancyKind::UnmanagedRealDir, &stray);
    let req = FixRequest {
        items: vec![id],
        policy: ReplacePolicy::UserConfirmed,
        ctx: env.ctx(&["claude"]),
        trash_root: env.trash.clone(),
        index_store: None,
        watcher: None,
    };
    let report = fix(req).unwrap();
    match &single(&report).outcome {
        FixOutcome::Skipped { reason } => assert!(reason.contains("index_store"), "{reason}"),
        other => panic!("无索引存储应 Skipped: {other:?}"),
    }
    assert!(stray.join("SKILL.md").is_file(), "未收编");
}

// ---------------------------------------------------------------------------
// BrokenLink
// ---------------------------------------------------------------------------

#[test]
fn broken_link_rebuilds_under_any_policy() {
    let env = HealthTestEnv::new();
    env.make_central_skill("alpha", &[]);
    env.register("alpha", &[("claude", SyncMode::Symlink, None)]);
    // 链接指向已删除的旧正本路径 → BrokenLink（规则 2 重建匹配）。
    env.make_broken_link(&env.central_root.join("alpha-old"), &env.target_path("claude", "alpha"));
    let path = env.target_path("claude", "alpha");
    let id = issue_id(&env, &["claude"], DiscrepancyKind::BrokenLink, &path);

    // 链接无数据：NoClobber 也放行重建。
    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);

    assert!(path.join("SKILL.md").is_file(), "重建后内容可达");
    assert_eq!(
        env.fs.canonicalize(&path).unwrap(),
        env.fs.canonicalize(&env.central_root.join("alpha")).unwrap()
    );
    assert!(run_doctor(&env, &["claude"]).issues.is_empty(), "重建后健康");
}

#[test]
fn unmatched_broken_link_remove_is_gated() {
    let env = HealthTestEnv::new();
    let path = env.target_path("claude", "dangling");
    env.make_broken_link(&env.root.path().join("gone"), &path);
    let id = issue_id(&env, &["claude"], DiscrepancyKind::BrokenLink, &path);

    // NoClobber：影响面预览，链接未动。
    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id.clone()], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::NeedsConfirmation { .. }), "{:?}", report.results);
    assert!(
        matches!(env.linker.link_kind(&path).unwrap(), LinkKind::Symlink | LinkKind::Junction),
        "确认前链接未动"
    );

    // UserConfirmed：入 trash。
    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);
    assert_eq!(env.linker.link_kind(&path).unwrap(), LinkKind::NotALink, "链接已移除");
}

// ---------------------------------------------------------------------------
// ForeignLink
// ---------------------------------------------------------------------------

#[test]
fn foreign_link_remove_is_gated() {
    let env = HealthTestEnv::new();
    let elsewhere = env.root.path().join("elsewhere").join("foo");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("keep.txt"), b"keep").unwrap();
    let path = env.target_path("claude", "foreign");
    env.make_link(&elsewhere, &path);
    let id = issue_id(&env, &["claude"], DiscrepancyKind::ForeignLink, &path);

    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id.clone()], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::NeedsConfirmation { .. }), "{:?}", report.results);
    assert!(path.exists(), "确认前链接未动");

    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);
    assert!(!path.exists(), "链接已入 trash");
    assert!(elsewhere.join("keep.txt").is_file(), "链接目标本身不动");
}

// ---------------------------------------------------------------------------
// MissingTarget
// ---------------------------------------------------------------------------

#[test]
fn missing_target_redeploys_without_confirmation() {
    let env = HealthTestEnv::new();
    env.make_central_skill("beta", &[]);
    env.register("beta", &[("claude", SyncMode::Symlink, None)]);
    let path = env.target_path("claude", "beta");
    let id = issue_id(&env, &["claude"], DiscrepancyKind::MissingTarget, &path);

    // Absent 目标无数据可失：NoClobber 直接放行。
    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);
    assert_eq!(
        env.fs.canonicalize(&path).unwrap(),
        env.fs.canonicalize(&env.central_root.join("beta")).unwrap()
    );
}

#[test]
fn missing_target_copy_requires_index_store() {
    let env = HealthTestEnv::new();
    env.make_central_skill("beta", &[]);
    env.register("beta", &[("codex", SyncMode::Copy, None)]);
    let path = env.target_path("codex", "beta");
    let id = issue_id(&env, &["codex"], DiscrepancyKind::MissingTarget, &path);
    let req = FixRequest {
        items: vec![id],
        policy: ReplacePolicy::UserConfirmed,
        ctx: env.ctx(&["codex"]),
        trash_root: env.trash.clone(),
        index_store: None,
        watcher: None,
    };
    let report = fix(req).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Skipped { .. }), "{:?}", report.results);
    assert!(!path.exists(), "无索引存储不动盘");
}

// ---------------------------------------------------------------------------
// UnrecordedDeployment
// ---------------------------------------------------------------------------

#[test]
fn unrecorded_deployment_registers_record() {
    let env = HealthTestEnv::new();
    env.make_central_skill("gamma", &[]);
    env.register("gamma", &[]); // skill 有、target 记录无
    let path = env.target_path("claude", "gamma");
    env.make_link(&env.central_root.join("gamma"), &path);
    let id = issue_id(&env, &["claude"], DiscrepancyKind::UnrecordedDeployment, &path);

    // 补登记不动用户数据：NoClobber 也放行。
    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id], &["claude"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);
    let targets = env.store.targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, path);
    assert_eq!(targets[0].mode, SyncMode::Symlink);
    assert!(run_doctor(&env, &["claude"]).issues.is_empty(), "补登记后健康");
}

// ---------------------------------------------------------------------------
// CopyDrift
// ---------------------------------------------------------------------------

#[test]
fn copy_drift_updates_after_confirmation() {
    let env = HealthTestEnv::new();
    env.make_central_skill("delta", &[("run.sh", b"echo delta")]);
    env.register("delta", &[("codex", SyncMode::Copy, None)]); // source_hash = 正本哈希
    let path = env.target_path("codex", "delta");
    // 副本漂移：用户改了副本内容。
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("SKILL.md"), b"---\nname: delta\n---\nuser edited").unwrap();
    let id = issue_id(&env, &["codex"], DiscrepancyKind::CopyDrift, &path);

    let report = fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id.clone()], &["codex"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::NeedsConfirmation { .. }), "{:?}", report.results);
    assert!(path.join("SKILL.md").is_file(), "确认前副本不动");

    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["codex"])).unwrap();
    assert!(matches!(single(&report).outcome, FixOutcome::Applied), "{:?}", report.results);
    // 副本已从正本更新（tracked copy 带来源标记）。
    assert!(path.join("run.sh").is_file());
    assert!(path.join(".skillbox-source.json").is_file(), "tracked copy 带来源标记");
    assert!(std::fs::read_to_string(path.join("SKILL.md")).unwrap().contains("body of delta"));
    // source_hash 已刷新 → doctor 不再报 CopyDrift。
    assert!(run_doctor(&env, &["codex"]).issues.is_empty(), "{:?}", run_doctor(&env, &["codex"]).issues);
}

// ---------------------------------------------------------------------------
// 匹配与去重
// ---------------------------------------------------------------------------

#[test]
fn unknown_issue_id_is_skipped() {
    let env = HealthTestEnv::new();
    let report =
        fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![IssueId("bogus:foo:bar".into())], &["claude"]))
            .unwrap();
    match &single(&report).outcome {
        FixOutcome::Skipped { reason } => assert!(reason.contains("已不存在"), "{reason}"),
        other => panic!("应 Skipped: {other:?}"),
    }
}

#[test]
fn duplicate_item_id_is_ignored() {
    let env = HealthTestEnv::new();
    env.make_central_skill("beta", &[]);
    env.register("beta", &[("claude", SyncMode::Symlink, None)]);
    let path = env.target_path("claude", "beta");
    let id = issue_id(&env, &["claude"], DiscrepancyKind::MissingTarget, &path);
    let report =
        fix(fix_req(&env, ReplacePolicy::NoClobber, vec![id.clone(), id], &["claude"])).unwrap();
    assert_eq!(report.results.len(), 2);
    assert!(matches!(report.results[0].outcome, FixOutcome::Applied), "{:?}", report.results);
    match &report.results[1].outcome {
        FixOutcome::Skipped { reason } => assert!(reason.contains("重复"), "{reason}"),
        other => panic!("重复 id 应 Skipped: {other:?}"),
    }
}

#[test]
fn fix_uses_live_disk_state_not_doctor_snapshot() {
    let env = HealthTestEnv::new();
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray\n---\n").unwrap();
    let id = issue_id(&env, &["claude"], DiscrepancyKind::UnmanagedRealDir, &stray);
    // doctor 之后磁盘已变：目录被用户删除。
    std::fs::remove_dir_all(&stray).unwrap();
    let report = fix(fix_req(&env, ReplacePolicy::UserConfirmed, vec![id], &["claude"])).unwrap();
    match &single(&report).outcome {
        FixOutcome::Skipped { reason } => assert!(reason.contains("已不存在"), "{reason}"),
        other => panic!("问题消失应 Skipped: {other:?}"),
    }
    // 中央库没有被塞入收编。
    assert!(!env.central_root.join("stray").exists());
}
