//! `scan_cache` 表的 [`ScanCache`] 实现（§7 约束 6）。
//!
//! 依赖边说明：store → ports 是本工程目前唯一的同层横向依赖
//! （`ScanCache` trait 定义在 ports——trait 抽象层，而 SQL 实现只能在
//! 持有 rusqlite 的 crate）。不构成环、不违反"禁止反向依赖 / 禁止跨层
//! 直达"；已在 docs/PROGRESS.md 备案，如不接受可把 trait 下沉 domain。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use skillbox_domain::{AppError, AppResult};
use skillbox_ports::{CachedScanEntry, ScanCache};

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::database(e.to_string())
}

/// SQLite 实现：`scan_cache` 表（path PK / fingerprint JSON / last_seen_at）。
///
/// 持有独立 `Connection`（WAL 支持多连接并发）；engine 侧以
/// `Arc<Mutex<DbScanCache>>` 串行化（`Connection` 非 `Sync`）。
/// fingerprint 列存放整个 `CachedScanEntry` 的 JSON——表结构注释中的
/// `{mtimeMs, size, childCount}` 是最小集，name / description 摘要为
/// 向后兼容的扩充（§3.1 允许）。
#[derive(Debug)]
pub struct DbScanCache {
    conn: Connection,
}

impl DbScanCache {
    /// 基于已打开的连接构造；幂等地确保 schema 到位。
    pub fn new(conn: Connection) -> AppResult<Self> {
        crate::run_migrations(&conn)?;
        Ok(Self { conn })
    }

    /// 打开（或创建）`skillbox.db` 并构造缓存（含 migration）。
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::new(crate::open(path)?)
    }
}

impl ScanCache for DbScanCache {
    fn get(&self, path: &Path) -> AppResult<Option<CachedScanEntry>> {
        let key = path.to_string_lossy().into_owned();
        let row: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT fingerprint, last_seen_at FROM scan_cache WHERE path = ?1",
                [&key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let Some((json, last_seen_ms)) = row else {
            return Ok(None);
        };
        let mut entry: CachedScanEntry = serde_json::from_str(&json)
            .map_err(|e| AppError::database(format!("scan_cache 反序列化失败: {e}")))?;
        // path / last_seen 以列为准（JSON 内是写入时的冗余快照）。
        entry.path = path.to_path_buf();
        entry.last_seen_ms = last_seen_ms;
        Ok(Some(entry))
    }

    fn put(&self, entry: &CachedScanEntry) -> AppResult<()> {
        let json = serde_json::to_string(entry)
            .map_err(|e| AppError::database(format!("scan_cache 序列化失败: {e}")))?;
        self.conn
            .execute(
                "INSERT INTO scan_cache (path, fingerprint, last_seen_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(path) DO UPDATE SET fingerprint = excluded.fingerprint, \
                 last_seen_at = excluded.last_seen_at",
                rusqlite::params![
                    entry.path.to_string_lossy(),
                    json,
                    entry.last_seen_ms
                ],
            )
            .map_err(db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use skillbox_domain::ScanFingerprint;

    fn sample(path: &str, name: &str, last_seen: i64) -> CachedScanEntry {
        CachedScanEntry {
            path: PathBuf::from(path),
            fingerprint: ScanFingerprint { mtime_ms: 1, size: 2, child_count: 3 },
            name: Some(name.into()),
            description: Some("描述".into()),
            last_seen_ms: last_seen,
        }
    }

    #[test]
    fn miss_then_roundtrip() {
        let conn = crate::open_in_memory().unwrap();
        let cache = DbScanCache::new(conn).unwrap();
        let path = Path::new("/x/skill-a");
        assert!(cache.get(path).unwrap().is_none(), "未命中必须返回 None");
        let entry = sample("/x/skill-a", "skill-a", 42);
        cache.put(&entry).unwrap();
        assert_eq!(cache.get(path).unwrap().unwrap(), entry, "命中应原样往返");
    }

    #[test]
    fn upsert_overwrites() {
        let conn = crate::open_in_memory().unwrap();
        let cache = DbScanCache::new(conn).unwrap();
        cache.put(&sample("/x/skill-a", "old", 1)).unwrap();
        cache.put(&sample("/x/skill-a", "renamed", 99)).unwrap();
        let hit = cache.get(Path::new("/x/skill-a")).unwrap().unwrap();
        assert_eq!(hit.name.as_deref(), Some("renamed"));
        assert_eq!(hit.last_seen_ms, 99, "last_seen_at 列必须随 upsert 更新");
    }
}
