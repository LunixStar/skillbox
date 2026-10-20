//! doctor —— 只读体检（TASKBOOK §6.3）。
//!
//! 【契约】
//! - **绝不写盘**：全部端口调用限于读取（read_dir / symlink_metadata /
//!   read_link / canonicalize / exists / hash_file）；由
//!   `doctor_is_read_only_zero_disk_writes` 单测钉死（CountingFs 写计数
//!   必须为 0）。任何时候可安全运行。
//! - 问题状态由磁盘 reconcile 推算（K4），DB 索引只作"上次意图"比对。
//! - `tools` 限定检查范围：未列出的工具不检查、其记录过滤掉，避免误报
//!   MissingTarget；`min_severity` 过滤严重度。
//!
//! 磁盘观察（`inspect_disk`）与内容哈希（`hash_dir`）为 `pub(crate)`：
//! fix 执行前重新推算差异（不信任 doctor 快照）时复用。

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillbox_domain::{AppResult, ScanFingerprint, SkillId, TargetState, ToolId};
use skillbox_ports::{Clock, EntryKind, EntryMeta, FileSystem, Hasher, Linker};

use crate::deploy::classify::SOURCE_MARKER_FILE;
use crate::reconcile::reconcile;
use crate::types::{DiskEntry, DiskView, DoctorIssue, DoctorReport, IndexView, Severity};

/// 单个哈希文件的读取上限：1 MB（§7 约束 6，超限近似并标记）。
const HASH_FILE_LIMIT: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// 请求类型
// ---------------------------------------------------------------------------

/// 单个体检目标：某工具的 skills 根目录。
#[derive(Debug, Clone)]
pub struct DoctorTarget {
    pub tool_id: ToolId,
    /// 工具 skills 根目录（agent 目录，非单个部署路径）。
    pub path: PathBuf,
}

/// 体检上下文：磁盘观察与内容哈希所需的全部注入。
///
/// 【注意】本类型不可序列化：端口句柄随请求传递（同 `ScanRequest` 约定），
/// IPC 层使用独立 DTO 组装。
pub struct HealthContext {
    /// 本次检查的工具 skills 根。
    pub targets: Vec<DoctorTarget>,
    /// 中央库 skills 根（`~/.skillbox/skills`）。
    pub central_root: PathBuf,
    /// DB 索引快照（只读比对，不写）。
    pub index: IndexView,
    pub fs: Arc<dyn FileSystem>,
    pub clock: Arc<dyn Clock>,
    pub linker: Arc<dyn Linker>,
    pub hasher: Arc<dyn Hasher>,
}

impl fmt::Debug for HealthContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 端口句柄无有意义的 Debug，只呈现行为相关字段。
        f.debug_struct("HealthContext")
            .field("targets", &self.targets)
            .field("central_root", &self.central_root)
            .field("index.skills", &self.index.skills.len())
            .field("index.records", &self.index.records.len())
            .field("fs", &"<dyn FileSystem>")
            .field("clock", &"<dyn Clock>")
            .field("linker", &"<dyn Linker>")
            .field("hasher", &"<dyn Hasher>")
            .finish()
    }
}

/// 体检选项。
pub struct DoctorOpts {
    pub ctx: HealthContext,
    /// 限定工具范围；None = 检查 `ctx.targets` 全部。
    pub tools: Option<Vec<ToolId>>,
    /// 只保留该严重度及以上的问题；None = 全部。
    pub min_severity: Option<Severity>,
}

impl fmt::Debug for DoctorOpts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DoctorOpts")
            .field("ctx", &self.ctx)
            .field("tools", &self.tools)
            .field("min_severity", &self.min_severity)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 体检：inspect → reconcile → 过滤，只读，任何时候可安全运行。
pub fn doctor(opts: DoctorOpts) -> AppResult<DoctorReport> {
    let start_ms = opts.ctx.clock.now_ms();
    let tools = opts.tools.as_deref();
    let disk = inspect_disk(&opts.ctx, tools)?;
    let index = filter_index(&opts.ctx.index, tools);
    let issues: Vec<DoctorIssue> = reconcile(&disk, &index)
        .into_iter()
        .filter(|d| opts.min_severity.is_none_or(|m| d.severity >= m))
        .map(|d| {
            let id = d.issue_id();
            DoctorIssue { id, severity: d.severity, discrepancy: d }
        })
        .collect();
    // checked_tools = 实际检查的工具（`tools` 过滤后的目标列表，排序去重）。
    let mut checked_tools: Vec<ToolId> = opts
        .ctx
        .targets
        .iter()
        .filter(|t| tools.is_none_or(|list| list.contains(&t.tool_id)))
        .map(|t| t.tool_id.clone())
        .collect();
    checked_tools.sort_by(|a, b| a.0.cmp(&b.0));
    checked_tools.dedup();
    Ok(DoctorReport {
        issues,
        checked_tools,
        duration_ms: opts.ctx.clock.now_ms() - start_ms,
    })
}

/// 按 `tools` 过滤索引记录（skills 永远保留——跨工具比对需要正本表）。
fn filter_index(index: &IndexView, tools: Option<&[ToolId]>) -> IndexView {
    let Some(tools) = tools else { return index.clone() };
    IndexView {
        skills: index.skills.clone(),
        records: index.records.iter().filter(|r| tools.contains(&r.tool_id)).cloned().collect(),
    }
}

// ---------------------------------------------------------------------------
// 磁盘观察
// ---------------------------------------------------------------------------

/// 遍历各工具 skills 根，产出磁盘现状快照（只读）。
///
/// 不存在的工具根直接跳过（该工具的记录由 reconcile 报 MissingTarget）；
/// `tools` 为 Some 时只检查列出的工具。
pub(crate) fn inspect_disk(ctx: &HealthContext, tools: Option<&[ToolId]>) -> AppResult<DiskView> {
    // 正本路径 → skill id（canonicalize 统一比较基准；正本缺失时退回原路径）。
    let skill_by_canonical: HashMap<PathBuf, SkillId> = ctx
        .index
        .skills
        .iter()
        .map(|s| {
            let canon = ctx.fs.canonicalize(&s.canonical_path).unwrap_or_else(|_| s.canonical_path.clone());
            (canon, s.id)
        })
        .collect();
    let central_canon = ctx
        .fs
        .canonicalize(&ctx.central_root)
        .unwrap_or_else(|_| ctx.central_root.clone());

    let mut entries = Vec::new();
    for target in &ctx.targets {
        if let Some(tools) = tools {
            if !tools.contains(&target.tool_id) {
                continue;
            }
        }
        if !ctx.fs.exists(&target.path) {
            continue;
        }
        for meta in ctx.fs.read_dir(&target.path)? {
            let path = target.path.join(&meta.name);
            entries.push(inspect_entry(
                ctx,
                &target.tool_id,
                &path,
                &meta,
                &skill_by_canonical,
                &central_canon,
            )?);
        }
    }
    Ok(DiskView { entries })
}

/// 单条目的磁盘观察（lstat 语义）：
/// - 真实目录：指纹 + 递归内容哈希；
/// - 链接：原始目标 + 悬空标记；非悬空时按解析目标归类（正本 →
///   `LinkToSource` + skill id；中央库内但非正本 → `LinkToSource` 无 id；
///   中央库外 → `ForeignLink`；悬空一律 `ForeignLink` + `link_broken`，
///   由 reconcile 走 BrokenLink 判定）；
/// - 文件 / 其他：`RealFile`。
fn inspect_entry(
    ctx: &HealthContext,
    tool_id: &ToolId,
    path: &Path,
    meta: &EntryMeta,
    skill_by_canonical: &HashMap<PathBuf, SkillId>,
    central_canon: &Path,
) -> AppResult<DiskEntry> {
    let mut entry = DiskEntry {
        tool_id: tool_id.clone(),
        path: path.to_path_buf(),
        state: TargetState::Absent,
        link_target: None,
        link_broken: false,
        fingerprint: None,
        content_hash: None,
        source_skill_id: None,
    };
    match meta.kind {
        EntryKind::Dir => {
            entry.state = TargetState::RealDir;
            let children = ctx.fs.read_dir(path)?;
            entry.fingerprint = Some(ScanFingerprint {
                mtime_ms: meta.modified_ms,
                size: children.iter().map(|c| c.len).sum(),
                child_count: children.len() as u32,
            });
            entry.content_hash =
                hash_dir(ctx.fs.as_ref(), ctx.linker.as_ref(), ctx.hasher.as_ref(), path);
        }
        EntryKind::Symlink => {
            entry.link_target = ctx.linker.read_link(path).ok();
            entry.link_broken = !ctx.fs.exists(path);
            if entry.link_broken {
                entry.state = TargetState::ForeignLink;
            } else {
                match ctx.fs.canonicalize(path) {
                    Ok(resolved) => {
                        if resolved.starts_with(central_canon) {
                            entry.state = TargetState::LinkToSource;
                            entry.source_skill_id = skill_by_canonical.get(&resolved).copied();
                        } else {
                            entry.state = TargetState::ForeignLink;
                        }
                    }
                    // exists 为真但解析失败（竞争窗口）：保守按外部链接处理。
                    Err(_) => entry.state = TargetState::ForeignLink,
                }
            }
        }
        EntryKind::File | EntryKind::Other => {
            entry.state = TargetState::RealFile;
        }
    }
    Ok(entry)
}

// ---------------------------------------------------------------------------
// 递归内容哈希（doctor 观察 / fix 记录刷新共用）
// ---------------------------------------------------------------------------

/// 目录递归内容哈希：内容驱动（与 mtime 无关——`fs::copy` 不保留 mtime，
/// copy 副本与正本必须可比对）。
///
/// - 文件 → `hash_file`（1 MB 上限，超限近似，§7 约束 6），行 `rel|file|hex`；
/// - 目录 → 递归，行 `rel|dir`；
/// - 符号链接 / 其他 → 不跟随，行 `rel|<kind>|<目标长度>`；
/// - 根级 `.skillbox-source.json`（我们的来源标记，非内容）跳过。
///
/// 行排序后拼接换行、整体 `hash_bytes`——同一内容恒同一哈希。
/// 任一文件读取失败返回 None（调用方保守不比对，不误报）。
pub(crate) fn hash_dir(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    hasher: &dyn Hasher,
    dir: &Path,
) -> Option<String> {
    let mut lines = Vec::new();
    hash_dir_inner(fs, linker, hasher, dir, "", &mut lines)?;
    lines.sort_unstable();
    let joined = lines.join("\n");
    Some(hasher.hash_bytes(joined.as_bytes()))
}

fn hash_dir_inner(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    hasher: &dyn Hasher,
    dir: &Path,
    prefix: &str,
    lines: &mut Vec<String>,
) -> Option<()> {
    for meta in fs.read_dir(dir).ok()? {
        // 根级来源标记不参与内容身份（副本有、正本没有）。
        if prefix.is_empty() && meta.name == SOURCE_MARKER_FILE {
            continue;
        }
        let rel = if prefix.is_empty() {
            meta.name.clone()
        } else {
            format!("{prefix}/{}", meta.name)
        };
        let child = dir.join(&meta.name);
        match meta.kind {
            EntryKind::File => {
                let hash = hasher.hash_file(&child, HASH_FILE_LIMIT).ok()?;
                lines.push(format!("{rel}|file|{}", hash.hex));
            }
            EntryKind::Dir => {
                lines.push(format!("{rel}|dir"));
                hash_dir_inner(fs, linker, hasher, &child, &rel, lines)?;
            }
            EntryKind::Symlink | EntryKind::Other => {
                let len = linker.read_link(&child).map(|t| t.as_os_str().len()).unwrap_or(0);
                lines.push(format!("{rel}|{:?}|{len}", meta.kind));
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests;
