//! # scan —— 有界增量扫描引擎（TASKBOOK §7，11 条硬约束）
//!
//! 【硬约束清单】§7 每条约束的实现位置（行号为定稿实测）：
//!
//! 1. **不读文件内容**：候选判定只依赖 `FileSystem::read_dir`（lstat 元数据，
//!    `process_dir` L211）；`SKILL.md` 只在目录确认为 skill 后截断读前 8 KB
//!    （`read_frontmatter` L345，上限常量 L44）。
//! 2. **深度 ≤ 6**：`scan` L450 钳制 `max_depth`；`process_dir` L239 的
//!    `TooDeep` 分支拦截超深子目录。
//! 3. **黑名单（内置 + 用户追加）**：[`BUILTIN_BLACKLIST`]（L57）与
//!    [`is_blacklisted`]（L63）。
//! 4. **不跟随符号链接**：`process_dir` L233 按 lstat 的 `EntryKind::Symlink`
//!    记录跳过，符号链接目录绝不入队。
//! 5. **跳过网络 / 可移动卷**：`process_dir` L206 对 depth == 0 根调用
//!    `FileSystem::is_local_volume`。
//! 6. **增量指纹复用**：`try_cache_hit` L302 / `put_cache_entry` L324，
//!    指纹由 `fingerprint_of` L366 计算。
//! 7. **并发 min(num_cpus, 4)，禁止无界 spawn**：`scan` L480 用
//!    `std::thread::scope` + 固定 worker 数（`worker_count` L418）。
//! 8. **每目录检查 cancel，200ms 内返回部分结果**：`worker_loop` L161 的
//!    `Condvar::wait_timeout(50ms)`（L179）+ 每任务入口检查（L163/L199）。
//! 9. **每 50 目录流式进度**：`note_scanned` L125。
//! 10. **峰值 RSS 增量 ≤ 80MB、分批产出**：结果经 `Mutex<Results>` 增量追加
//!     （`Results` L101），从不预建整棵树；实测数字见
//!     `docs/PERFORMANCE.md`（scan_bench 采样）。
//! 11. **无网络 / git**：本文件零网络调用，全部 IO 经 `FileSystem` /
//!     `ScanCache` 端口（FakeFs 注入可证）。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use skillbox_domain::{AppResult, DiscoveredSkill, ScanFingerprint, ToolId};
use skillbox_ports::{CachedScanEntry, Clock, EntryKind, EntryMeta, FileSystem, ScanCache};

use crate::types::{ScanProgress, ScanReport, ScanRequest, SkipReason, SkippedDir};
use crate::types::CancellationToken;

/// skill 目录的标记文件名（大小写敏感匹配）。
pub(crate) const SKILL_FILE: &str = "SKILL.md";
/// frontmatter 截断读取上限：8 KB（§7 约束 1）。
pub(crate) const SKILL_HEAD_LIMIT: u64 = 8 * 1024;
/// 深度硬上限：请求 > 6 一律按 6 执行（§7 约束 2）。
const MAX_DEPTH_CAP: u8 = 6;
/// 进度节流：每处理 50 个目录 emit 一次（§7 约束 9）。
const PROGRESS_EVERY: u32 = 50;
/// 工作线程数上限（§7 约束 7：min(num_cpus, 4)）。
const MAX_WORKERS: usize = 4;
/// 空闲 worker 唤醒间隔：取消信号 ≤ 50ms 被感知，加上当前目录收尾，
/// 满足"取消后 200ms 内返回"（§7 约束 8）。
const IDLE_POLL_MS: u64 = 50;

/// 内置黑名单（§7 约束 3）。用户经 `ScanRequest::blacklist` 只能追加，
/// 不能移除内置项。
const BUILTIN_BLACKLIST: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", "venv", ".venv",
    "__pycache__", ".cache", ".next", "Library", "AppData",
];

/// 黑名单匹配：Windows 大小写不敏感（与文件系统语义一致），Unix 精确匹配。
fn is_blacklisted(name: &str, extra: &[String]) -> bool {
    let hit = |candidate: &str| {
        if cfg!(windows) {
            name.eq_ignore_ascii_case(candidate)
        } else {
            name == candidate
        }
    };
    BUILTIN_BLACKLIST.iter().any(|b| hit(b)) || extra.iter().any(|b| hit(b))
}

// ---------------------------------------------------------------------------
// 工作队列与上下文
// ---------------------------------------------------------------------------

/// 单个待处理目录。
struct ScanJob {
    path: PathBuf,
    depth: u8,
    tool_id: Option<ToolId>,
    read_only: bool,
}

/// 工作队列共享状态（`Pool::state` 内）。
#[derive(Default)]
struct PoolState {
    jobs: VecDeque<ScanJob>,
    active: usize,
}

/// 手工工作池：固定 worker 数 + 共享队列 + Condvar（§7 约束 7/8）。
struct Pool {
    state: Mutex<PoolState>,
    idle: Condvar,
}

/// 一次扫描的全部共享可变状态（线程安全）。
#[derive(Default)]
struct Results {
    skills: Vec<DiscoveredSkill>,
    skipped: Vec<SkippedDir>,
    warnings: Vec<String>,
}

/// 一次扫描的共享上下文。
struct ScanCtx<'a> {
    fs: &'a Arc<dyn FileSystem>,
    clock: &'a Arc<dyn Clock>,
    cache: Option<&'a Arc<Mutex<dyn ScanCache>>>,
    cancel: &'a CancellationToken,
    on_progress: &'a (dyn Fn(ScanProgress) + Sync),
    blacklist: Vec<String>,
    max_depth: u8,
    incremental: bool,
    pool: Pool,
    results: Mutex<Results>,
    scanned: AtomicU32,
}

impl<'a> ScanCtx<'a> {
    /// §7 约束 9：目录计数，每 50 个 emit 一次进度。
    /// `scanned` 单调递增，多线程交错下恰好每个 50 倍数 emit 一次。
    fn note_scanned(&self, current: &Path) {
        let n = self.scanned.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(PROGRESS_EVERY) {
            (self.on_progress)(ScanProgress {
                scanned: n,
                total_hint: None,
                current: current.to_path_buf(),
            });
        }
    }

    fn push_job(&self, job: ScanJob) {
        let mut st = self.pool.state.lock().expect("scan pool poisoned");
        st.jobs.push_back(job);
        drop(st);
        self.pool.idle.notify_all();
    }

    fn record_skip(&self, path: PathBuf, reason: SkipReason) {
        self.results
            .lock()
            .expect("scan results poisoned")
            .skipped
            .push(SkippedDir { path, reason });
    }

    fn record_warning(&self, message: String) {
        self.results
            .lock()
            .expect("scan results poisoned")
            .warnings
            .push(message);
    }

    /// §7 约束 8：worker 主循环——取任务 → 处理 → 队列空且无活跃时退出；
    /// 空闲等待带 50ms 超时，取消信号必然被感知。
    fn worker_loop(&self) {
        loop {
            if self.cancel.is_cancelled() {
                return;
            }
            let job = {
                let mut st = self.pool.state.lock().expect("scan pool poisoned");
                loop {
                    if let Some(job) = st.jobs.pop_front() {
                        st.active += 1;
                        break Some(job);
                    }
                    if st.active == 0 {
                        break None; // 队列空且无活跃任务：全部完成
                    }
                    let (guard, _) = self
                        .pool
                        .idle
                        .wait_timeout(st, Duration::from_millis(IDLE_POLL_MS))
                        .expect("scan pool poisoned");
                    st = guard;
                }
            };
            match job {
                Some(job) => {
                    self.process_dir(job);
                    let mut st = self.pool.state.lock().expect("scan pool poisoned");
                    st.active -= 1;
                    drop(st);
                    self.pool.idle.notify_all();
                }
                None => return,
            }
        }
    }

    /// 处理单个目录：lstat 元数据判定 → skill 确认与登记 → 子目录入队。
    fn process_dir(&self, job: ScanJob) {
        if self.cancel.is_cancelled() {
            return;
        }
        self.note_scanned(&job.path);

        // §7 约束 5：仅对扫描根做卷检查（逐目录 syscall 代价过高；根之下
        // 的遍历不跨卷——符号链接不会入队）。
        if job.depth == 0 && !self.fs.is_local_volume(&job.path) {
            self.record_skip(job.path, SkipReason::NonLocalVolume);
            return;
        }

        let entries = match self.fs.read_dir(&job.path) {
            Ok(entries) => entries,
            Err(err) => {
                self.record_warning(format!("无法读取目录 {}: {err}", job.path.display()));
                return;
            }
        };

        // §7 约束 1：判定只看元数据；确认后才允许截断读 SKILL.md。
        if entries
            .iter()
            .any(|e| e.name == SKILL_FILE && e.kind == EntryKind::File)
        {
            self.record_skill(&job, &entries);
        }

        // 统一递归规则（无 skill 目录特例）：黑名单 / 符号链接 / 超深
        // 子目录跳过，其余入队。
        for entry in &entries {
            let child = job.path.join(&entry.name);
            match entry.kind {
                // §7 约束 4：lstat 识别的符号链接绝不入队（防循环 / 越权）。
                EntryKind::Symlink => {
                    self.record_skip(child, SkipReason::Symlink);
                }
                EntryKind::Dir => {
                    if is_blacklisted(&entry.name, &self.blacklist) {
                        self.record_skip(child, SkipReason::Blacklisted);
                    } else if job.depth >= self.max_depth {
                        // 子目录深度 = depth + 1 > max_depth（§7 约束 2）。
                        self.record_skip(child, SkipReason::TooDeep);
                    } else {
                        self.push_job(ScanJob {
                            path: child,
                            depth: job.depth + 1,
                            tool_id: job.tool_id.clone(),
                            read_only: job.read_only,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// 确认后的 skill 目录：算指纹 → 增量复用（§7 约束 6）或截断读
    /// frontmatter → 登记 `DiscoveredSkill`。
    fn record_skill(&self, job: &ScanJob, entries: &[EntryMeta]) {
        let fingerprint = fingerprint_of(self.fs.as_ref(), &job.path, entries);
        let dir_name = job
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| job.path.to_string_lossy().into_owned());

        let parsed: Option<(Option<String>, Option<String>)> = if self.incremental {
            self.cache.and_then(|cache| self.try_cache_hit(cache, &job.path, fingerprint))
        } else {
            None
        };
        let (name, description) = match parsed {
            Some(hit) => hit,
            None => {
                let parsed = self.read_frontmatter(&job.path);
                if self.incremental {
                    if let Some(cache) = self.cache {
                        self.put_cache_entry(cache, &job.path, fingerprint, &parsed);
                    }
                }
                parsed
            }
        };

        let skill = DiscoveredSkill {
            tool_id: job.tool_id.clone(),
            path: job.path.clone(),
            dir_name,
            name,
            description,
            fingerprint,
            read_only: job.read_only,
        };
        self.results
            .lock()
            .expect("scan results poisoned")
            .skills
            .push(skill);
    }

    /// §7 约束 6 前半：指纹一致时复用缓存的 name / description，
    /// 免重读 `SKILL.md`（缓存命中路径，读计数由测试观测）。
    fn try_cache_hit(
        &self,
        cache: &Arc<Mutex<dyn ScanCache>>,
        path: &Path,
        fingerprint: ScanFingerprint,
    ) -> Option<(Option<String>, Option<String>)> {
        let guard = cache.lock().expect("scan cache poisoned");
        match guard.get(path) {
            Ok(Some(entry)) if entry.fingerprint == fingerprint => {
                Some((entry.name, entry.description))
            }
            Ok(_) => None,
            Err(err) => {
                // 缓存读失败不致命：降级为全量读取。
                drop(guard);
                self.record_warning(format!("scan_cache 读取失败（降级全量）: {err}"));
                None
            }
        }
    }

    /// §7 约束 6 后半：写入（upsert）缓存条目。
    fn put_cache_entry(
        &self,
        cache: &Arc<Mutex<dyn ScanCache>>,
        path: &Path,
        fingerprint: ScanFingerprint,
        parsed: &(Option<String>, Option<String>),
    ) {
        let entry = CachedScanEntry {
            path: path.to_path_buf(),
            fingerprint,
            name: parsed.0.clone(),
            description: parsed.1.clone(),
            last_seen_ms: self.clock.now_ms(),
        };
        if let Err(err) = cache.lock().expect("scan cache poisoned").put(&entry) {
            self.record_warning(format!("scan_cache 写入失败（忽略）: {err}"));
        }
    }

    /// §7 约束 1 后半：仅在确认为 skill 目录后读取 `SKILL.md`，
    /// 且截断读取前 8 KB。
    fn read_frontmatter(&self, dir: &Path) -> (Option<String>, Option<String>) {
        match self.fs.read_head(&dir.join(SKILL_FILE), SKILL_HEAD_LIMIT) {
            Ok(bytes) => parse_frontmatter_head(&bytes),
            Err(err) => {
                // 目录结构已确认是 skill；读失败降级为无元数据，不中断扫描。
                self.record_warning(format!(
                    "无法读取 {}（按无 frontmatter 处理）: {err}",
                    dir.join(SKILL_FILE).display()
                ));
                (None, None)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 纯函数
// ---------------------------------------------------------------------------

/// 目录指纹 `(mtime, size, 子项数)`（§7 约束 6）：mtime 取目录自身 lstat，
/// size 为直接子项 len 累计，child_count 为直接子项数。
fn fingerprint_of(fs: &dyn FileSystem, dir: &Path, entries: &[EntryMeta]) -> ScanFingerprint {
    let mtime_ms = fs.symlink_metadata(dir).map(|m| m.modified_ms).unwrap_or(0);
    let size = entries.iter().map(|e| e.len).sum();
    ScanFingerprint { mtime_ms, size, child_count: entries.len() as u32 }
}

/// 解析 `SKILL.md` 头部 frontmatter 的 `name` / `description`。
///
/// 输入为截断后的字节（≤ 8 KB，可能截在多字节字符中间——lossy 转换）。
/// 仅接受 `---` 包裹的标准块；缺失 / 畸形按无元数据处理。
/// 8 KB 处被截断（未闭合）时仍提取已见字段（frontmatter 不会超过 8 KB）。
pub(crate) fn parse_frontmatter_head(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return (None, None);
    }
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    for line in lines {
        let line = line.trim_end();
        if line == "---" {
            break; // frontmatter 块闭合
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if key == "name" && name.is_none() {
            name = Some(value);
        } else if key == "description" && description.is_none() {
            description = Some(value);
        }
    }
    (name, description)
}

/// 去掉成对的单 / 双引号包裹。
fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// §7 约束 7：min(num_cpus, 4)，至少 1。
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, MAX_WORKERS)
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 扫描入口：§7 全部 11 条约束的编排（逐条位置见本文件顶部清单）。
///
/// - 取消后返回**已扫到的部分结果**（`Ok(ScanReport)`，约束 8）；
///   取消不是错误。
/// - roots 由调用方组装（未来步骤：adapters 解析 + 中央库根）；根不受
///   黑名单 / 深度启发式约束。
/// - 不存在的根产生一条 warning（调用方应只传入已探测存在的根）。
pub fn scan(
    req: ScanRequest,
    cancel: CancellationToken,
    on_progress: impl Fn(ScanProgress) + Sync,
) -> AppResult<ScanReport> {
    let start_ms = req.clock.now_ms();
    if req.roots.is_empty() {
        return Ok(ScanReport {
            duration_ms: req.clock.now_ms() - start_ms,
            ..ScanReport::default()
        });
    }

    // §7 约束 2：深度硬上限 6，请求超出按 6 执行。
    let max_depth = req.max_depth.min(MAX_DEPTH_CAP);
    let ctx = ScanCtx {
        fs: &req.fs,
        clock: &req.clock,
        cache: req.cache.as_ref(),
        cancel: &cancel,
        on_progress: &on_progress,
        blacklist: req.blacklist.clone(),
        max_depth,
        incremental: req.incremental,
        pool: Pool { state: Mutex::new(PoolState::default()), idle: Condvar::new() },
        results: Mutex::new(Results::default()),
        scanned: AtomicU32::new(0),
    };

    // 播种根任务（depth 0，不受黑名单 / 深度约束）。
    {
        let mut st = ctx.pool.state.lock().expect("scan pool poisoned");
        for root in &req.roots {
            st.jobs.push_back(ScanJob {
                path: root.path.clone(),
                depth: 0,
                tool_id: root.tool_id.clone(),
                read_only: root.read_only,
            });
        }
    }

    // §7 约束 7：固定数量 scoped 线程，禁止无界 spawn。
    let n_workers = worker_count();
    std::thread::scope(|scope| {
        for _ in 0..n_workers {
            scope.spawn(|| ctx.worker_loop());
        }
    });

    let mut results = ctx.results.into_inner().expect("scan results poisoned");
    // 确定性输出：并发遍历的完成顺序不稳定，报告按路径 / 字典序排序，
    // 保证同一磁盘状态产生同一份报告（增量 UI diff 与测试断言的前提）。
    results.skills.sort_by(|a, b| a.path.cmp(&b.path));
    results.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    results.warnings.sort();
    Ok(ScanReport {
        skills: results.skills,
        skipped: results.skipped,
        warnings: results.warnings,
        duration_ms: req.clock.now_ms() - start_ms,
    })
}

#[cfg(test)]
mod tests;
