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
use skillbox_domain::{AppResult, OriginKind, ScanFingerprint, SkillId, SyncMode, ToolId};

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
// WatchMute（§5.3）
// ---------------------------------------------------------------------------

/// 文件监听静音端口（§5.3：所有写操作执行前 mute 文件监听，避免自触发）。
///
/// v1 的 watcher 尚未实现；engine 在写盘前后经由本端口成对调用，
/// 未来接入真实监听时无需改动 engine。
pub trait WatchMute: Send + Sync {
    /// 暂停文件监听（写盘前）。
    fn mute(&self);
    /// 恢复文件监听（写盘后；与 mute 严格成对）。
    fn unmute(&self);
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

// ---------------------------------------------------------------------------
// IndexStore（§3.2 skills / skill_targets 写入端口）
// ---------------------------------------------------------------------------

/// skill 的入库来源信息（`skills` 表 origin_* 列的写入面）。
///
/// 入库（install）与收编（adopt）共用：收编固定 [`OriginInfo::local`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginInfo {
    pub kind: OriginKind,
    /// 仓库 URL / 本地路径 / 压缩包路径。
    pub ref_path: Option<String>,
    /// 仓库内子路径（monorepo 场景）。
    pub subpath: Option<String>,
    /// 来源 revision（git）。
    pub revision: Option<String>,
}

impl OriginInfo {
    /// 本地来源（收编 / 本地目录入库）。
    pub fn local() -> Self {
        OriginInfo { kind: OriginKind::Local, ref_path: None, subpath: None, revision: None }
    }

    /// git 来源。
    pub fn git(url: &str, subpath: Option<String>, revision: Option<String>) -> Self {
        OriginInfo {
            kind: OriginKind::Git,
            ref_path: Some(url.to_string()),
            subpath,
            revision,
        }
    }
}

/// 中央库索引写入端口（`skills` / `skill_targets` 两表的写入面）。
///
/// 读侧不设端口：调用方（ipc）直接查询 SQLite 组装 [`IndexView`] 快照
/// 传入 engine；写入经由本端口，使 adopt / 补登记等流程可离线单测。
/// SQLite 实现在 store crate（Phase 2 接线），测试用 [`MemoryIndexStore`]。
pub trait IndexStore: Send + Sync {
    /// 新增 skill 索引行，返回分配的 id。
    /// `canonical_path` 冲突（UNIQUE）时返回 `Database` 错误。
    fn insert_skill(
        &self,
        name: &str,
        description: &str,
        origin: &OriginInfo,
        canonical_path: &Path,
        content_hash: Option<&str>,
    ) -> AppResult<SkillId>;

    /// upsert `skill_targets`（UNIQUE(skill_id, tool_id)）：存在则整行
    /// 刷新 path / mode / source_hash，不存在则插入。
    fn upsert_target(
        &self,
        skill_id: SkillId,
        tool_id: &ToolId,
        path: &Path,
        mode: SyncMode,
        source_hash: Option<&str>,
    ) -> AppResult<()>;

    /// 删除单条 `skill_targets` 记录（取消部署 / 移除 skill 时清理意图）。
    fn delete_target(&self, skill_id: SkillId, tool_id: &ToolId) -> AppResult<()>;

    /// 删除 skill 索引行及其全部 `skill_targets` 记录与标签
    /// （删除 Skill：正本已移入 trash 后的索引清理）。
    fn delete_skill(&self, skill_id: SkillId) -> AppResult<()>;
}

/// 内存索引行（`skills` 表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSkill {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub origin: OriginInfo,
    pub canonical_path: PathBuf,
    pub content_hash: Option<String>,
}

/// 内存索引行（`skill_targets` 表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTarget {
    pub skill_id: SkillId,
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub mode: SyncMode,
    pub source_hash: Option<String>,
}

/// 内存索引实现：测试注入与"无 DB"场景（id 从 1 顺序分配）。
#[derive(Debug, Default)]
pub struct MemoryIndexStore {
    inner: Mutex<MemoryIndexState>,
}

#[derive(Debug, Default)]
struct MemoryIndexState {
    next_id: i64,
    skills: Vec<StoredSkill>,
    targets: Vec<StoredTarget>,
}

impl MemoryIndexStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 全部 skill 行（插入序）。
    pub fn skills(&self) -> Vec<StoredSkill> {
        self.inner.lock().expect("MemoryIndexStore poisoned").skills.clone()
    }

    /// 全部 target 行（插入序）。
    pub fn targets(&self) -> Vec<StoredTarget> {
        self.inner.lock().expect("MemoryIndexStore poisoned").targets.clone()
    }
}

impl IndexStore for MemoryIndexStore {
    fn insert_skill(
        &self,
        name: &str,
        description: &str,
        origin: &OriginInfo,
        canonical_path: &Path,
        content_hash: Option<&str>,
    ) -> AppResult<SkillId> {
        let mut st = self.inner.lock().expect("MemoryIndexStore poisoned");
        if st.skills.iter().any(|s| s.canonical_path == canonical_path) {
            return Err(skillbox_domain::AppError::database(format!(
                "canonical_path 已存在: {}",
                canonical_path.display()
            )));
        }
        st.next_id += 1;
        let id = SkillId(st.next_id);
        st.skills.push(StoredSkill {
            id,
            name: name.to_string(),
            description: description.to_string(),
            origin: origin.clone(),
            canonical_path: canonical_path.to_path_buf(),
            content_hash: content_hash.map(|h| h.to_string()),
        });
        Ok(id)
    }

    fn upsert_target(
        &self,
        skill_id: SkillId,
        tool_id: &ToolId,
        path: &Path,
        mode: SyncMode,
        source_hash: Option<&str>,
    ) -> AppResult<()> {
        let mut st = self.inner.lock().expect("MemoryIndexStore poisoned");
        let row = StoredTarget {
            skill_id,
            tool_id: tool_id.clone(),
            path: path.to_path_buf(),
            mode,
            source_hash: source_hash.map(|h| h.to_string()),
        };
        match st.targets.iter_mut().find(|t| t.skill_id == skill_id && t.tool_id == *tool_id) {
            Some(existing) => *existing = row,
            None => st.targets.push(row),
        }
        Ok(())
    }

    fn delete_target(&self, skill_id: SkillId, tool_id: &ToolId) -> AppResult<()> {
        let mut st = self.inner.lock().expect("MemoryIndexStore poisoned");
        st.targets.retain(|t| !(t.skill_id == skill_id && t.tool_id == *tool_id));
        Ok(())
    }

    fn delete_skill(&self, skill_id: SkillId) -> AppResult<()> {
        let mut st = self.inner.lock().expect("MemoryIndexStore poisoned");
        st.skills.retain(|s| s.id != skill_id);
        st.targets.retain(|t| t.skill_id != skill_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillbox_domain::OriginKind;

    #[test]
    fn origin_info_serializes_camel_case() {
        let origin = OriginInfo::git("https://x", Some("skills/a".into()), Some("abc".into()));
        let json = serde_json::to_value(&origin).unwrap();
        assert_eq!(json.get("kind"), Some(&serde_json::json!("git")));
        assert_eq!(json.get("refPath"), Some(&serde_json::json!("https://x")));
        assert_eq!(json.get("subpath"), Some(&serde_json::json!("skills/a")));
        assert_eq!(json.get("revision"), Some(&serde_json::json!("abc")));
        assert_eq!(OriginInfo::local().kind, OriginKind::Local);
    }

    #[test]
    fn memory_index_store_delete_semantics() {
        let store = MemoryIndexStore::new();
        let tool_a = ToolId("claude_code".into());
        let tool_b = ToolId("codex".into());
        let id = store
            .insert_skill(
                "demo",
                "demo skill",
                &OriginInfo::local(),
                Path::new("/central/demo"),
                None,
            )
            .unwrap();
        store.upsert_target(id, &tool_a, Path::new("/a/demo"), SyncMode::Symlink, None).unwrap();
        store.upsert_target(id, &tool_b, Path::new("/b/demo"), SyncMode::Copy, None).unwrap();

        // 删除单条 target：另一条保留。
        store.delete_target(id, &tool_a).unwrap();
        let targets = store.targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].tool_id, tool_b);

        // 删除 skill：行与残余 target 一并清理。
        store.delete_skill(id).unwrap();
        assert!(store.skills().is_empty());
        assert!(store.targets().is_empty());
    }

    #[test]
    fn memory_index_store_stores_origin() {
        let store = MemoryIndexStore::new();
        let origin = OriginInfo::git("https://x", None, Some("r1".into()));
        store
            .insert_skill("demo", "", &origin, Path::new("/central/demo"), None)
            .unwrap();
        assert_eq!(store.skills()[0].origin, origin);
    }
}
