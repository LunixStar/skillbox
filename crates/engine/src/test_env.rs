//! 引擎级共享测试设施（仅 cfg(test)）：doctor / fix 测试共用的固定时钟、
//! 真实文件系统环境与写入计数文件系统。置于 crate 根而非某个子模块，
//! 因为 `pub(super)` 无法跨兄弟子树（doctor::tests 与 fix::tests）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use skillbox_domain::{AppResult, SkillId, SyncMode, ToolId};
use skillbox_ports::{
    Clock, EntryMeta, FileSystem, Hasher, IndexStore, Linker, MemoryIndexStore, ProdFileSystem,
    ProdHasher, ProdLinker,
};

use crate::doctor::{hash_dir, DoctorTarget, HealthContext};
use crate::types::{IndexView, SkillRecord, TargetRecord};

/// 固定时钟：duration / trash 槽位确定可断言。
pub(crate) const NOW_MS: i64 = 1_700_000_000_000;

pub(crate) struct FixedClock;

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        NOW_MS
    }
}

/// doctor / fix 测试环境：真实文件系统（tempfile）+ 内存索引。
pub(crate) struct HealthTestEnv {
    pub(crate) root: tempfile::TempDir,
    /// 中央库 skills 根（`<root>/central`）。
    pub(crate) central_root: PathBuf,
    pub(crate) trash: PathBuf,
    pub(crate) fs: Arc<dyn FileSystem>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) linker: Arc<dyn Linker>,
    pub(crate) hasher: Arc<dyn Hasher>,
    pub(crate) store: Arc<MemoryIndexStore>,
}

impl HealthTestEnv {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let central_root = root.path().join("central");
        let trash = root.path().join("trash");
        std::fs::create_dir_all(&central_root).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        HealthTestEnv {
            root,
            central_root,
            trash,
            fs: Arc::new(ProdFileSystem),
            clock: Arc::new(FixedClock),
            linker: Arc::new(ProdLinker),
            hasher: Arc::new(ProdHasher),
            store: Arc::new(MemoryIndexStore::new()),
        }
    }

    /// 工具 skills 根（`<root>/tools/<tool>/skills`）。
    pub(crate) fn skills_dir(&self, tool: &str) -> PathBuf {
        self.root.path().join("tools").join(tool).join("skills")
    }

    /// 工具目录中的部署路径。
    pub(crate) fn target_path(&self, tool: &str, name: &str) -> PathBuf {
        self.skills_dir(tool).join(name)
    }

    /// 在中央库创建 skill 正本（SKILL.md frontmatter + 附加文件）。
    pub(crate) fn make_central_skill(&self, name: &str, extra: &[(&str, &[u8])]) -> PathBuf {
        let dir = self.central_root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n---\nbody of {name}"),
        )
        .unwrap();
        for (file, bytes) in extra {
            std::fs::write(dir.join(file), bytes).unwrap();
        }
        dir
    }

    /// 登记中央库 skill + 可选部署记录，返回 skill id。
    /// `targets`: `(tool, mode, source_hash)`；Copy 模式且 source_hash 为
    /// None 时自动填正本内容哈希。
    pub(crate) fn register(
        &self,
        name: &str,
        targets: &[(&str, SyncMode, Option<String>)],
    ) -> SkillId {
        let canonical = self.central_root.join(name);
        assert!(canonical.is_dir(), "先 make_central_skill: {}", canonical.display());
        let content_hash =
            hash_dir(self.fs.as_ref(), self.linker.as_ref(), self.hasher.as_ref(), &canonical);
        let id = self
            .store
            .insert_skill(name, &format!("{name} skill"), &canonical, content_hash.as_deref())
            .unwrap();
        for (tool, mode, source_hash) in targets {
            let source_hash = match mode {
                SyncMode::Copy => Some(
                    source_hash
                        .clone()
                        .unwrap_or_else(|| content_hash.clone().expect("content_hash 已算出")),
                ),
                SyncMode::Symlink => source_hash.clone(),
            };
            self.store
                .upsert_target(
                    id,
                    &ToolId((*tool).into()),
                    &self.target_path(tool, name),
                    *mode,
                    source_hash.as_deref(),
                )
                .unwrap();
        }
        id
    }

    /// 从内存索引组装 IndexView 快照。
    pub(crate) fn index_view(&self) -> IndexView {
        IndexView {
            skills: self
                .store
                .skills()
                .into_iter()
                .map(|s| SkillRecord {
                    id: s.id,
                    name: s.name,
                    canonical_path: s.canonical_path,
                    content_hash: s.content_hash,
                })
                .collect(),
            records: self
                .store
                .targets()
                .into_iter()
                .map(|t| TargetRecord {
                    skill_id: t.skill_id,
                    tool_id: t.tool_id,
                    path: t.path,
                    mode: t.mode,
                    source_hash: t.source_hash,
                })
                .collect(),
        }
    }

    /// 体检上下文（真实端口注入；顺带确保工具 skills 根存在）。
    pub(crate) fn ctx(&self, tools: &[&str]) -> HealthContext {
        self.ctx_with_fs(tools, self.fs.clone())
    }

    /// 体检上下文（自定义文件系统——零写入证明用 CountingFs）。
    /// 不自动创建工具 skills 根：不存在的根由 doctor 跳过（测试显式验证）。
    pub(crate) fn ctx_with_fs(&self, tools: &[&str], fs: Arc<dyn FileSystem>) -> HealthContext {
        HealthContext {
            targets: tools
                .iter()
                .map(|t| DoctorTarget {
                    tool_id: ToolId((*t).into()),
                    path: self.skills_dir(t),
                })
                .collect(),
            central_root: self.central_root.clone(),
            index: self.index_view(),
            fs,
            clock: self.clock.clone(),
            linker: self.linker.clone(),
            hasher: self.hasher.clone(),
        }
    }

    /// 创建目录链接：优先 symlink，失败退化为 junction（Windows 常态）。
    pub(crate) fn make_link(&self, target: &Path, link: &Path) {
        std::fs::create_dir_all(link.parent().expect("link 有父目录")).unwrap();
        if self.linker.symlink_dir(target, link).is_err() {
            self.linker.junction(target, link).unwrap();
        }
    }

    /// 创建悬空链接：先建目标占位并链接成功，再删除目标
    /// （Windows junction 对不存在目标的创建行为不稳定，此路径跨平台稳定）。
    pub(crate) fn make_broken_link(&self, target: &Path, link: &Path) {
        std::fs::create_dir_all(target).unwrap();
        self.make_link(target, link);
        std::fs::remove_dir_all(target).unwrap();
    }
}

/// 写入计数文件系统：doctor 零写入证明（计数全部变更类调用：
/// write / create_dir_all / rename / copy_file / copy_dir_all /
/// remove_dir / remove_dir_all / remove_file）。
pub(crate) struct CountingFs {
    inner: Arc<dyn FileSystem>,
    writes: Arc<AtomicUsize>,
}

impl CountingFs {
    pub(crate) fn wrap(inner: Arc<dyn FileSystem>) -> (Arc<CountingFs>, Arc<AtomicUsize>) {
        let writes = Arc::new(AtomicUsize::new(0));
        (Arc::new(CountingFs { inner, writes: writes.clone() }), writes)
    }

    fn count(&self) {
        self.writes.fetch_add(1, Ordering::Relaxed);
    }
}

impl FileSystem for CountingFs {
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    fn symlink_metadata(&self, path: &Path) -> AppResult<EntryMeta> {
        self.inner.symlink_metadata(path)
    }

    fn read_dir(&self, path: &Path) -> AppResult<Vec<EntryMeta>> {
        self.inner.read_dir(path)
    }

    fn read_head(&self, path: &Path, max_bytes: u64) -> AppResult<Vec<u8>> {
        self.inner.read_head(path, max_bytes)
    }

    fn read_to_string(&self, path: &Path) -> AppResult<String> {
        self.inner.read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> AppResult<()> {
        self.count();
        self.inner.write(path, contents)
    }

    fn create_dir_all(&self, path: &Path) -> AppResult<()> {
        self.count();
        self.inner.create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.count();
        self.inner.rename(from, to)
    }

    fn copy_file(&self, from: &Path, to: &Path) -> AppResult<u64> {
        self.count();
        self.inner.copy_file(from, to)
    }

    fn copy_dir_all(&self, from: &Path, to: &Path) -> AppResult<()> {
        self.count();
        self.inner.copy_dir_all(from, to)
    }

    fn remove_dir(&self, path: &Path) -> AppResult<()> {
        self.count();
        self.inner.remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> AppResult<()> {
        self.count();
        self.inner.remove_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> AppResult<()> {
        self.count();
        self.inner.remove_file(path)
    }

    fn canonicalize(&self, path: &Path) -> AppResult<PathBuf> {
        self.inner.canonicalize(path)
    }

    fn is_local_volume(&self, path: &Path) -> bool {
        self.inner.is_local_volume(path)
    }
}
