//! # skillbox-ports
//!
//! trait 抽象层：`FileSystem` / `Clock` / `Hasher` / `Linker` / `GitClient`
//! （TASKBOOK §2.1）。这些端口的唯一目的：让 engine 可以在 tmpfs + 假时钟下
//! 被完整单测，不依赖真实磁盘与系统时间。
//!
//! 生产实现（真实文件系统 / 系统时钟 / SHA-256 / 平台链接 / git 子进程）见
//! [`prod`] 模块，engine 在运行时注入；测试注入假实现。
//!
//! 【硬约束】本 crate 禁止引用 tauri 任何命名空间。

pub mod prod;

pub use prod::{ProdClock, ProdFileSystem, ProdGitClient, ProdHasher, ProdLinker};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppResult, ScanFingerprint};

// ---------------------------------------------------------------------------
// 共享数据结构
// ---------------------------------------------------------------------------

/// 目录条目的类型（lstat 语义，不跟随符号链接）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

/// 单个目录条目的元数据快照（§7 约束 1：扫描只看元数据，不读内容）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryMeta {
    /// 条目名（basename）。
    pub name: String,
    pub kind: EntryKind,
    /// 字节大小（目录为底层文件系统报告值）。
    pub len: u64,
    /// mtime（epoch 毫秒）。
    pub modified_ms: i64,
}

/// 文件哈希结果。超限文件用 mtime+size 近似并标记（§7 约束 6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHash {
    /// SHA-256 十六进制（精确内容哈希或近似指纹）。
    pub hex: String,
    /// true = 超过上限，用 (mtime, size) 近似，非内容哈希。
    pub approximated: bool,
}

/// 链接种类（`TargetState` 分类与部署回退的唯一依据，§5.1 / §5.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    Symlink,
    Junction,
    NotALink,
}

// ---------------------------------------------------------------------------
// FileSystem
// ---------------------------------------------------------------------------

/// 文件系统端口。所有方法均为 lstat 语义（不跟随符号链接），
/// 与 §7 扫描约束（不读内容、不跟随链接）对齐。
pub trait FileSystem: Send + Sync {
    /// 路径是否存在（跟随符号链接的常规语义，仅作粗判）。
    fn exists(&self, path: &Path) -> bool;

    /// lstat：获取单个路径的元数据，不跟随最后一跳的符号链接。
    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta>;

    /// 列出目录直接子项，每个子项附带 lstat 元数据。
    /// 这是最底层的目录枚举原语，扫描引擎不得绕过它直接碰 std::fs。
    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>>;

    /// 只读文件前 `max_bytes` 字节（`SKILL.md` 截断读取前 8KB，§7 约束 1）。
    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>>;

    fn read_to_string(&self, path: &Path) -> AppResult<String>;

    fn write(&self, path: &Path, contents: &[u8]) -> AppResult<()>;

    fn create_dir_all(&self, path: &Path) -> AppResult<()>;

    /// 同卷原子改名（部署写入纪律：先写临时目录再 rename，§5.3）。
    fn rename(&self, from: &Path, to: &Path) -> AppResult<()>;

    fn copy_file(&self, from: &Path, to: &Path) -> AppResult<u64>;

    /// 递归复制目录（tracked-copy 部署与 trash 暂存需要）。
    fn copy_dir_all(&self, from: &Path, to: &Path) -> AppResult<()>;

    /// 删除空目录。
    fn remove_dir(&self, path: &Path) -> AppResult<()>;

    /// 递归删除目录。仅允许 trash 流程内部使用；engine 不得对
    /// agent 目标目录直接调用（v1 删除一律先进 trash，§5.3）。
    fn remove_dir_all(&self, path: &Path) -> AppResult<()>;

    fn remove_file(&self, path: &Path) -> AppResult<()>;

    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf>;

    /// 路径是否位于本机固定卷（§7 约束 5：跳过网络卷 / 可移动卷 /
    /// 系统伪文件系统）。仅在扫描根（depth == 0）调用——逐目录 syscall
    /// 代价过高，且根之下的遍历不跨卷（符号链接不会入队）。
    fn is_local_volume(&self, path: &Path) -> bool;
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// 时钟端口。假实现返回固定时间，使扫描缓存 / 审计日志可确定性测试。
pub trait Clock: Send + Sync {
    /// 当前时间（epoch 毫秒）。
    fn now_ms(&self) -> i64;
}

// ---------------------------------------------------------------------------
// Hasher
// ---------------------------------------------------------------------------

/// 哈希端口。假实现返回恒定值，用于核对调用次数与截断行为。
pub trait Hasher: Send + Sync {
    /// 对内存字节计算 SHA-256 十六进制。
    fn hash_bytes(&self, data: &[u8]) -> String;

    /// 对文件计算哈希：`len(path) <= max_bytes` 时精确计算内容哈希；
    /// 超限时用 `(mtime, size)` 近似并置 `approximated = true`（§7 约束 6）。
    fn hash_file(&self, path: &Path, max_bytes: u64) -> AppResult<FileHash>;
}

// ---------------------------------------------------------------------------
// Linker
// ---------------------------------------------------------------------------

/// 链接端口：部署（symlink → junction → copy 回退链）与 TargetState 分类
/// 都经由它进行，从而可在临时目录下单测 Windows 特有行为。
pub trait Linker: Send + Sync {
    /// 当前环境能否创建目录符号链接（Windows 需开发者模式/管理员）。
    fn supports_symlink(&self) -> bool;

    /// 当前环境能否创建 junction（仅 Windows）。
    fn supports_junction(&self) -> bool;

    /// 创建目录符号链接：`link` → `target`。
    fn symlink_dir(&self, target: &Path, link: &Path) -> AppResult<()>;

    /// 创建 junction：`link` → `target`（仅 Windows）。
    fn junction(&self, target: &Path, link: &Path) -> AppResult<()>;

    /// 读取链接目标。
    fn read_link(&self, link: &Path) -> AppResult<PathBuf>;

    /// 删除链接本身（不动目标）。要求当前路径确为链接；
    /// 对真实目录调用是契约违规。
    fn remove_link(&self, link: &Path) -> AppResult<()>;

    /// 判定路径是符号链接 / junction / 非链接；不存在的路径返回 `NotALink`
    ///（`Absent` 判定由调用方用 `exists` 区分）。
    /// `TargetState` 分类的唯一依据（§5.1 硬约束：基于 symlink_metadata）。
    fn link_kind(&self, path: &Path) -> AppResult<LinkKind>;
}

// ---------------------------------------------------------------------------
// GitClient
// ---------------------------------------------------------------------------

/// git 端口：install(git) 与 updates(check/apply) 的全部远端交互。
/// 假实现不联网，使来源解析逻辑可离线单测（v1 除用户主动触发的 git
/// 更新外禁止联网，§0.3）。
pub trait GitClient: Send + Sync {
    /// git 二进制是否可用。
    fn is_available(&self) -> bool;

    /// 克隆仓库到 `dest`；`depth` 为浅克隆深度（无历史场景）。
    fn clone(&self, url: &str, dest: &Path, depth: Option<u32>) -> AppResult<()>;

    /// fetch 远端（不合并）。
    fn fetch(&self, repo: &Path) -> AppResult<()>;

    /// 当前 HEAD 的 revision。
    fn head_revision(&self, repo: &Path) -> AppResult<String>;

    /// 两个 revision 之间的 diff 摘要（更新前预览，§9.4）。
    fn diff_summary(&self, repo: &Path, from: &str, to: &str) -> AppResult<String>;

    /// 检出指定 revision。
    fn checkout(&self, repo: &Path, revision: &str) -> AppResult<()>;
}

// ---------------------------------------------------------------------------
// ScanCache（§7 约束 6）
// ---------------------------------------------------------------------------

/// `scan_cache` 单条缓存：目录指纹 + 上次解析出的 name / description。
/// 指纹 `(mtime, size, childCount)` 未变时直接复用，免重读 `SKILL.md`。
/// name / description 属于摘要字段，不违反"DB 禁存正文 / frontmatter 全文"
/// 约束（§3.1 允许存 name / description 摘要）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedScanEntry {
    /// 目录绝对路径（缓存键）。
    pub path: PathBuf,
    /// 目录指纹 `(mtime, size, childCount)`。
    pub fingerprint: ScanFingerprint,
    /// 上次解析出的 frontmatter `name`。
    pub name: Option<String>,
    /// 上次解析出的 frontmatter `description`。
    pub description: Option<String>,
    /// 上次写入时间（epoch 毫秒），为后续淘汰策略预留。
    pub last_seen_ms: i64,
}

/// 扫描缓存端口（`scan_cache` 表 / 内存实现）。
///
/// 实现只需 `Send`：engine 侧以 `Arc<Mutex<dyn ScanCache>>` 共享给工作
/// 线程（rusqlite `Connection` 非 `Sync`，必须串行化访问）。
pub trait ScanCache: Send {
    /// 取缓存条目；未命中返回 `None`。
    fn get(&self, path: &Path) -> AppResult<Option<CachedScanEntry>>;
    /// 写入（upsert）缓存条目。
    fn put(&self, entry: &CachedScanEntry) -> AppResult<()>;
}

/// 内存扫描缓存：测试注入与"无 DB"场景。
#[derive(Debug, Default)]
pub struct MemoryScanCache {
    inner: Mutex<HashMap<PathBuf, CachedScanEntry>>,
}

impl MemoryScanCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ScanCache for MemoryScanCache {
    fn get(&self, path: &Path) -> AppResult<Option<CachedScanEntry>> {
        Ok(self.inner.lock().expect("MemoryScanCache poisoned").get(path).cloned())
    }

    fn put(&self, entry: &CachedScanEntry) -> AppResult<()> {
        self.inner
            .lock()
            .expect("MemoryScanCache poisoned")
            .insert(entry.path.clone(), entry.clone());
        Ok(())
    }
}
