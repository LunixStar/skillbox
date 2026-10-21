//! # skillbox-engine
//!
//! SkillBox 内核：scan / reconcile / install / deploy / doctor / audit / cost
//! （TASKBOOK §2.2）。本文件在 Phase 0 只钉死**函数签名与契约**——
//! 实现体一律 `todo!()`（"先签约、后实现"）。
//!
//! 【硬约束】
//! - 本 crate 禁止引用 tauri 任何命名空间；必须能在 `cargo test` 下
//!   不链接 WebView 完整跑通（自检：engine 源码中 tauri 命名空间前缀
//!   计数必须为 0）。
//! - 依赖方向：只依赖 domain / ports / adapters / store，禁止依赖 ipc / app。
//! - 所有 IO 经由 ports 注入（tmpfs + 假时钟可完整单测）。

pub mod deploy;
pub mod doctor;
pub mod doc;
pub mod env;
pub mod fix;
pub mod install;
pub mod ops;
pub mod reconcile;
pub mod scan;
pub mod system;
pub mod types;

pub use deploy::{
    deploy, undeploy, DeployRequest, DeployTarget, ToolUndeployOutcome, ToolUndeployResult,
    UndeployReport, UndeployRequest, UndeployTarget,
};
pub use doc::{doc_read, doc_write, validate_meta, DocCheckReport, DocIssue, DocWriteOutcome};
pub use doctor::{doctor, DoctorOpts};
pub use env::{
    doctor_targets, health_context, scan_roots, tool_deploy_dir, tools, HealthContextInputs,
    ToolInfo,
};
pub use fix::{fix, FixRequest};
pub use install::{install, InstallContext, InstallOutcome};
pub use ops::{
    deploy_set_mode, deploy_to_tools, remove_skill, revoke, DeployAction, DeployOutcome,
    DeployPreview, OpsContext, RemoveMode, RemoveSkillOutcome, RemoveSkillPreview, RemoveSkillReport,
    RevokeAction, RevokeOutcome, RevokePreview, RevokeReport, SetModeOutcome, SetModePreview,
    ToolDeployDir, ToolDeployPreview, ToolRevokePreview,
};
pub use reconcile::reconcile;
pub use scan::scan;
pub use system::{
    cache_clear, cache_inspect, trash_list, trash_restore, CacheClearReport, CacheEntry,
    TrashEntry, TrashRestoreOutcome,
};
pub use types::*;

// ipc 禁止直接依赖 adapters（§2.1 依赖图）；环境类型经 engine 转发。
pub use skillbox_adapters::{PathScope, ToolOverrides};

#[cfg(test)]
mod test_env;

use skillbox_domain::{AppResult, SkillId, ToolId};

// ---------------------------------------------------------------------------
// scan —— 扫描（§7）
// ---------------------------------------------------------------------------
//
// 实现与 §7 全部 11 条约束的逐条标注见 `engine/src/scan.rs` 顶部清单；
// 单测（FakeFileSystem / FakeClock 注入）见 `engine/src/scan/tests.rs`。

// ---------------------------------------------------------------------------
// reconcile —— 对账（§6）
// ---------------------------------------------------------------------------
//
// 实现见 `engine/src/reconcile.rs`（纯计算，不写盘）；
// 单测见 `engine/src/reconcile/tests.rs`（覆盖 §6.2 全部 7 类差异）。

// ---------------------------------------------------------------------------
// doctor / fix —— 体检与修复（§6.3）
// ---------------------------------------------------------------------------
//
// 实现见 `engine/src/doctor.rs`（只读体检）与 `engine/src/fix.rs`（逐项修复，
// 危险项逐个 NeedsConfirmation，删除先进 trash）；单测分别见
// `engine/src/doctor/tests.rs` 与 `engine/src/fix/tests.rs`。

// ---------------------------------------------------------------------------
// install / deploy —— 入库与部署分离（K5）
// ---------------------------------------------------------------------------

// 入库实现见 `engine/src/install.rs`（只写中央库，不碰 agent 目录；
// keep-in-place / git / dry_run 见其模块文档）；单测同文件。

// 部署 / 取消部署：链接部署与显式覆盖授权（§5）已实现，见
// deploy / undeploy 与 `engine/src/deploy.rs`（含硬约束清单）；
// 单测（真实文件系统 + 注入失败点 + 固定时钟）见
// `engine/src/deploy/tests.rs`。
// ---------------------------------------------------------------------------
// 专业面板（§9）
// ---------------------------------------------------------------------------

/// 安全扫描：对单个 skill 做纯静态规则匹配（§9.1 八条规则）。
///
/// 契约：
/// - 【硬约束】不得执行 skill 内任何脚本，纯静态文本匹配。
/// - 只提示不阻断安装；`risk_level` 取命中的最高严重度。
pub fn audit_skill(skill: SkillId) -> AppResult<SecurityReport> {
    let _ = &skill;
    todo!("Phase 4：实现八条静态安全规则（engine/src/audit.rs）")
}

/// 上下文成本：统计各已启用工具常驻 skill 的 description 成本与触发冲突。
///
/// 契约：
/// - token 折算按 ~3.5 字符/token 估算，输出中明确标注。
/// - 冲突检测：description 分词后两两 Jaccard ≥ 0.35 的组。
pub fn context_cost(tools: &[ToolId]) -> AppResult<CostReport> {
    let _ = tools;
    todo!("Phase 4：实现上下文成本面板（engine/src/cost.rs）")
}
