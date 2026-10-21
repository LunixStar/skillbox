//! 链接部署与显式覆盖授权（TASKBOOK §5，K3）。
//!
//! 这是全项目最容易造成用户数据丢失的一块，写得保守：
//!
//! 1. 【硬约束 §5.1】目标分类一律 `symlink_metadata()`（lstat 语义），
//!    禁止 `is_dir()`——它会跟随链接，把 junction / 目录 symlink 误判为
//!    真实目录，进而走 `remove_dir_all` 造成数据丢失。
//! 2. 【硬约束 §5.2】`Recorded { mode: "symlink" }` 绝不授权删除当前是
//!    `RealDir` 的目标——DB 记录只代表历史意图，不代表磁盘现状。
//!    授权判定见 `policy::overwrite_authorized` 的显式 16 臂决策表。
//! 3. 【写入纪律 §5.3】先在目标同目录（= 同卷）写临时路径，全部可失败的
//!    重活完成后才动旧目标；旧目标先整体 rename 进 `trash/`（v1 不做硬
//!    删除），最后同卷原子 rename 提交。写盘前 mute 文件监听（RAII 成对）。
//! 4. 授权拒绝是全有或全无：任何冲突都发生在写盘之前，携带逐路径
//!    `conflicts` 清单（§2.3），一次 `deploy` 调用零写入。
//! 5. 链接回退链：symlink → junction（Windows）→ tracked copy
//!    （copy 必须写 `.skillbox-source.json` 记录源路径，§5.2）。
//! 6. 执行期重分类（TOCTOU 防护）：precheck 与执行之间磁盘可能变化，
//!    执行时对每个目标重新分类并以现状为准；现状超出 precheck 授权
//!    范围 → 拒写（deploy `Failed`、undeploy `Skipped`），绝不按快照删。
//!
//! 模块拆分（单文件 ≤600 行红线，TASKBOOK §2.5）：类型与公共出口在本
//! 文件；来源标记与目标分类 → `classify`，授权决策表 → `policy`，
//! 写入纪律原语 → `write`，deploy/undeploy 执行 → `ops`。

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{ReplacePolicy, SkillId, SyncMode, ToolId};
use skillbox_ports::{Clock, FileSystem, Linker, WatchMute};

pub(crate) mod classify;
mod ops;
pub(crate) mod policy;
pub(crate) mod write;

pub use ops::{deploy, undeploy};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_undeploy;

// ---------------------------------------------------------------------------
// 请求 / 报告类型
// ---------------------------------------------------------------------------

/// 单个部署目标。
#[derive(Debug, Clone)]
pub struct DeployTarget {
    pub tool_id: ToolId,
    /// agent 目录中的目标路径（通常 `<skills>/<skill-name>`）。
    pub path: PathBuf,
    /// 请求模式：`Symlink` = 链接优先（symlink → junction → tracked copy
    /// 回退，§5.2）；`Copy` = 显式副本。
    pub mode: SyncMode,
}

/// 部署请求。
///
/// 【注意】本类型不可序列化：注入的端口实现只能随请求传递；IPC 层使用
/// 独立 DTO 组装，禁止把本类型直接暴露给前端（同 `ScanRequest` 约定）。
pub struct DeployRequest<'a> {
    pub skill_id: SkillId,
    /// 中央库正本路径（`~/.skillbox/skills/<name>`）。
    pub source: PathBuf,
    /// 逐工具目标（按请求顺序执行与回报）。
    pub targets: Vec<DeployTarget>,
    /// 覆盖授权（§5.2）。
    pub policy: ReplacePolicy<'a>,
    /// trash 根（`~/.skillbox/trash`）；v1 所有删除先进这里。
    pub trash_root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    /// 文件监听；None = 无监听可 mute。
    pub watcher: Option<Arc<dyn WatchMute>>,
}

impl fmt::Debug for DeployRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 端口句柄无有意义的 Debug，只呈现行为相关字段。
        f.debug_struct("DeployRequest")
            .field("skill_id", &self.skill_id)
            .field("source", &self.source)
            .field("targets", &self.targets)
            .field("policy", &self.policy)
            .field("trash_root", &self.trash_root)
            .field("fs", &"<dyn FileSystem>")
            .field("clock", &"<dyn Clock>")
            .field("linker", &"<dyn Linker>")
            .field("watcher", &self.watcher.is_some())
            .finish()
    }
}

/// 单个取消部署目标。
#[derive(Debug, Clone)]
pub struct UndeployTarget {
    pub tool_id: ToolId,
    pub path: PathBuf,
}

/// 取消部署请求。`source` 用于判定链接是否指向本源、副本标记是否一致；
/// 只移除指向本源的链接与带来源标记（`.skillbox-source.json` 且 source
/// 一致）的 copy 副本，其余一律拒绝（删除语义两级：取消部署 ≠ 删除
/// Skill，§10）。
pub struct UndeployRequest {
    pub skill_id: SkillId,
    pub source: PathBuf,
    pub targets: Vec<UndeployTarget>,
    pub trash_root: PathBuf,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    pub watcher: Option<Arc<dyn WatchMute>>,
}

impl fmt::Debug for UndeployRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UndeployRequest")
            .field("skill_id", &self.skill_id)
            .field("source", &self.source)
            .field("targets", &self.targets)
            .field("trash_root", &self.trash_root)
            .field("fs", &"<dyn FileSystem>")
            .field("clock", &"<dyn Clock>")
            .field("linker", &"<dyn Linker>")
            .field("watcher", &self.watcher.is_some())
            .finish()
    }
}

/// 单个工具上的取消部署结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolUndeployOutcome {
    /// 已移入 trash（指向本源的链接，或带来源标记的副本）。
    Removed,
    /// 跳过（零写盘）。`reason` 为稳定约定值，UI 可据此分组：
    /// - `already_absent`：目标本就不存在（幂等终态）；
    /// - `state_changed`：执行时磁盘现状与 precheck 预期不符，
    ///   拒绝按快照删除（TOCTOU 防护）。
    Skipped { reason: String },
    /// 失败（含原因）。
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUndeployResult {
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub outcome: ToolUndeployOutcome,
}

/// 取消部署报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndeployReport {
    pub skill_id: SkillId,
    pub results: Vec<ToolUndeployResult>,
}
