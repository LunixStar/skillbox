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

pub mod types;

pub use types::*;

use skillbox_domain::{AppResult, ReplacePolicy, SkillId, ToolId};

// ---------------------------------------------------------------------------
// scan —— 扫描（§7）
// ---------------------------------------------------------------------------

/// 扫描 agent 目录与中央库，发现 `SKILL.md` 目录包。
///
/// 契约（§7 全部 11 条硬约束）：
/// 1. 不读文件内容：候选判定只看 `read_dir` + `symlink_metadata`；
///    `SKILL.md` 只在确定为 skill 目录后截断读取前 8 KB。
/// 2. 递归深度 ≤ `ScanRequest::depth_limit`（默认 6）。
/// 3. 黑名单目录跳过（内置 + 用户追加）。
/// 4. 不跟随符号链接（防循环与越权）。
/// 5. 跳过网络卷 / 可移动卷。
/// 6. 增量指纹 `(mtime, size, 子项数)` 未变则复用 `scan_cache`。
/// 7. 并发工作线程 `min(num_cpus, 4)`，禁止无界 spawn。
/// 8. 可中断：每个目录检查一次 `cancel`，取消后 200 ms 内返回部分结果。
/// 9. 流式进度：每 50 个目录回调一次 `on_progress`。
/// 10. 内存：峰值 RSS 增量 ≤ 80 MB，分批产出。
/// 11. 扫描过程禁止任何网络 / git 操作。
pub fn scan(
    req: ScanRequest,
    cancel: CancellationToken,
    on_progress: impl Fn(ScanProgress),
) -> AppResult<ScanReport> {
    let _ = (&req, &cancel, &on_progress);
    todo!("Phase 1：实现 §7 全部 11 条扫描约束（engine/src/scan.rs）")
}

// ---------------------------------------------------------------------------
// reconcile —— 对账（§6）
// ---------------------------------------------------------------------------

/// 以磁盘为真相，将磁盘现状与 DB 索引比对，产出差异清单。
///
/// 契约：
/// - 纯计算，不写盘、不修改 DB。
/// - DB 索引只作为"上次意图"参与比对；任何 UI 状态必须由磁盘推算（K4）。
/// - 覆盖 §6.2 全部 7 类差异：UnmanagedRealDir / BrokenLink / ForeignLink /
///   MissingTarget / UnrecordedDeployment / CopyDrift / DuplicateName。
pub fn reconcile(disk: &DiskView, index: &IndexView) -> Vec<Discrepancy> {
    let _ = (disk, index);
    todo!("Phase 1：实现 §6.2 七类差异的纯计算对账（engine/src/reconcile.rs）")
}

// ---------------------------------------------------------------------------
// doctor / fix —— 体检与修复（§6.3）
// ---------------------------------------------------------------------------

/// 体检：只读，可在任何时候安全运行；返回带 severity 的问题清单与建议动作。
///
/// 契约：
/// - 绝不写盘；问题状态由磁盘 reconcile 推算，不读 DB 展示字段。
/// - `opts.min_severity` 过滤严重度；`opts.tools` 限定工具范围。
pub fn doctor(opts: DoctorOpts) -> AppResult<DoctorReport> {
    let _ = &opts;
    todo!("Phase 1：实现只读体检（engine/src/doctor.rs）")
}

/// 修复：接受**显式选中的问题 id 列表**，逐项执行，返回逐项结果。
///
/// 契约：
/// - 【硬约束】禁止"一键修复全部"作为默认行为；危险项（涉及 `RealDir` /
///   `ForeignLink`）必须逐个二次确认（`FixOutcome::NeedsConfirmation`）。
/// - `policy` 同样受 §5.2 授权模型约束：`NoClobber` 下任何可能删除
///   用户数据的操作必须拒绝。
/// - 每个会删除的路径，删除前先进 `~/.skillbox/trash/` 暂存（§5.3）。
pub fn fix(items: &[IssueId], policy: ReplacePolicy<'_>) -> AppResult<FixReport> {
    let _ = (items, &policy);
    todo!("Phase 1：实现逐项修复（engine/src/doctor.rs）")
}

// ---------------------------------------------------------------------------
// install / deploy —— 入库与部署分离（K5）
// ---------------------------------------------------------------------------

/// 入库：只写中央库 `~/.skillbox/skills/`，**不碰任何 agent 目录**。
///
/// 契约：
/// - 与 deploy 永不合并（K5 硬约束）。
/// - 返回新 skill 的 id；frontmatter `name` 必须符合
///   `^[a-z0-9][a-z0-9-]{1,63}$`，否则 `InvalidInput`。
/// - 首启 keep-in-place 登记同样走这里（canonical_path 指向首次发现位置）。
pub fn install(req: InstallRequest) -> AppResult<SkillId> {
    let _ = &req;
    todo!("Phase 1：实现入库（engine/src/install.rs）")
}

/// 部署：把中央库正本以链接（symlink → junction → tracked copy 回退链）
/// 写入指定 agent 目录。
///
/// 契约：
/// - 写入纪律（§5.3）：先写临时目录 → 同卷原子 rename；写前 mute 文件监听。
/// - 授权（§5.2）：`TargetState` 非 overwritable 的目标在未获 `UserConfirmed`
///   时返回 `ToolDeployOutcome::NeedsConfirmation` 或 `TargetConflict` 错误
///   （携带 `details.conflicts` 路径清单）。
/// - 退化为 copy 时必须写入 `.skillbox-source.json` 记录源路径，且 UI
///   标注"副本模式（非实时同步）"。
pub fn deploy(
    skill: SkillId,
    tools: &[ToolId],
    policy: ReplacePolicy<'_>,
) -> AppResult<DeployReport> {
    let _ = (skill, tools, &policy);
    todo!("Phase 1：实现链接部署（engine/src/deploy.rs）")
}

/// 取消部署：从指定 agent 目录移除链接，正本仍在中央库。
///
/// 契约：只允许删除"我们风格的链接"；对 `RealDir` / `ForeignLink`
/// 一律拒绝（删除语义两级：取消部署 ≠ 删除 Skill，§10）。
pub fn undeploy(skill: SkillId, tools: &[ToolId]) -> AppResult<()> {
    let _ = (skill, tools);
    todo!("Phase 1：实现取消部署（engine/src/deploy.rs）")
}

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
