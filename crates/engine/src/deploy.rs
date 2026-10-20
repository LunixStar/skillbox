//! 链接部署与显式覆盖授权（TASKBOOK §5，K3）。
//!
//! 这是全项目最容易造成用户数据丢失的一块，写得保守：
//!
//! 1. 【硬约束 §5.1】目标分类一律 `symlink_metadata()`（lstat 语义），
//!    禁止 `is_dir()`——它会跟随链接，把 junction / 目录 symlink 误判为
//!    真实目录，进而走 `remove_dir_all` 造成数据丢失。
//! 2. 【硬约束 §5.2】`Recorded { mode: "symlink" }` 绝不授权删除当前是
//!    `RealDir` 的目标——DB 记录只代表历史意图，不代表磁盘现状。
//!    授权判定见 [`overwrite_authorized`] 的显式 16 臂决策表。
//! 3. 【写入纪律 §5.3】先在目标同目录（= 同卷）写临时路径，全部可失败的
//!    重活完成后才动旧目标；旧目标先整体 rename 进 `trash/`（v1 不做硬
//!    删除），最后同卷原子 rename 提交。写盘前 mute 文件监听（RAII 成对）。
//! 4. 授权拒绝是全有或全无：任何冲突都发生在写盘之前，携带逐路径
//!    `conflicts` 清单（§2.3），一次 `deploy` 调用零写入。
//! 5. 链接回退链：symlink → junction（Windows）→ tracked copy
//!    （copy 必须写 `.skillbox-source.json` 记录源路径，§5.2）。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{
    AppError, AppResult, PathConflict, ReplacePolicy, SkillId, SyncMode, TargetState, ToolId,
};
use skillbox_ports::{Clock, EntryKind, FileSystem, Linker, WatchMute};

use crate::types::{DeployReport, ToolDeployOutcome, ToolDeployResult};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// tracked-copy 来源标记（§5.2）
// ---------------------------------------------------------------------------

/// tracked-copy 部署的来源标记（写入目标内 `.skillbox-source.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMarker {
    /// 中央库正本路径。
    pub source: PathBuf,
    /// 写入时间（epoch 毫秒）。
    pub written_at_ms: i64,
}

const SOURCE_MARKER_FILE: &str = ".skillbox-source.json";

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

/// 取消部署请求。`source` 用于判定链接是否指向本源；只移除指向本源的
/// 链接，对 `RealDir` / `ForeignLink` / `RealFile` 一律拒绝
/// （删除语义两级：取消部署 ≠ 删除 Skill，§10）。
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
    /// 链接已移入 trash。
    Removed,
    /// 目标本就不存在（幂等成功）。
    AlreadyAbsent,
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

// ---------------------------------------------------------------------------
// 目标分类（§5.1）
// ---------------------------------------------------------------------------

/// 目标路径现状分类（lstat 语义）。
///
/// 【硬约束 §5.1】判定基于 `symlink_metadata()`，禁止 `is_dir()`。
pub fn classify_target(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    source: &Path,
    path: &Path,
) -> AppResult<TargetState> {
    match fs.symlink_metadata(path) {
        Ok(meta) => match meta.kind {
            EntryKind::Symlink => {
                if resolves_to_source(fs, linker, source, path) {
                    Ok(TargetState::LinkToSource)
                } else {
                    // 指向别处或悬空（canonicalize 失败）：用户配置。
                    Ok(TargetState::ForeignLink)
                }
            }
            EntryKind::Dir => Ok(TargetState::RealDir),
            EntryKind::File | EntryKind::Other => Ok(TargetState::RealFile),
        },
        Err(e) => {
            // AppError 未保留 io ErrorKind，用 exists 兜底区分 NotFound；
            // 悬空链接 lstat 成功，不会走到这里。
            if !fs.exists(path) {
                Ok(TargetState::Absent)
            } else {
                Err(e)
            }
        }
    }
}

/// 链接是否解析到 `source`（两边 canonicalize 后比较；任何一步失败即否）。
fn resolves_to_source(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    source: &Path,
    link: &Path,
) -> bool {
    let Ok(raw_target) = linker.read_link(link) else { return false };
    let Ok(source_canon) = fs.canonicalize(source) else { return false };
    let Ok(target_canon) = fs.canonicalize(&raw_target) else { return false };
    source_canon == target_canon
}

// ---------------------------------------------------------------------------
// 授权决策表（§5.2）
// ---------------------------------------------------------------------------

/// §5.1/§5.2 决策表：`(TargetState, ReplacePolicy)` 是否授权覆盖。
///
/// 逐格展开（不用 if 链），让每一格都能被单测与 review 钉死。
/// 【硬约束】`RealDir × Recorded { mode: "symlink" }` 必须为 false：
/// DB 记录只代表历史意图，不代表磁盘现状。
#[allow(clippy::match_same_arms)]
fn overwrite_authorized(state: TargetState, policy: &ReplacePolicy<'_>) -> bool {
    match (state, policy) {
        // Absent：目标不存在，任何策略下写入都不可能损失数据。
        (TargetState::Absent, ReplacePolicy::NoClobber) => true,
        (TargetState::Absent, ReplacePolicy::Recorded { .. }) => true,
        (TargetState::Absent, ReplacePolicy::UserConfirmed) => true,
        // LinkToSource：链接已指向本源，重写等价幂等。
        (TargetState::LinkToSource, ReplacePolicy::NoClobber) => true,
        (TargetState::LinkToSource, ReplacePolicy::Recorded { .. }) => true,
        (TargetState::LinkToSource, ReplacePolicy::UserConfirmed) => true,
        // ForeignLink：用户配置（指向别处/悬空），仅显式确认可覆盖。
        (TargetState::ForeignLink, ReplacePolicy::NoClobber) => false,
        (TargetState::ForeignLink, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::ForeignLink, ReplacePolicy::UserConfirmed) => true,
        // RealDir：真实目录。Recorded 一律不授权——从中央库更新副本
        // 属 UserConfirmed 领域（§5.2 括注）。
        (TargetState::RealDir, ReplacePolicy::NoClobber) => false,
        (TargetState::RealDir, ReplacePolicy::Recorded { mode: "symlink" }) => false,
        (TargetState::RealDir, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::RealDir, ReplacePolicy::UserConfirmed) => true,
        // RealFile：真实文件，仅显式确认可覆盖。
        (TargetState::RealFile, ReplacePolicy::NoClobber) => false,
        (TargetState::RealFile, ReplacePolicy::Recorded { .. }) => false,
        (TargetState::RealFile, ReplacePolicy::UserConfirmed) => true,
    }
}

/// 未授权状态的冲突原因（进入 `details.conflicts`，UI"指着目录说话"）。
fn conflict_reason(state: TargetState) -> String {
    match state {
        TargetState::ForeignLink => "链接指向别处或悬空（用户配置），覆盖需 UserConfirmed".to_string(),
        TargetState::RealDir => "真实目录（可能含用户数据），覆盖需 UserConfirmed".to_string(),
        TargetState::RealFile => "真实文件，覆盖需 UserConfirmed".to_string(),
        TargetState::Absent | TargetState::LinkToSource => "无冲突".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 写入纪律原语（§5.3）
// ---------------------------------------------------------------------------

/// 写盘期间的监听静音守卫：构造时 mute，Drop 时 unmute（§5.3）。
/// 提前 return / 出错也保证成对。
struct MuteGuard<'a> {
    watcher: Option<&'a Arc<dyn WatchMute>>,
}

impl<'a> MuteGuard<'a> {
    fn new(watcher: Option<&'a Arc<dyn WatchMute>>) -> Self {
        if let Some(w) = watcher {
            w.mute();
        }
        MuteGuard { watcher }
    }
}

impl Drop for MuteGuard<'_> {
    fn drop(&mut self) {
        if let Some(w) = self.watcher {
            w.unmute();
        }
    }
}

/// 校验中央库正本：必须存在且为真实目录（拒绝链接/文件）。
fn validate_source(fs: &dyn FileSystem, source: &Path) -> AppResult<()> {
    if !fs.exists(source) {
        return Err(AppError::not_found(format!("中央库正本不存在: {}", source.display())));
    }
    match fs.symlink_metadata(source)?.kind {
        EntryKind::Dir => Ok(()),
        EntryKind::Symlink => Err(AppError::invalid_input(format!(
            "中央库正本本身是链接，拒绝部署: {}",
            source.display()
        ))),
        _ => Err(AppError::invalid_input(format!(
            "中央库正本不是目录: {}",
            source.display()
        ))),
    }
}

/// 在目标的父目录下生成未占用的临时路径（同父目录 = 同卷，§5.3）。
fn fresh_tmp(fs: &dyn FileSystem, clock: &dyn Clock, target: &Path) -> AppResult<PathBuf> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = target.parent().ok_or_else(|| {
        AppError::invalid_input(format!("目标路径没有父目录: {}", target.display()))
    })?;
    fs.create_dir_all(parent)?;
    loop {
        let tmp = parent.join(format!(
            ".skillbox-deploy-{}-{}",
            clock.now_ms(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        if !fs.exists(&tmp) {
            return Ok(tmp);
        }
    }
}

/// 把 `path` 整体 rename 进 `trash/<now_ms>/<basename>`（v1 无硬删除）。
/// 同卷 rename：跨卷直接报错，原路径保持原样（绝不删除式搬运）。
fn move_to_trash(
    fs: &dyn FileSystem,
    clock: &dyn Clock,
    trash_root: &Path,
    path: &Path,
) -> AppResult<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        AppError::invalid_input(format!("无法暂存没有文件名的路径: {}", path.display()))
    })?;
    let slot = trash_root.join(clock.now_ms().to_string());
    fs.create_dir_all(&slot)?;
    let mut dest = slot.join(name);
    let mut n: u32 = 0;
    while fs.exists(&dest) {
        n += 1;
        dest = slot.join(format!("{}-{n}", name.to_string_lossy()));
    }
    fs.rename(path, &dest).map_err(|e| {
        AppError::io(format!(
            "移入 trash 失败（可能跨卷；原路径未动）: {} -> {}: {e}",
            path.display(),
            dest.display()
        ))
    })?;
    Ok(dest)
}

/// 阶段一（链接）：在 `tmp` 上创建指向 `source` 的链接（回退链前两级）。
/// 两级都不可用时返回 Err，由调用方退化为 tracked copy。
fn stage_link(linker: &dyn Linker, source: &Path, tmp: &Path) -> AppResult<()> {
    if linker.supports_symlink() && linker.symlink_dir(source, tmp).is_ok() {
        return Ok(());
    }
    if linker.supports_junction() && linker.junction(source, tmp).is_ok() {
        return Ok(());
    }
    Err(AppError::io(format!(
        "无法为 {} 创建链接（symlink 与 junction 均不可用）",
        source.display()
    )))
}

/// 阶段一（副本）：递归复制 + 写来源标记（§5.2）。
fn stage_copy(
    fs: &dyn FileSystem,
    clock: &dyn Clock,
    source: &Path,
    tmp: &Path,
) -> AppResult<()> {
    fs.copy_dir_all(source, tmp)?;
    let marker = SourceMarker { source: source.to_path_buf(), written_at_ms: clock.now_ms() };
    let json = serde_json::to_vec_pretty(&marker)
        .map_err(|e| AppError::internal(format!("序列化 {SOURCE_MARKER_FILE} 失败: {e}")))?;
    fs.write(&tmp.join(SOURCE_MARKER_FILE), &json)
}

/// 清理残留的临时路径：链接 tmp 只删链接本身；目录 tmp 递归删
/// （tmp 属于我们，不涉及用户数据）。
fn cleanup_tmp(fs: &dyn FileSystem, linker: &dyn Linker, tmp: &Path) {
    let Ok(meta) = fs.symlink_metadata(tmp) else { return };
    match meta.kind {
        EntryKind::Symlink => {
            let _ = linker.remove_link(tmp);
        }
        _ => {
            let _ = fs.remove_dir_all(tmp);
        }
    }
}

// ---------------------------------------------------------------------------
// deploy
// ---------------------------------------------------------------------------

/// 部署：把中央库正本以链接（symlink → junction → tracked copy 回退链）
/// 写入指定 agent 目录（§5）。
///
/// - 授权拒绝全有或全无：冲突在写盘前收集并以 `TargetConflict` 返回，
///   携带逐路径 `conflicts` 清单，本调用零写入。
/// - 逐工具失败只影响该工具（`ToolDeployOutcome::Failed`），其余照常。
pub fn deploy(req: DeployRequest<'_>) -> AppResult<DeployReport> {
    validate_source(req.fs.as_ref(), &req.source)?;

    // 阶段一：分类 + 授权（全部冲突都发生在任何写盘之前）。
    let mut classified = Vec::with_capacity(req.targets.len());
    let mut conflicts = Vec::new();
    for target in &req.targets {
        let state =
            classify_target(req.fs.as_ref(), req.linker.as_ref(), &req.source, &target.path)?;
        if !overwrite_authorized(state, &req.policy) {
            conflicts.push(PathConflict {
                path: target.path.clone(),
                reason: conflict_reason(state),
            });
        }
        classified.push((target, state));
    }
    if !conflicts.is_empty() {
        return Err(AppError::target_conflict(
            "目标路径存在未授权覆盖的内容，已整体拒绝（未做任何写盘）",
            conflicts,
        ));
    }

    // 阶段二：逐目标执行。
    let mut results = Vec::with_capacity(classified.len());
    for (target, state) in classified {
        let outcome = deploy_one(&req, target, state);
        results.push(ToolDeployResult {
            tool_id: target.tool_id.clone(),
            path: target.path.clone(),
            outcome,
        });
    }
    Ok(DeployReport { skill_id: req.skill_id, results })
}

/// 单目标执行：幂等检查 → mute → tmp 准备 → 旧目标进 trash → 原子提交。
fn deploy_one(
    req: &DeployRequest<'_>,
    target: &DeployTarget,
    state: TargetState,
) -> ToolDeployOutcome {
    // 幂等：目标已是指向本源的链接且请求的就是链接模式。
    if state == TargetState::LinkToSource && target.mode == SyncMode::Symlink {
        return ToolDeployOutcome::AlreadyUpToDate;
    }

    let _mute = MuteGuard::new(req.watcher.as_ref());

    let tmp = match fresh_tmp(req.fs.as_ref(), req.clock.as_ref(), &target.path) {
        Ok(t) => t,
        Err(e) => return ToolDeployOutcome::Failed { reason: format!("创建临时路径失败: {e}") },
    };

    // 阶段一：全部可失败的重活先落在 tmp 上（旧目标尚未被触动）。
    let staged_mode = match target.mode {
        SyncMode::Symlink => {
            match stage_link(req.linker.as_ref(), &req.source, &tmp) {
                Ok(()) => SyncMode::Symlink,
                Err(_) => {
                    // 回退链末端：tracked copy（§5.2）。
                    match stage_copy(req.fs.as_ref(), req.clock.as_ref(), &req.source, &tmp) {
                        Ok(()) => SyncMode::Copy,
                        Err(e) => {
                            cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                            return ToolDeployOutcome::Failed {
                                reason: format!("部署准备失败: {e}"),
                            };
                        }
                    }
                }
            }
        }
        SyncMode::Copy => match stage_copy(req.fs.as_ref(), req.clock.as_ref(), &req.source, &tmp)
        {
            Ok(()) => SyncMode::Copy,
            Err(e) => {
                cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                return ToolDeployOutcome::Failed { reason: format!("部署准备失败: {e}") };
            }
        },
    };

    // 阶段二：旧目标整体进 trash（v1 无硬删除，§5.3）。
    let trashed = if state == TargetState::Absent {
        None
    } else {
        match move_to_trash(
            req.fs.as_ref(),
            req.clock.as_ref(),
            &req.trash_root,
            &target.path,
        ) {
            Ok(dest) => Some(dest),
            Err(e) => {
                cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
                return ToolDeployOutcome::Failed {
                    reason: format!("旧目标暂存 trash 失败（目标未动）: {e}"),
                };
            }
        }
    };

    // 阶段三：同卷原子 rename 提交（§5.3）。
    if let Err(e) = req.fs.rename(&tmp, &target.path) {
        cleanup_tmp(req.fs.as_ref(), req.linker.as_ref(), &tmp);
        let mut reason = format!("提交失败: {e}");
        // 已暂存的旧目标尽力回滚到原位（trash 副本保留，双保险）。
        if let Some(dest) = &trashed {
            match req.fs.rename(dest, &target.path) {
                Ok(()) => reason.push_str("；旧目标已回滚原位"),
                Err(re) => reason.push_str(&format!(
                    "；旧目标回滚失败，仍在 trash: {}（{re}）",
                    dest.display()
                )),
            }
        }
        return ToolDeployOutcome::Failed { reason };
    }

    ToolDeployOutcome::Deployed { mode: staged_mode }
}

// ---------------------------------------------------------------------------
// undeploy
// ---------------------------------------------------------------------------

/// 取消部署：从指定 agent 目录移除链接，正本仍在中央库。
///
/// 只移除指向 `source` 的链接（先入 trash）；对 `RealDir` / `ForeignLink` /
/// `RealFile` 一律 `TargetConflict` 拒绝（删除语义两级，§10）。
pub fn undeploy(req: UndeployRequest) -> AppResult<UndeployReport> {
    // 阶段一：分类 + 拒绝判定（全有或全无，写盘前完成）。
    let mut classified = Vec::with_capacity(req.targets.len());
    let mut conflicts = Vec::new();
    for target in &req.targets {
        let state =
            classify_target(req.fs.as_ref(), req.linker.as_ref(), &req.source, &target.path)?;
        if !matches!(state, TargetState::Absent | TargetState::LinkToSource) {
            conflicts.push(PathConflict {
                path: target.path.clone(),
                reason: format!("{}；取消部署只移除指向本源的链接", conflict_reason(state)),
            });
        }
        classified.push((target, state));
    }
    if !conflicts.is_empty() {
        return Err(AppError::target_conflict(
            "取消部署只允许移除指向本源的链接；以下目标拒绝自动处理（未做任何写盘）",
            conflicts,
        ));
    }

    // 阶段二：逐目标执行。
    let mut results = Vec::with_capacity(classified.len());
    for (target, state) in classified {
        let outcome = match state {
            TargetState::Absent => ToolUndeployOutcome::AlreadyAbsent,
            TargetState::LinkToSource => {
                let _mute = MuteGuard::new(req.watcher.as_ref());
                match move_to_trash(
                    req.fs.as_ref(),
                    req.clock.as_ref(),
                    &req.trash_root,
                    &target.path,
                ) {
                    Ok(_) => ToolUndeployOutcome::Removed,
                    Err(e) => {
                        ToolUndeployOutcome::Failed { reason: format!("移入 trash 失败: {e}") }
                    }
                }
            }
            // 阶段一已排除；防御性兜底。
            TargetState::ForeignLink | TargetState::RealDir | TargetState::RealFile => {
                ToolUndeployOutcome::Failed {
                    reason: "内部错误：阶段一后不应出现不可移除状态".to_string(),
                }
            }
        };
        results.push(ToolUndeployResult {
            tool_id: target.tool_id.clone(),
            path: target.path.clone(),
            outcome,
        });
    }
    Ok(UndeployReport { skill_id: req.skill_id, results })
}
