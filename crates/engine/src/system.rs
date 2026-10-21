//! system —— cache / trash 资源管理（TASKBOOK §8 system 组的盘上操作）。
//!
//! - cache（`~/.skillbox/cache`）：git 克隆体等可再生内容。inspect 只读；
//!   clear 支持按名清理与 dry_run（真实执行前完成全部校验，零半清状态）。
//! - trash（`~/.skillbox/trash`）：v1 无硬删除（§5.3），所有删除先入
//!   `trash/<now_ms>/<name>`。list 按槽位时间倒序（最新在前）；
//!   restore 把条目搬回指定根目录（同卷 rename），dest 冲突整体拒绝。
//! - slot / name 只接受单段组件（拒绝 `..`、分隔符），杜绝路径穿越。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use skillbox_domain::{AppError, AppResult, PathConflict};
use skillbox_ports::{EntryKind, EntryMeta, FileSystem, Linker, WatchMute};

use crate::deploy::write::MuteGuard;

// ---------------------------------------------------------------------------
// 结果类型
// ---------------------------------------------------------------------------

/// cache 条目（`cache_inspect` 返回）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub name: String,
    pub path: PathBuf,
    /// 目录为递归总字节；符号链接只计链接自身。
    pub size_bytes: u64,
    pub modified_ms: i64,
}

/// cache 清理报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearReport {
    pub dry_run: bool,
    /// 本次选中的条目名（按盘上顺序）。
    pub removed: Vec<String>,
    pub freed_bytes: u64,
}

/// trash 条目（`trash_list` 返回）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    /// 槽位目录名（移入 trash 时的 `now_ms`）。
    pub slot: String,
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub modified_ms: i64,
}

/// trash 恢复结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRestoreOutcome {
    pub dry_run: bool,
    pub restored_to: PathBuf,
}

// ---------------------------------------------------------------------------
// cache
// ---------------------------------------------------------------------------

/// 列出 cache 根的直接子项（按名排序）；cache 根不存在时返回空。
pub fn cache_inspect(fs: &dyn FileSystem, cache_root: &Path) -> AppResult<Vec<CacheEntry>> {
    let mut entries = list_dir(fs, cache_root)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries
        .into_iter()
        .map(|e| CacheEntry {
            size_bytes: entry_size(fs, cache_root.join(&e.name), &e),
            path: cache_root.join(&e.name),
            name: e.name,
            modified_ms: e.modified_ms,
        })
        .collect())
}

/// 清理 cache。`names = None` 清全部；否则只清指定条目（任一不存在则
/// 整体拒绝，零写入）。`dry_run = true` 只报告不清。
pub fn cache_clear(
    fs: &dyn FileSystem,
    linker: &dyn Linker,
    watcher: Option<&Arc<dyn WatchMute>>,
    cache_root: &Path,
    names: Option<&[String]>,
    dry_run: bool,
) -> AppResult<CacheClearReport> {
    let all = list_dir(fs, cache_root)?;
    let selected: Vec<EntryMeta> = match names {
        None => all,
        Some(names) => {
            for name in names {
                ensure_safe_component(name)?;
            }
            let mut missing: Vec<String> = names
                .iter()
                .filter(|n| !all.iter().any(|e| &e.name == *n))
                .cloned()
                .collect();
            if !missing.is_empty() {
                missing.sort();
                return Err(AppError::not_found(format!(
                    "cache 中不存在以下条目: {}",
                    missing.join(", ")
                )));
            }
            all.into_iter().filter(|e| names.contains(&e.name)).collect()
        }
    };
    let freed_bytes: u64 = selected
        .iter()
        .map(|e| entry_size(fs, cache_root.join(&e.name), e))
        .sum();
    let removed = selected.iter().map(|e| e.name.clone()).collect();
    let report = CacheClearReport { dry_run, removed, freed_bytes };
    if dry_run {
        return Ok(report);
    }

    let _mute = MuteGuard::new(watcher);
    for entry in &selected {
        let path = cache_root.join(&entry.name);
        match entry.kind {
            EntryKind::Dir => fs.remove_dir_all(&path)?,
            EntryKind::Symlink => linker.remove_link(&path)?,
            EntryKind::File | EntryKind::Other => fs.remove_file(&path)?,
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------------------
// trash
// ---------------------------------------------------------------------------

/// 列出 trash 全部条目：槽位按时间倒序（最新在前），槽内按名排序。
/// trash 根不存在时返回空。
pub fn trash_list(fs: &dyn FileSystem, trash_root: &Path) -> AppResult<Vec<TrashEntry>> {
    let mut out = Vec::new();
    if !fs.exists(trash_root) {
        return Ok(out);
    }
    let mut slots: Vec<(i64, String)> = fs
        .read_dir(trash_root)?
        .into_iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .map(|e| (e.name.parse::<i64>().unwrap_or(i64::MIN), e.name))
        .collect();
    slots.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, slot) in slots {
        let slot_dir = trash_root.join(&slot);
        let mut entries = list_dir(fs, &slot_dir)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for e in entries {
            out.push(TrashEntry {
                size_bytes: entry_size(fs, slot_dir.join(&e.name), &e),
                path: slot_dir.join(&e.name),
                kind: e.kind,
                name: e.name,
                modified_ms: e.modified_ms,
                slot: slot.clone(),
            });
        }
    }
    Ok(out)
}

/// 把 `trash/<slot>/<name>` 恢复到 `dest_root/<name>`（同卷 rename）。
/// dest 已存在 → `TargetConflict` 整体拒绝（零写入）。
pub fn trash_restore(
    fs: &dyn FileSystem,
    watcher: Option<&Arc<dyn WatchMute>>,
    trash_root: &Path,
    slot: &str,
    name: &str,
    dest_root: &Path,
    dry_run: bool,
) -> AppResult<TrashRestoreOutcome> {
    ensure_safe_component(slot)?;
    ensure_safe_component(name)?;
    let src = trash_root.join(slot).join(name);
    if !fs.exists(&src) {
        return Err(AppError::not_found(format!("trash 中不存在该条目: {}/{}", slot, name)));
    }
    let dest = dest_root.join(name);
    if fs.exists(&dest) {
        return Err(AppError::target_conflict(
            "目标位置已存在同名条目，拒绝覆盖（未做任何写盘）",
            vec![PathConflict { path: dest, reason: "恢复目标已存在".to_string() }],
        ));
    }
    if dry_run {
        return Ok(TrashRestoreOutcome { dry_run: true, restored_to: dest });
    }

    let _mute = MuteGuard::new(watcher);
    fs.create_dir_all(dest_root)?;
    fs.rename(&src, &dest).map_err(|e| {
        AppError::io(format!(
            "恢复失败（可能跨卷；trash 条目未动）: {} -> {}: {e}",
            src.display(),
            dest.display()
        ))
    })?;
    // 槽位空了就顺手清掉（remove_dir 只删空目录，失败可忽略）。
    let _ = fs.remove_dir(&trash_root.join(slot));
    Ok(TrashRestoreOutcome { dry_run: false, restored_to: dest })
}

// ---------------------------------------------------------------------------
// 内部
// ---------------------------------------------------------------------------

/// 列目录直接子项；目录不存在返回空（cache / trash 均允许缺失）。
fn list_dir(fs: &dyn FileSystem, dir: &Path) -> AppResult<Vec<EntryMeta>> {
    if !fs.exists(dir) {
        return Ok(Vec::new());
    }
    fs.read_dir(dir)
}

/// 条目大小：目录递归累计；其余取 lstat len（符号链接不跟随）。
fn entry_size(fs: &dyn FileSystem, path: PathBuf, meta: &EntryMeta) -> u64 {
    match meta.kind {
        EntryKind::Dir => dir_size(fs, &path),
        _ => meta.len,
    }
}

fn dir_size(fs: &dyn FileSystem, path: &Path) -> u64 {
    let Ok(entries) = fs.read_dir(path) else { return 0 };
    entries
        .into_iter()
        .map(|e| match e.kind {
            EntryKind::Dir => dir_size(fs, &path.join(&e.name)),
            _ => e.len,
        })
        .sum()
}

/// slot / name 必须是单段安全组件：非空、非 `.`/`..`、不含分隔符。
fn ensure_safe_component(s: &str) -> AppResult<()> {
    if s.is_empty() || s == "." || s == ".." || s.contains('/') || s.contains('\\') {
        return Err(AppError::invalid_input(format!("非法的路径组件: {s:?}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::CountingFs;
    use skillbox_ports::{ProdFileSystem, ProdLinker};

    struct SysEnv {
        root: tempfile::TempDir,
        cache: PathBuf,
        trash: PathBuf,
        dest: PathBuf,
        fs: Arc<ProdFileSystem>,
        linker: Arc<ProdLinker>,
    }

    impl SysEnv {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let cache = root.path().join("cache");
            let trash = root.path().join("trash");
            let dest = root.path().join("dest");
            std::fs::create_dir_all(&cache).unwrap();
            std::fs::create_dir_all(&trash).unwrap();
            std::fs::create_dir_all(&dest).unwrap();
            SysEnv { root, cache, trash, dest, fs: Arc::new(ProdFileSystem), linker: Arc::new(ProdLinker) }
        }

        fn counting(&self) -> (Arc<CountingFs>, Arc<std::sync::atomic::AtomicUsize>) {
            CountingFs::wrap(self.fs.clone())
        }

        /// 造一个 cache 条目：目录（含 name.txt）或文件。
        fn cache_item(&self, name: &str, bytes: usize) {
            let path = self.cache.join(name);
            if name.contains('.') {
                std::fs::write(&path, vec![b'x'; bytes]).unwrap();
            } else {
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("SKILL.md"), vec![b'x'; bytes]).unwrap();
            }
        }

        /// 在 trash/<slot>/<name> 造一个目录条目（含 SKILL.md）。
        fn trash_item(&self, slot: &str, name: &str, marker: &str) -> PathBuf {
            let dir = self.trash.join(slot).join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), marker).unwrap();
            dir
        }
    }

    #[test]
    fn cache_inspect_reports_sizes_and_handles_missing_root() {
        let env = SysEnv::new();
        env.cache_item("demo", 10);
        env.cache_item("b.txt", 5);
        let entries = cache_inspect(env.fs.as_ref(), &env.cache).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "b.txt");
        assert_eq!(entries[0].size_bytes, 5);
        assert_eq!(entries[1].name, "demo");
        assert_eq!(entries[1].size_bytes, 10);
        assert!(entries[0].modified_ms > 0);

        let missing = env.root.path().join("no-such-cache");
        assert!(cache_inspect(env.fs.as_ref(), &missing).unwrap().is_empty());
    }

    #[test]
    fn cache_clear_all_removes_everything() {
        let env = SysEnv::new();
        env.cache_item("demo", 10);
        env.cache_item("b.txt", 5);
        let report = cache_clear(
            env.fs.as_ref(),
            env.linker.as_ref(),
            None,
            &env.cache,
            None,
            false,
        )
        .unwrap();
        assert_eq!(report.removed, vec!["b.txt", "demo"]);
        assert_eq!(report.freed_bytes, 15);
        assert!(!report.dry_run);
        assert!(cache_inspect(env.fs.as_ref(), &env.cache).unwrap().is_empty());
    }

    #[test]
    fn cache_clear_selected_names_only() {
        let env = SysEnv::new();
        env.cache_item("demo", 10);
        env.cache_item("other", 7);
        let names = vec!["demo".to_string()];
        cache_clear(env.fs.as_ref(), env.linker.as_ref(), None, &env.cache, Some(&names), false)
            .unwrap();
        let left = cache_inspect(env.fs.as_ref(), &env.cache).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "other");
    }

    #[test]
    fn cache_clear_missing_name_rejected_zero_writes() {
        let env = SysEnv::new();
        env.cache_item("demo", 10);
        let (fs, writes) = env.counting();
        let names = vec!["demo".to_string(), "nope".to_string()];
        let err = cache_clear(fs.as_ref(), env.linker.as_ref(), None, &env.cache, Some(&names), false)
            .unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(cache_inspect(env.fs.as_ref(), &env.cache).unwrap().len(), 1);
    }

    #[test]
    fn cache_clear_dry_run_zero_writes() {
        let env = SysEnv::new();
        env.cache_item("demo", 10);
        let (fs, writes) = env.counting();
        let report = cache_clear(fs.as_ref(), env.linker.as_ref(), None, &env.cache, None, true)
            .unwrap();
        assert!(report.dry_run);
        assert_eq!(report.removed, vec!["demo"]);
        assert_eq!(report.freed_bytes, 10);
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(cache_inspect(env.fs.as_ref(), &env.cache).unwrap().len(), 1);
    }

    #[test]
    fn trash_list_sorted_newest_slot_first_and_missing_root_empty() {
        let env = SysEnv::new();
        env.trash_item("100", "alpha", "a");
        env.trash_item("200", "beta", "b");
        let entries = trash_list(env.fs.as_ref(), &env.trash).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].slot.as_str(), entries[0].name.as_str()), ("200", "beta"));
        assert_eq!(entries[1].slot, "100");
        assert_eq!(entries[0].size_bytes, 1);

        let missing = env.root.path().join("no-such-trash");
        assert!(trash_list(env.fs.as_ref(), &missing).unwrap().is_empty());
    }

    #[test]
    fn trash_restore_moves_entry_to_dest() {
        let env = SysEnv::new();
        let src = env.trash_item("100", "alpha", "hello");
        let outcome =
            trash_restore(env.fs.as_ref(), None, &env.trash, "100", "alpha", &env.dest, false)
                .unwrap();
        assert!(!outcome.dry_run);
        assert_eq!(outcome.restored_to, env.dest.join("alpha"));
        assert!(!src.exists());
        assert_eq!(
            std::fs::read_to_string(env.dest.join("alpha").join("SKILL.md")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn trash_restore_conflict_rejected_zero_writes() {
        let env = SysEnv::new();
        let src = env.trash_item("100", "alpha", "hello");
        std::fs::create_dir_all(env.dest.join("alpha")).unwrap();
        let (fs, writes) = env.counting();
        let err = trash_restore(fs.as_ref(), None, &env.trash, "100", "alpha", &env.dest, false)
            .unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::TargetConflict);
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(src.exists());
    }

    #[test]
    fn trash_restore_dry_run_zero_writes() {
        let env = SysEnv::new();
        let src = env.trash_item("100", "alpha", "hello");
        let (fs, writes) = env.counting();
        let outcome =
            trash_restore(fs.as_ref(), None, &env.trash, "100", "alpha", &env.dest, true).unwrap();
        assert!(outcome.dry_run);
        assert_eq!(outcome.restored_to, env.dest.join("alpha"));
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(src.exists());
        assert!(!env.dest.join("alpha").exists());
    }

    #[test]
    fn trash_restore_rejects_traversal_and_bad_components() {
        let env = SysEnv::new();
        for (slot, name) in [("..", "alpha"), ("100", "a/b"), ("100", ".."), ("", "alpha"), ("100", "")] {
            let err = trash_restore(env.fs.as_ref(), None, &env.trash, slot, name, &env.dest, false)
                .unwrap_err();
            assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput, "应拒绝 {slot:?}/{name:?}");
        }
    }

    #[test]
    fn trash_restore_missing_entry_not_found() {
        let env = SysEnv::new();
        let err = trash_restore(env.fs.as_ref(), None, &env.trash, "100", "ghost", &env.dest, false)
            .unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::NotFound);
    }

    #[test]
    fn cache_clear_rejects_unsafe_names() {
        let env = SysEnv::new();
        let names = vec!["..".to_string()];
        let err = cache_clear(env.fs.as_ref(), env.linker.as_ref(), None, &env.cache, Some(&names), false)
            .unwrap_err();
        assert_eq!(err.kind, skillbox_domain::ErrorKind::InvalidInput);
    }
}
