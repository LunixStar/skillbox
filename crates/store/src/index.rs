//! [`IndexStore`] 的 SQLite 实现（`skills` / `skill_targets` 两表写入面）。
//!
//! 与 [`crate::Store`] 共享同一 `Arc<Mutex<Connection>>`：ipc 侧一个连接
//! 同时服务读（Store）与写（DbIndexStore），WAL + busy_timeout 下无锁竞争
//! 之外的额外代价。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use skillbox_domain::{AppError, AppResult, OriginKind, SkillId, SyncMode, ToolId};
use skillbox_ports::{IndexStore, OriginInfo};

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::database(e.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// SQLite 索引写入端口实现。
#[derive(Debug, Clone)]
pub struct DbIndexStore {
    conn: Arc<Mutex<Connection>>,
}

impl DbIndexStore {
    /// 以共享连接构造（`Store::index_store` 的底层）。
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        DbIndexStore { conn }
    }

    /// 独立打开（不与 Store 共享连接的场景）。
    pub fn open(path: &Path) -> AppResult<Self> {
        Ok(Self::new(Arc::new(Mutex::new(crate::open(path)?))))
    }
}

impl IndexStore for DbIndexStore {
    fn insert_skill(
        &self,
        name: &str,
        description: &str,
        origin: &OriginInfo,
        canonical_path: &Path,
        content_hash: Option<&str>,
    ) -> AppResult<SkillId> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        let now = now_ms();
        // state 列仅作"上次意图参考"，写入时磁盘真相未知，占位 unknown。
        conn.execute(
            "INSERT INTO skills (name, description, origin_kind, origin_ref, \
             origin_subpath, origin_revision, canonical_path, content_hash, \
             created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            rusqlite::params![
                name,
                description,
                origin.kind.as_str(),
                origin.ref_path,
                origin.subpath,
                origin.revision,
                canonical_path.to_string_lossy(),
                content_hash,
                now,
            ],
        )
        .map_err(db_err)?;
        Ok(SkillId(conn.last_insert_rowid()))
    }

    fn upsert_target(
        &self,
        skill_id: SkillId,
        tool_id: &ToolId,
        path: &Path,
        mode: SyncMode,
        source_hash: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        conn.execute(
            "INSERT INTO skill_targets (skill_id, tool_id, path, mode, state, synced_at, source_hash) \
             VALUES (?1, ?2, ?3, ?4, 'unknown', ?5, ?6) \
             ON CONFLICT(skill_id, tool_id) DO UPDATE SET \
             path = excluded.path, mode = excluded.mode, state = excluded.state, \
             synced_at = excluded.synced_at, source_hash = excluded.source_hash",
            rusqlite::params![
                skill_id.0,
                tool_id.0,
                path.to_string_lossy(),
                mode.as_str(),
                now_ms(),
                source_hash,
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    fn delete_target(&self, skill_id: SkillId, tool_id: &ToolId) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        conn.execute(
            "DELETE FROM skill_targets WHERE skill_id = ?1 AND tool_id = ?2",
            rusqlite::params![skill_id.0, tool_id.0],
        )
        .map_err(db_err)?;
        Ok(())
    }

    fn delete_skill(&self, skill_id: SkillId) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        let tx = conn.unchecked_transaction().map_err(db_err)?;
        tx.execute("DELETE FROM skill_targets WHERE skill_id = ?1", [skill_id.0])
            .map_err(db_err)?;
        tx.execute("DELETE FROM skill_tags WHERE skill_id = ?1", [skill_id.0])
            .map_err(db_err)?;
        tx.execute("DELETE FROM skills WHERE id = ?1", [skill_id.0])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }
}

/// `skill_targets` 单行（读侧投影；engine 的 `TargetRecord` 组装原料）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRow {
    pub skill_id: SkillId,
    pub tool_id: ToolId,
    pub path: PathBuf,
    pub mode: SyncMode,
    pub synced_at: i64,
    pub source_hash: Option<String>,
}

/// 行 → `TargetRow`。未知 mode 字面值报 Database（schema 违约）。
fn target_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TargetRow> {
    let tool_id: String = row.get(1)?;
    let mode: String = row.get(3)?;
    Ok(TargetRow {
        skill_id: SkillId(row.get(0)?),
        tool_id: ToolId(tool_id),
        path: PathBuf::from(row.get::<_, String>(2)?),
        mode: SyncMode::from_str_opt(&mode).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, {
                let mode = mode.clone();
                Box::new(std::io::Error::other(format!("未知 mode 字面值: {mode}")))
            })
        })?,
        synced_at: row.get(4)?,
        source_hash: row.get(5)?,
    })
}

// ---------------------------------------------------------------------------
// Store：读侧 + settings（IPC 直接查询的落点，见 ports::IndexStore 文档）
// ---------------------------------------------------------------------------

use skillbox_domain::SkillMeta;

/// SQLite 读侧句柄（与 [`DbIndexStore`] 共享连接）。
#[derive(Debug, Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// 打开（或创建）`skillbox.db` 并跑到最新 schema。
    pub fn open(path: &Path) -> AppResult<Store> {
        Ok(Store { conn: Arc::new(Mutex::new(crate::open(path)?)) })
    }

    /// 共享同一连接的索引写入端口（`Arc<DbIndexStore>` 供 engine 注入）。
    pub fn index_store(&self) -> Arc<DbIndexStore> {
        Arc::new(DbIndexStore::new(self.conn.clone()))
    }

    /// 全部 skill 行（按 id 升序）。
    pub fn list_skills(&self) -> AppResult<Vec<SkillMeta>> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, origin_kind, origin_ref, origin_subpath, \
                 origin_revision, canonical_path, content_hash, created_at, updated_at \
                 FROM skills ORDER BY id",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], skill_meta_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// 单个 skill 行。
    pub fn get_skill(&self, id: SkillId) -> AppResult<Option<SkillMeta>> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        conn.query_row(
            "SELECT id, name, description, origin_kind, origin_ref, origin_subpath, \
             origin_revision, canonical_path, content_hash, created_at, updated_at \
             FROM skills WHERE id = ?1",
            [id.0],
            skill_meta_row,
        )
        .optional()
        .map_err(db_err)
    }

    /// 全部部署意图记录（按 skill_id, tool_id 升序）。
    pub fn list_targets(&self) -> AppResult<Vec<TargetRow>> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        let mut stmt = conn
            .prepare(
                "SELECT skill_id, tool_id, path, mode, synced_at, source_hash \
                 FROM skill_targets ORDER BY skill_id, tool_id",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], target_row)
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// 读 settings 值。
    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .map_err(db_err)
    }

    /// 写 settings 值（upsert）。
    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| AppError::database("DB 连接锁中毒"))?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

/// 行 → `SkillMeta`。
fn skill_meta_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillMeta> {
    let origin_kind: String = row.get(3)?;
    Ok(SkillMeta {
        id: SkillId(row.get(0)?),
        name: row.get(1)?,
        description: row.get(2)?,
        origin_kind: OriginKind::from_str_opt(&origin_kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, {
                let origin_kind = origin_kind.clone();
                Box::new(std::io::Error::other(format!("未知 origin_kind: {origin_kind}")))
            })
        })?,
        origin_ref: row.get(4)?,
        origin_subpath: row.get(5)?,
        origin_revision: row.get(6)?,
        canonical_path: PathBuf::from(row.get::<_, String>(7)?),
        content_hash: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 共享内存库（同一 `Connection` 模拟 Store + DbIndexStore 共存）。
    fn shared() -> (Store, Arc<DbIndexStore>) {
        let conn = crate::open_in_memory().unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let store = Store { conn: conn.clone() };
        (store, Arc::new(DbIndexStore::new(conn)))
    }

    #[test]
    fn insert_then_read_skill_meta() {
        let (store, index) = shared();
        let origin = OriginInfo::git("https://x", Some("sub".into()), Some("r1".into()));
        let id = index
            .insert_skill("demo", "描述", &origin, Path::new("/central/demo"), Some("h1"))
            .unwrap();

        let meta = store.get_skill(id).unwrap().unwrap();
        assert_eq!(meta.id, id);
        assert_eq!(meta.name, "demo");
        assert_eq!(meta.origin_kind, OriginKind::Git);
        assert_eq!(meta.origin_ref.as_deref(), Some("https://x"));
        assert_eq!(meta.origin_subpath.as_deref(), Some("sub"));
        assert_eq!(meta.origin_revision.as_deref(), Some("r1"));
        assert_eq!(meta.canonical_path, PathBuf::from("/central/demo"));
        assert_eq!(meta.content_hash.as_deref(), Some("h1"));
        assert_eq!(store.list_skills().unwrap().len(), 1);
        assert!(store.get_skill(SkillId(999)).unwrap().is_none());
    }

    #[test]
    fn target_upsert_delete_roundtrip() {
        let (store, index) = shared();
        let id = index
            .insert_skill("demo", "", &OriginInfo::local(), Path::new("/central/demo"), None)
            .unwrap();
        let tool = ToolId("claude_code".into());

        index.upsert_target(id, &tool, Path::new("/a/demo"), SyncMode::Symlink, None).unwrap();
        index
            .upsert_target(id, &tool, Path::new("/a2/demo"), SyncMode::Copy, Some("h"))
            .unwrap();
        let rows = store.list_targets().unwrap();
        assert_eq!(rows.len(), 1, "UNIQUE(skill_id, tool_id) 应 upsert 而非追加");
        assert_eq!(rows[0].path, PathBuf::from("/a2/demo"));
        assert_eq!(rows[0].mode, SyncMode::Copy);
        assert_eq!(rows[0].source_hash.as_deref(), Some("h"));

        index.delete_target(id, &tool).unwrap();
        assert!(store.list_targets().unwrap().is_empty());
    }

    #[test]
    fn delete_skill_cascades_rows() {
        let (store, index) = shared();
        let id = index
            .insert_skill("demo", "", &OriginInfo::local(), Path::new("/central/demo"), None)
            .unwrap();
        let tool = ToolId("codex".into());
        index.upsert_target(id, &tool, Path::new("/c/demo"), SyncMode::Symlink, None).unwrap();
        conn_tag(&store, id);

        index.delete_skill(id).unwrap();
        assert!(store.get_skill(id).unwrap().is_none());
        assert!(store.list_targets().unwrap().is_empty());
        let tags: i64 = {
            let conn = store.conn.lock().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM skill_tags WHERE skill_id = ?1",
                [id.0],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(tags, 0);
    }

    fn conn_tag(store: &Store, id: SkillId) {
        let conn = store.conn.lock().unwrap();
        conn.execute("INSERT INTO skill_tags (skill_id, tag) VALUES (?1, 't')", [id.0])
            .unwrap();
    }

    #[test]
    fn settings_upsert() {
        let (store, _index) = shared();
        assert_eq!(store.get_setting("k").unwrap(), None);
        store.set_setting("k", "v1").unwrap();
        assert_eq!(store.get_setting("k").unwrap().as_deref(), Some("v1"));
        store.set_setting("k", "v2").unwrap();
        assert_eq!(store.get_setting("k").unwrap().as_deref(), Some("v2"));
    }

    #[test]
    fn duplicate_canonical_path_rejected() {
        let (store, index) = shared();
        index
            .insert_skill("a", "", &OriginInfo::local(), Path::new("/x/a"), None)
            .unwrap();
        let dup = index.insert_skill("b", "", &OriginInfo::local(), Path::new("/x/a"), None);
        assert!(dup.is_err());
        assert!(matches!(
            dup.unwrap_err().kind,
            skillbox_domain::ErrorKind::Database
        ));
        assert!(store.get_skill(SkillId(1)).is_ok());
    }
}
