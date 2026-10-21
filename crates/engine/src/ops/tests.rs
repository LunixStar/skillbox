//! ops 单测：编排层全流程（真实文件系统 + 内存索引 + 固定时钟）。

use std::sync::Arc;

use skillbox_domain::{ErrorKind, SkillId, SyncMode, ToolId};
use skillbox_ports::{EntryKind, FileSystem};

use super::*;
use crate::deploy::classify::SOURCE_MARKER_FILE;
use crate::test_env::{CountingFs, HealthTestEnv};

fn ops_ctx(env: &HealthTestEnv) -> OpsContext {
    OpsContext {
        central_root: env.central_root.clone(),
        trash_root: env.trash.clone(),
        fs: env.fs.clone(),
        clock: env.clock.clone(),
        linker: env.linker.clone(),
        index: env.store.clone(),
        watcher: None,
    }
}

fn ops_ctx_with_fs(env: &HealthTestEnv, fs: Arc<dyn FileSystem>) -> OpsContext {
    OpsContext {
        central_root: env.central_root.clone(),
        trash_root: env.trash.clone(),
        fs,
        clock: env.clock.clone(),
        linker: env.linker.clone(),
        index: env.store.clone(),
        watcher: None,
    }
}

fn dir_of(env: &HealthTestEnv, tool: &str) -> ToolDeployDir {
    ToolDeployDir { tool_id: ToolId(tool.into()), dir: env.skills_dir(tool) }
}

/// 造 skill（中央正本 + 索引行），返回 id。
fn skill_of(env: &HealthTestEnv, name: &str) -> SkillId {
    env.make_central_skill(name, &[]);
    env.register(name, &[])
}

#[test]
fn deploy_to_tools_creates_link_and_record() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    let out = deploy_to_tools(
        id,
        &[dir_of(&env, "claude"), dir_of(&env, "codex")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let DeployOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert_eq!(report.results.len(), 2);
    for tool in ["claude", "codex"] {
        let target = env.target_path(tool, "alpha");
        assert!(env.fs.exists(&target), "{tool} 目标应存在");
    }
    let targets = env.store.targets();
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().all(|t| t.mode == SyncMode::Symlink));
    assert!(targets.iter().all(|t| t.source_hash.is_none()));
}

#[test]
fn deploy_to_tools_dry_run_preview_zero_writes() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let (fs, writes) = CountingFs::wrap(env.fs.clone());
    let ctx = ops_ctx_with_fs(&env, fs);
    let out = deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        true,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let DeployOutcome::Preview(preview) = out else { panic!("应为 Preview") };
    assert_eq!(preview.results.len(), 1);
    assert_eq!(preview.results[0].action, DeployAction::Deploy { mode: SyncMode::Symlink });
    assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(!env.fs.exists(&env.target_path("claude", "alpha")));
    assert!(env.store.targets().is_empty());
}

#[test]
fn deploy_to_tools_conflict_then_overwrite() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    // 用户数据占位：无标记真实目录。
    let foreign = env.target_path("claude", "alpha");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("user.txt"), "user data").unwrap();

    // 默认 NoClobber：整体拒绝（零写入）。
    let err = deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::TargetConflict);
    assert_eq!(
        std::fs::read_to_string(foreign.join("user.txt")).unwrap(),
        "user data"
    );

    // dry_run 预览给 Conflict，而非报错。
    let out = deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        true,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let DeployOutcome::Preview(preview) = out else { panic!("应为 Preview") };
    assert!(matches!(preview.results[0].action, DeployAction::Conflict { .. }));

    // overwrite = true：用户确认级授权，旧目录整体入 trash。
    let out = deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        true,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let DeployOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert_eq!(
        report.results[0].outcome,
        crate::types::ToolDeployOutcome::Deployed { mode: SyncMode::Symlink }
    );
    assert!(env.fs.exists(&env.target_path("claude", "alpha")));
    assert_eq!(env.store.targets().len(), 1);
}

#[test]
fn deploy_to_tools_missing_skill_not_found() {
    let env = HealthTestEnv::new();
    let ctx = ops_ctx(&env);
    let err = deploy_to_tools(
        SkillId(999),
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[test]
fn revoke_removes_targets_and_clears_records() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude"), dir_of(&env, "codex")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = revoke(id, &[], false, &env.index_view(), &ctx).unwrap();
    let RevokeOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert_eq!(report.tools_cleared.len(), 2);
    assert!(!env.fs.exists(&env.target_path("claude", "alpha")));
    assert!(!env.fs.exists(&env.target_path("codex", "alpha")));
    assert!(env.store.targets().is_empty());
    assert!(env.fs.exists(&env.central_root.join("alpha")), "正本不动");
}

#[test]
fn revoke_selected_tool_only() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude"), dir_of(&env, "codex")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = revoke(
        id,
        &[ToolId("claude".into())],
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let RevokeOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert_eq!(report.tools_cleared, vec![ToolId("claude".into())]);
    assert!(!env.fs.exists(&env.target_path("claude", "alpha")));
    assert!(env.fs.exists(&env.target_path("codex", "alpha")));
    let targets = env.store.targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].tool_id, ToolId("codex".into()));
}

#[test]
fn revoke_dry_run_preview_zero_writes() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let (fs, writes) = CountingFs::wrap(env.fs.clone());
    let ctx = ops_ctx_with_fs(&env, fs);
    let out = revoke(id, &[], true, &env.index_view(), &ctx).unwrap();
    let RevokeOutcome::Preview(preview) = out else { panic!("应为 Preview") };
    assert_eq!(preview.results.len(), 1);
    assert_eq!(preview.results[0].action, RevokeAction::Remove);
    assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(env.fs.exists(&env.target_path("claude", "alpha")));
    assert_eq!(env.store.targets().len(), 1);
}

#[test]
fn remove_skill_undeploy_keeps_central_and_skill_row() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = remove_skill(id, RemoveMode::Undeploy, false, &env.index_view(), &ctx).unwrap();
    let RemoveSkillOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert!(!report.index_deleted);
    assert_eq!(report.tools_cleared.len(), 1);
    assert!(env.fs.exists(&env.central_root.join("alpha")));
    assert!(!env.fs.exists(&env.target_path("claude", "alpha")));
    assert_eq!(env.store.skills().len(), 1);
    assert!(env.store.targets().is_empty());
}

#[test]
fn remove_skill_delete_trashes_central_and_deletes_index() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = remove_skill(id, RemoveMode::Delete, false, &env.index_view(), &ctx).unwrap();
    let RemoveSkillOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert!(report.index_deleted);
    assert!(report.central_trashed_to.is_some());
    assert!(!env.fs.exists(&env.central_root.join("alpha")));
    assert!(!env.fs.exists(&env.target_path("claude", "alpha")));
    assert!(env.store.skills().is_empty());
    assert!(env.store.targets().is_empty());
    // 正本可在 trash 中找回。
    let trashed = report.central_trashed_to.unwrap();
    assert!(env.fs.symlink_metadata(&trashed).unwrap().kind == EntryKind::Dir);
}

#[test]
fn remove_skill_delete_dry_run_zero_writes() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let (fs, writes) = CountingFs::wrap(env.fs.clone());
    let ctx = ops_ctx_with_fs(&env, fs);
    let out = remove_skill(id, RemoveMode::Delete, true, &env.index_view(), &ctx).unwrap();
    let RemoveSkillOutcome::Preview(preview) = out else { panic!("应为 Preview") };
    assert!(preview.will_delete_central);
    assert!(preview.will_delete_index);
    assert_eq!(preview.revoke.results[0].action, RevokeAction::Remove);
    assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(env.fs.exists(&env.central_root.join("alpha")));
    assert!(env.fs.exists(&env.target_path("claude", "alpha")));
    assert_eq!(env.store.skills().len(), 1);
    assert_eq!(env.store.targets().len(), 1);
}

#[test]
fn deploy_set_mode_switches_link_to_copy() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = deploy_set_mode(
        id,
        ToolId("claude".into()),
        SyncMode::Copy,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let SetModeOutcome::Applied(report) = out else { panic!("应为 Applied") };
    assert_eq!(
        report.results[0].outcome,
        crate::types::ToolDeployOutcome::Deployed { mode: SyncMode::Copy }
    );
    let target = env.target_path("claude", "alpha");
    assert_eq!(env.fs.symlink_metadata(&target).unwrap().kind, EntryKind::Dir);
    assert!(target.join(SOURCE_MARKER_FILE).exists(), "tracked copy 必须带来源标记");
    let targets = env.store.targets();
    assert_eq!(targets[0].mode, SyncMode::Copy);
    assert!(targets[0].source_hash.is_some(), "copy 记录必须带 source_hash");
}

#[test]
fn deploy_set_mode_dry_run_preview() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    deploy_to_tools(
        id,
        &[dir_of(&env, "claude")],
        SyncMode::Symlink,
        false,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let out = deploy_set_mode(
        id,
        ToolId("claude".into()),
        SyncMode::Copy,
        true,
        &env.index_view(),
        &ctx,
    )
    .unwrap();
    let SetModeOutcome::Preview(preview) = out else { panic!("应为 Preview") };
    assert_eq!(preview.from, SyncMode::Symlink);
    assert_eq!(preview.to, SyncMode::Copy);
    // 磁盘仍是链接，未被切换。
    let target = env.target_path("claude", "alpha");
    assert_eq!(env.fs.symlink_metadata(&target).unwrap().kind, EntryKind::Symlink);
}

#[test]
fn deploy_set_mode_missing_record_not_found() {
    let env = HealthTestEnv::new();
    let id = skill_of(&env, "alpha");
    let ctx = ops_ctx(&env);
    let err = deploy_set_mode(
        id,
        ToolId("codex".into()),
        SyncMode::Copy,
        false,
        &env.index_view(),
        &ctx,
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}
