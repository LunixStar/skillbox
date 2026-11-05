//! doctor 单测：只读体检的零写入证明（CountingFs）+ 七类差异端到端分类
//! + 过滤选项 + 内容哈希语义。

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;

use skillbox_domain::{SyncMode, ToolId};

use super::{doctor, hash_dir, DoctorOpts};
use crate::test_env::{CountingFs, HealthTestEnv};
use crate::types::{DiscrepancyKind, DoctorIssue, DoctorReport, Severity, SuggestedFix};

fn run_doctor(env: &HealthTestEnv, tools: &[&str]) -> DoctorReport {
    doctor(DoctorOpts { ctx: env.ctx(tools), tools: None, min_severity: None }).unwrap()
}

fn kind_set(report: &DoctorReport) -> HashSet<DiscrepancyKind> {
    report.issues.iter().map(|i| i.discrepancy.kind).collect()
}

fn find(report: &DoctorReport, kind: DiscrepancyKind) -> &DoctorIssue {
    report
        .issues
        .iter()
        .find(|i| i.discrepancy.kind == kind)
        .unwrap_or_else(|| panic!("未找到 {kind:?}: {:?}", report.issues))
}

// ---------------------------------------------------------------------------
// 只读证明
// ---------------------------------------------------------------------------

#[test]
fn doctor_is_read_only_zero_disk_writes() {
    let env = HealthTestEnv::new();
    env.make_central_skill("alpha", &[]);
    env.register("alpha", &[("claude", SyncMode::Symlink, None)]);
    env.make_link(&env.central_root.join("alpha"), &env.target_path("claude", "alpha"));
    // 真实目录（要算内容哈希）+ 外部链接：让 doctor 做足读盘工作。
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray\n---\n").unwrap();
    let elsewhere = env.root.path().join("elsewhere").join("foo");
    std::fs::create_dir_all(&elsewhere).unwrap();
    env.make_link(&elsewhere, &env.target_path("claude", "foreign"));

    let (cfs, writes) = CountingFs::wrap(env.fs.clone());
    let report =
        doctor(DoctorOpts { ctx: env.ctx_with_fs(&["claude"], cfs), tools: None, min_severity: None })
            .unwrap();
    assert_eq!(writes.load(Ordering::Relaxed), 0, "doctor 必须零写盘");
    assert!(!report.issues.is_empty(), "场景应产出问题（否则零写入断言无意义）");
}

// ---------------------------------------------------------------------------
// 七类差异端到端
// ---------------------------------------------------------------------------

#[test]
fn doctor_end_to_end_classifies_all_states() {
    let env = HealthTestEnv::new();
    let tools = ["claude", "codex"];

    // 中央库正本与登记。
    env.make_central_skill("alpha", &[("run.sh", b"echo alpha")]);
    env.make_central_skill("beta", &[]);
    env.make_central_skill("gamma", &[]);
    env.make_central_skill("delta", &[]); // 不登记：UnrecordedDeployment 无 skill_id
    env.make_central_skill("zeta", &[]);
    env.register("alpha", &[
        ("claude", SyncMode::Symlink, None),
        ("codex", SyncMode::Copy, Some("stale-hash".into())),
    ]);
    env.register("beta", &[("claude", SyncMode::Symlink, None)]);
    let gamma_id = env.register("gamma", &[]); // 有 skill 无 target 记录
    env.register("zeta", &[("codex", SyncMode::Symlink, None)]);

    // 健康态：claude/alpha 链接 + 记录齐备。
    env.make_link(&env.central_root.join("alpha"), &env.target_path("claude", "alpha"));
    // BrokenLink：链接指过的旧正本路径已删（规则 2 重建匹配）。
    env.make_broken_link(&env.central_root.join("beta-old"), &env.target_path("claude", "beta"));
    // ForeignLink：指向中央库之外。
    let elsewhere = env.root.path().join("elsewhere").join("foo");
    std::fs::create_dir_all(&elsewhere).unwrap();
    env.make_link(&elsewhere, &env.target_path("claude", "foreign"));
    // UnrecordedDeployment：gamma（skill 已登记、无 target 记录）与
    // delta（完全未登记）。
    env.make_link(&env.central_root.join("gamma"), &env.target_path("claude", "gamma"));
    env.make_link(&env.central_root.join("delta"), &env.target_path("claude", "delta"));
    // UnmanagedRealDir + DuplicateName：两个工具下同名不同内容的真实目录。
    for (tool, body) in [("claude", "A"), ("codex", "B")] {
        let stray = env.target_path(tool, "stray");
        std::fs::create_dir_all(&stray).unwrap();
        std::fs::write(
            stray.join("SKILL.md"),
            format!("---\nname: stray-{body}\n---\n{body}").as_bytes(),
        )
        .unwrap();
    }
    // CopyDrift：codex/alpha 真实目录，内容哈希与记录 source_hash 不一致。
    let drift = env.target_path("codex", "alpha");
    std::fs::create_dir_all(&drift).unwrap();
    std::fs::write(drift.join("SKILL.md"), b"---\nname: alpha\n---\ndrifted").unwrap();
    // MissingTarget：codex/zeta 只有记录，无磁盘部署。

    let report = run_doctor(&env, &tools);
    let kinds = kind_set(&report);
    for kind in [
        DiscrepancyKind::UnmanagedRealDir,
        DiscrepancyKind::BrokenLink,
        DiscrepancyKind::ForeignLink,
        DiscrepancyKind::MissingTarget,
        DiscrepancyKind::UnrecordedDeployment,
        DiscrepancyKind::CopyDrift,
        DiscrepancyKind::DuplicateName,
    ] {
        assert!(kinds.contains(&kind), "缺少差异类型 {kind:?}: {:?}", report.issues);
    }

    // 抽查关键字段。
    let broken = find(&report, DiscrepancyKind::BrokenLink);
    assert_eq!(broken.discrepancy.suggested_fix, SuggestedFix::RebuildLink);
    assert_eq!(broken.discrepancy.severity, Severity::Warning);

    let gamma = report
        .issues
        .iter()
        .find(|i| i.discrepancy.kind == DiscrepancyKind::UnrecordedDeployment
            && i.discrepancy.path.ends_with("gamma"))
        .unwrap();
    assert_eq!(gamma.discrepancy.skill_id, Some(gamma_id));
    let delta = report
        .issues
        .iter()
        .find(|i| i.discrepancy.kind == DiscrepancyKind::UnrecordedDeployment
            && i.discrepancy.path.ends_with("delta"))
        .unwrap();
    assert_eq!(delta.discrepancy.skill_id, None);

    let missing = find(&report, DiscrepancyKind::MissingTarget);
    assert_eq!(missing.discrepancy.path, env.target_path("codex", "zeta"));

    assert_eq!(find(&report, DiscrepancyKind::CopyDrift).discrepancy.severity, Severity::Warning);
    assert_eq!(find(&report, DiscrepancyKind::DuplicateName).discrepancy.severity, Severity::Info);

    // 健康链接不应出现在问题清单。
    let healthy = env.target_path("claude", "alpha");
    assert!(
        report.issues.iter().all(|i| i.discrepancy.path != healthy),
        "健康链接不应报问题: {:?}",
        report.issues
    );
    // checked_tools 排序；固定时钟 duration 为 0。
    assert_eq!(report.checked_tools, vec![ToolId("claude".into()), ToolId("codex".into())]);
    assert_eq!(report.duration_ms, 0);
}

// ---------------------------------------------------------------------------
// 过滤选项
// ---------------------------------------------------------------------------

#[test]
fn min_severity_filters_info_level_issues() {
    let env = HealthTestEnv::new();
    // MissingTarget（Warning）：有记录无部署，应存活 Warning 过滤。
    env.make_central_skill("gone", &[]);
    env.register("gone", &[("claude", SyncMode::Symlink, None)]);
    // UnmanagedRealDir（Info）：未纳管真实目录。
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray\n---\n").unwrap();
    // ForeignLink（Info）：指向中央库之外。
    let elsewhere = env.root.path().join("elsewhere").join("foo");
    std::fs::create_dir_all(&elsewhere).unwrap();
    env.make_link(&elsewhere, &env.target_path("claude", "foreign"));

    let report = doctor(DoctorOpts {
        ctx: env.ctx(&["claude"]),
        tools: None,
        min_severity: Some(Severity::Warning),
    })
    .unwrap();
    assert!(report.issues.iter().all(|i| i.discrepancy.severity >= Severity::Warning));
    assert!(report.issues.iter().any(|i| i.discrepancy.kind == DiscrepancyKind::MissingTarget));
    assert!(!report.issues.iter().any(|i| i.discrepancy.kind == DiscrepancyKind::UnmanagedRealDir));
    assert!(!report.issues.iter().any(|i| i.discrepancy.kind == DiscrepancyKind::ForeignLink));

    // 不过滤时 Info 级也在。
    let report = run_doctor(&env, &["claude"]);
    assert!(report.issues.iter().any(|i| i.discrepancy.kind == DiscrepancyKind::UnmanagedRealDir));
    assert!(report.issues.iter().any(|i| i.discrepancy.kind == DiscrepancyKind::ForeignLink));
}

/// 【修复体检误报】点前缀目录（agent 内部目录如 `.system`）与
/// 无 SKILL.md 的目录不是技能，不参与对账；有 SKILL.md 的未纳管目录仍报告（Info）。
#[test]
fn inspect_skips_dot_dirs_and_dirs_without_skill_md() {
    let env = HealthTestEnv::new();
    // 点目录：agent 内部目录（无 SKILL.md，含标记文件）。
    let dot = env.target_path("claude", ".system");
    std::fs::create_dir_all(&dot).unwrap();
    std::fs::write(dot.join("marker"), b"x").unwrap();
    // 无 SKILL.md 的普通目录。
    let plain = env.target_path("claude", "not-a-skill");
    std::fs::create_dir_all(&plain).unwrap();
    // 未纳管技能（有 SKILL.md）——仍应报告。
    let stray = env.target_path("claude", "stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("SKILL.md"), b"---\nname: stray\n---\n").unwrap();

    let report = run_doctor(&env, &["claude"]);
    let paths: HashSet<&Path> = report.issues.iter().map(|i| i.discrepancy.path.as_path()).collect();
    assert!(!paths.iter().any(|p| p.ends_with(".system")), "点目录不应体检: {:?}", paths);
    assert!(!paths.iter().any(|p| p.ends_with("not-a-skill")), "无 SKILL.md 目录不应体检: {:?}", paths);
    assert!(paths.iter().any(|p| p.ends_with("stray")), "未纳管技能应报告: {:?}", paths);
}

#[test]
fn tools_filter_limits_scope_and_avoids_false_missing_target() {
    let env = HealthTestEnv::new();
    env.make_central_skill("alpha", &[]);
    env.make_central_skill("beta", &[]);
    env.register("alpha", &[("claude", SyncMode::Symlink, None)]);
    env.register("beta", &[("codex", SyncMode::Symlink, None)]); // codex 无磁盘部署
    env.make_link(&env.central_root.join("alpha"), &env.target_path("claude", "alpha"));

    let report = doctor(DoctorOpts {
        ctx: env.ctx(&["claude", "codex"]),
        tools: Some(vec![ToolId("claude".into())]),
        min_severity: None,
    })
    .unwrap();
    assert_eq!(report.checked_tools, vec![ToolId("claude".into())]);
    // codex 未检查：不报 codex 的问题，也不误报其记录 MissingTarget。
    let codex_dir = env.skills_dir("codex");
    assert!(
        report.issues.iter().all(|i| !i.discrepancy.path.starts_with(&codex_dir)),
        "codex 未检查不应报问题: {:?}",
        report.issues
    );
    assert!(report.issues.is_empty(), "claude 侧健康: {:?}", report.issues);
}

#[test]
fn nonexistent_tool_roots_are_skipped() {
    let env = HealthTestEnv::new();
    env.make_central_skill("alpha", &[]);
    env.register("alpha", &[("claude", SyncMode::Symlink, None)]);
    env.make_link(&env.central_root.join("alpha"), &env.target_path("claude", "alpha"));
    // ghost 的 skills 根从未创建：doctor 不应报错、不应产生条目。
    let report = doctor(DoctorOpts {
        ctx: env.ctx(&["claude", "ghost"]),
        tools: None,
        min_severity: None,
    })
    .unwrap();
    assert!(report.issues.is_empty(), "claude 健康且 ghost 无记录: {:?}", report.issues);
    assert_eq!(
        report.checked_tools,
        vec![ToolId("claude".into()), ToolId("ghost".into())]
    );
}

// ---------------------------------------------------------------------------
// 内容哈希语义
// ---------------------------------------------------------------------------

#[test]
fn hash_dir_is_content_based_and_ignores_root_marker() {
    let env = HealthTestEnv::new();
    let a = env.root.path().join("a");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join("SKILL.md"), b"---\nname: a\n---\n").unwrap();
    std::fs::write(a.join("run.sh"), b"echo a").unwrap();

    // mtime 与 a 必然不同（写入时间已流逝），内容一致 → 哈希必须一致。
    std::thread::sleep(std::time::Duration::from_millis(15));
    let b = env.root.path().join("b");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("SKILL.md"), b"---\nname: a\n---\n").unwrap();
    std::fs::write(b.join("run.sh"), b"echo a").unwrap();
    // 根级来源标记不参与内容身份。
    std::fs::write(b.join(".skillbox-source.json"), br#"{"source":"x","writtenAtMs":1}"#).unwrap();

    let h = |dir: &Path| hash_dir(env.fs.as_ref(), env.linker.as_ref(), env.hasher.as_ref(), dir);
    let ha = h(&a).unwrap();
    assert_eq!(ha, h(&b).unwrap(), "内容一致（mtime 不同、根级标记跳过）→ 哈希一致");

    std::fs::write(b.join("run.sh"), b"echo changed").unwrap();
    assert_ne!(ha, h(&b).unwrap(), "内容变化 → 哈希变化");
}
