//! reconcile 单测（纯计算，无 IO）：§6.2 七类差异每类至少一条 + 健康态
//! 无差异 + 确定性排序。

use std::path::PathBuf;

use skillbox_domain::{SkillId, SyncMode, TargetState, ToolId};

use crate::types::{
    DiscrepancyKind, DiskEntry, DiskView, IndexView, Severity, SkillRecord, SuggestedFix,
    TargetRecord,
};

use super::reconcile;

// ---------------------------------------------------------------------------
// 构造助手
// ---------------------------------------------------------------------------

fn entry(state: TargetState, tool: &str, path: &str) -> DiskEntry {
    DiskEntry {
        tool_id: ToolId(tool.into()),
        path: PathBuf::from(path),
        state,
        link_target: None,
        link_broken: false,
        fingerprint: None,
        content_hash: None,
        source_skill_id: None,
    }
}

/// 链接条目（带目标与悬空标记）。
fn link_entry(tool: &str, path: &str, target: &str, broken: bool) -> DiskEntry {
    DiskEntry {
        link_target: Some(PathBuf::from(target)),
        link_broken: broken,
        ..entry(TargetState::ForeignLink, tool, path)
    }
}

fn record(skill_id: i64, tool: &str, path: &str, mode: SyncMode, source_hash: Option<&str>) -> TargetRecord {
    TargetRecord {
        skill_id: SkillId(skill_id),
        tool_id: ToolId(tool.into()),
        path: PathBuf::from(path),
        mode,
        source_hash: source_hash.map(str::to_string),
    }
}

fn skill(id: i64, name: &str, canonical: &str) -> SkillRecord {
    SkillRecord {
        id: SkillId(id),
        name: name.to_string(),
        canonical_path: PathBuf::from(canonical),
        content_hash: None,
    }
}

fn view(entries: Vec<DiskEntry>) -> DiskView {
    DiskView { entries }
}

fn index(records: Vec<TargetRecord>, skills: Vec<SkillRecord>) -> IndexView {
    IndexView { skills, records }
}

fn kinds(discrepancies: &[crate::types::Discrepancy]) -> Vec<DiscrepancyKind> {
    discrepancies.iter().map(|d| d.kind).collect()
}

// ---------------------------------------------------------------------------
// 七类差异
// ---------------------------------------------------------------------------

#[test]
fn unmanaged_real_dir_without_record_or_with_symlink_record() {
    // 无记录。
    let out = reconcile(
        &view(vec![entry(TargetState::RealDir, "claude", "/a/.claude/skills/foo")]),
        &index(vec![], vec![]),
    );
    assert_eq!(kinds(&out), vec![DiscrepancyKind::UnmanagedRealDir]);
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].suggested_fix, SuggestedFix::Adopt);

    // 记录为链接模式，磁盘却是真实目录 → 同样 UnmanagedRealDir
    //（DB 记录只代表历史意图，不改变"非我们部署的目录"判定）。
    let out = reconcile(
        &view(vec![entry(TargetState::RealDir, "claude", "/a/.claude/skills/foo")]),
        &index(
            vec![record(1, "claude", "/a/.claude/skills/foo", SyncMode::Symlink, None)],
            vec![],
        ),
    );
    assert_eq!(kinds(&out), vec![DiscrepancyKind::UnmanagedRealDir]);
}

#[test]
fn copy_drift_only_when_both_hashes_present_and_differ() {
    let dir = |hash: Option<&str>| {
        let mut e = entry(TargetState::RealDir, "claude", "/a/.claude/skills/foo");
        e.content_hash = hash.map(str::to_string);
        e
    };
    let copy_record =
        |source_hash: Option<&str>| record(1, "claude", "/a/.claude/skills/foo", SyncMode::Copy, source_hash);

    // 哈希不一致 → CopyDrift。
    let out = reconcile(
        &view(vec![dir(Some("hash-new"))]),
        &index(vec![copy_record(Some("hash-old"))], vec![]),
    );
    assert_eq!(kinds(&out), vec![DiscrepancyKind::CopyDrift]);
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].suggested_fix, SuggestedFix::UpdateCopy);
    assert_eq!(out[0].skill_id, Some(SkillId(1)));

    // 哈希一致 → 健康态。
    let out = reconcile(
        &view(vec![dir(Some("hash-same"))]),
        &index(vec![copy_record(Some("hash-same"))], vec![]),
    );
    assert!(out.is_empty(), "哈希一致不应报漂移: {out:?}");

    // 任一侧哈希缺失 → 不比对（保守，不误报）。
    let out = reconcile(&view(vec![dir(None)]), &index(vec![copy_record(Some("x"))], vec![]));
    assert!(out.is_empty(), "缺磁盘哈希不应报漂移");
    let out = reconcile(&view(vec![dir(Some("x"))]), &index(vec![copy_record(None)], vec![]));
    assert!(out.is_empty(), "缺记录哈希不应报漂移");
}

#[test]
fn broken_link_suggests_rebuild_when_target_matches_index_skill() {
    let skill = skill(3, "foo", "/home/u/.skillbox/skills/foo");
    let e = link_entry("claude", "/a/.claude/skills/foo", "/home/u/.skillbox/skills/foo", true);

    let out = reconcile(&view(vec![e]), &index(vec![], vec![skill]));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::BrokenLink]);
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].suggested_fix, SuggestedFix::RebuildLink);
    assert_eq!(out[0].skill_id, Some(SkillId(3)));
    assert_eq!(out[0].related, vec![PathBuf::from("/home/u/.skillbox/skills/foo")]);
}

#[test]
fn broken_link_rebuilds_via_parent_basename_fallback() {
    // 链接指向已不存在的旧正本路径，但中央库存在同名正本（同目录、
    // 条目名与 skill 名一致）→ 规则 2 命中，建议重建。
    let rec = skill(3, "foo", "/central/foo");
    let e = link_entry("claude", "/a/.claude/skills/foo", "/central/foo-old", true);

    let out = reconcile(&view(vec![e]), &index(vec![], vec![rec.clone()]));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::BrokenLink]);
    assert_eq!(out[0].suggested_fix, SuggestedFix::RebuildLink);
    assert_eq!(out[0].skill_id, Some(SkillId(3)));

    // 条目名与 skill 名不一致 → 不匹配（不猜测意图）。
    let skill2 = skill(3, "bar", "/central/bar");
    let e = link_entry("claude", "/a/.claude/skills/foo", "/central/bar-old", true);
    let out = reconcile(&view(vec![e]), &index(vec![], vec![skill2]));
    assert_eq!(out[0].suggested_fix, SuggestedFix::RemoveLink);
}

#[test]
fn broken_link_suggests_remove_when_target_unmatched() {
    let e = link_entry("claude", "/a/.claude/skills/foo", "/gone/elsewhere", true);
    let out = reconcile(&view(vec![e]), &index(vec![], vec![]));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::BrokenLink]);
    assert_eq!(out[0].suggested_fix, SuggestedFix::RemoveLink);
    assert_eq!(out[0].skill_id, None);
}

#[test]
fn foreign_link_is_info_level_hint() {
    let e = link_entry("claude", "/a/.claude/skills/foo", "/elsewhere/foo", false);
    let out = reconcile(&view(vec![e]), &index(vec![], vec![]));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::ForeignLink]);
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].suggested_fix, SuggestedFix::RemoveLink);
}

#[test]
fn missing_target_when_record_path_not_on_disk() {
    let out = reconcile(&view(vec![]), &index(
        vec![record(9, "codex", "/a/.codex/skills/foo", SyncMode::Symlink, None)],
        vec![],
    ));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::MissingTarget]);
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].suggested_fix, SuggestedFix::Redeploy);
    assert_eq!(out[0].skill_id, Some(SkillId(9)));
}

#[test]
fn missing_target_when_disk_holds_real_file_where_record_deployed() {
    // 记录已部署，磁盘现状是真实文件 → 部署本体缺失。
    let e = entry(TargetState::RealFile, "claude", "/a/.claude/skills/foo");
    let out = reconcile(
        &view(vec![e]),
        &index(vec![record(2, "claude", "/a/.claude/skills/foo", SyncMode::Symlink, None)], vec![]),
    );
    assert_eq!(kinds(&out), vec![DiscrepancyKind::MissingTarget]);
}

#[test]
fn absent_entry_with_record_is_missing_target() {
    let e = entry(TargetState::Absent, "claude", "/a/.claude/skills/foo");
    let out = reconcile(
        &view(vec![e]),
        &index(vec![record(4, "claude", "/a/.claude/skills/foo", SyncMode::Copy, None)], vec![]),
    );
    assert_eq!(kinds(&out), vec![DiscrepancyKind::MissingTarget]);
}

#[test]
fn unrecorded_deployment_for_unindexed_link_to_source() {
    let mut e = link_entry("claude", "/a/.claude/skills/foo", "/home/u/.skillbox/skills/foo", false);
    e.state = TargetState::LinkToSource;
    e.source_skill_id = Some(SkillId(5));

    let out = reconcile(&view(vec![e.clone()]), &index(vec![], vec![skill(5, "foo", "/x")]));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::UnrecordedDeployment]);
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].suggested_fix, SuggestedFix::RegisterRecord);
    assert_eq!(out[0].skill_id, Some(SkillId(5)));

    // 有记录 → 健康态，无差异。
    let out = reconcile(
        &view(vec![e]),
        &index(vec![record(5, "claude", "/a/.claude/skills/foo", SyncMode::Symlink, None)], vec![]),
    );
    assert!(out.is_empty(), "链接 + 记录齐备应为健康态: {out:?}");
}

#[test]
fn duplicate_name_flagged_with_sorted_related() {
    let mut a = entry(TargetState::RealDir, "claude", "/a/.claude/skills/foo");
    a.content_hash = Some("hash-a".into());
    let mut b = entry(TargetState::RealDir, "codex", "/a/.codex/skills/foo");
    b.content_hash = Some("hash-b".into());

    let out = reconcile(&view(vec![b, a]), &index(vec![], vec![]));
    // 同路径按 kind 排序：duplicateName < unmanagedRealDir。
    assert_eq!(
        kinds(&out),
        vec![
            DiscrepancyKind::DuplicateName,
            DiscrepancyKind::UnmanagedRealDir,
            DiscrepancyKind::UnmanagedRealDir,
        ]
    );
    let dup = &out[0];
    assert_eq!(dup.severity, Severity::Info);
    assert_eq!(dup.suggested_fix, SuggestedFix::ChooseCanonical);
    // path 取字典序最小，related 为其余（排序），不自动选正本。
    assert_eq!(dup.path, PathBuf::from("/a/.claude/skills/foo"));
    assert_eq!(dup.related, vec![PathBuf::from("/a/.codex/skills/foo")]);
    assert_eq!(dup.issue_id().0, "duplicateName:-:/a/.claude/skills/foo");
}

#[test]
fn duplicate_name_not_flagged_when_same_identity() {
    // 两个工具的链接指向同一目标 → 同一内容身份，非重复
    //（ForeignLink 提示照报，与重名判定无关）。
    let a = link_entry("claude", "/a/.claude/skills/foo", "/central/foo", false);
    let b = link_entry("codex", "/a/.codex/skills/foo", "/central/foo", false);
    let out = reconcile(&view(vec![a, b]), &index(vec![], vec![]));
    assert!(
        out.iter().all(|d| d.kind != DiscrepancyKind::DuplicateName),
        "同内容身份不应报重名: {out:?}"
    );

    // 两个真实目录哈希一致 → 同一身份（UnmanagedRealDir 照报，但非重名）。
    let mut a = entry(TargetState::RealDir, "claude", "/a/.claude/skills/foo");
    a.content_hash = Some("same".into());
    let mut b = entry(TargetState::RealDir, "codex", "/a/.codex/skills/foo");
    b.content_hash = Some("same".into());
    let out = reconcile(&view(vec![a, b]), &index(vec![], vec![]));
    assert!(out.iter().all(|d| d.kind != DiscrepancyKind::DuplicateName));
    assert_eq!(kinds(&out), vec![DiscrepancyKind::UnmanagedRealDir, DiscrepancyKind::UnmanagedRealDir]);

    // 同一 skill 以链接 + copy 双模式部署 → 同一身份，健康态不报重名。
    let mut link = link_entry("claude", "/a/.claude/skills/foo", "/central/foo", false);
    link.state = TargetState::LinkToSource;
    link.source_skill_id = Some(SkillId(1));
    let mut copy = entry(TargetState::RealDir, "codex", "/a/.codex/skills/foo");
    copy.content_hash = Some("hash-1".into());
    let out = reconcile(
        &view(vec![link, copy]),
        &index(
            vec![
                record(1, "claude", "/a/.claude/skills/foo", SyncMode::Symlink, None),
                record(1, "codex", "/a/.codex/skills/foo", SyncMode::Copy, Some("hash-1")),
            ],
            vec![skill(1, "foo", "/central/foo")],
        ),
    );
    assert!(out.is_empty(), "同一 skill 的双模式部署为健康态: {out:?}");
}

// ---------------------------------------------------------------------------
// 健康态与确定性
// ---------------------------------------------------------------------------

#[test]
fn healthy_state_yields_no_discrepancies() {
    let mut link = link_entry("claude", "/a/.claude/skills/foo", "/home/u/.skillbox/skills/foo", false);
    link.state = TargetState::LinkToSource;
    link.source_skill_id = Some(SkillId(1));
    let mut copy = entry(TargetState::RealDir, "codex", "/a/.codex/skills/foo");
    copy.content_hash = Some("hash-1".into());

    let out = reconcile(
        &view(vec![link, copy]),
        &index(
            vec![
                record(1, "claude", "/a/.claude/skills/foo", SyncMode::Symlink, None),
                record(1, "codex", "/a/.codex/skills/foo", SyncMode::Copy, Some("hash-1")),
            ],
            vec![skill(1, "foo", "/home/u/.skillbox/skills/foo")],
        ),
    );
    assert!(out.is_empty(), "健康态不应有差异: {out:?}");
}

#[test]
fn output_is_deterministic_and_sorted_by_path_then_kind() {
    let mut dir = entry(TargetState::RealDir, "codex", "/a/.codex/skills/zeta");
    dir.content_hash = Some("h1".into());
    let broken = link_entry("claude", "/a/.claude/skills/beta", "/gone", true);
    let foreign = link_entry("claude", "/a/.claude/skills/alpha", "/elsewhere", false);

    let disk = view(vec![dir, broken, foreign]);
    let idx = index(vec![], vec![]);
    let first = reconcile(&disk, &idx);
    let second = reconcile(&disk, &idx);

    // 同输入恒同输出（排序稳定，含 kind 次序）。
    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.issue_id(), b.issue_id());
        assert_eq!(a.detail, b.detail);
    }
    // 按 path 排序。
    let paths: Vec<_> = first.iter().map(|d| d.path.clone()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
}

#[test]
fn affected_paths_includes_related() {
    let a = link_entry("claude", "/a/.claude/skills/foo", "/central/foo", false);
    let mut b = entry(TargetState::RealDir, "codex", "/a/.codex/skills/foo");
    b.content_hash = Some("h".into());
    let out = reconcile(&view(vec![a, b]), &index(vec![], vec![]));
    let dup = out.iter().find(|d| d.kind == DiscrepancyKind::DuplicateName).unwrap();
    assert_eq!(
        dup.affected_paths(),
        vec![
            std::path::Path::new("/a/.claude/skills/foo"),
            std::path::Path::new("/a/.codex/skills/foo"),
        ]
    );
}
