//! migration 框架（TASKBOOK §3.2）：版本表 + 顺序迁移。
//!
//! v1 只有一个迁移 `migrate_v0_to_v1`（6 张表）。新增迁移时在
//! [`MIGRATIONS`] 末尾追加（版本号严格递增），禁止修改已发布的迁移。

use rusqlite::Connection;
use skillbox_domain::{AppError, AppResult};

/// v0 → v1：初始 6 张表。
const MIGRATE_V0_TO_V1: &str = r#"
CREATE TABLE IF NOT EXISTS skills (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL,
    description    TEXT    NOT NULL DEFAULT '',
    origin_kind    TEXT    NOT NULL,             -- local | git | archive
    origin_ref     TEXT,                         -- 仓库 URL / 本地路径 / 压缩包路径
    origin_subpath TEXT,                         -- 仓库内子路径
    origin_revision TEXT,                        -- git revision
    canonical_path TEXT    NOT NULL UNIQUE,      -- 中央库唯一正本路径
    content_hash   TEXT,                         -- 正本内容哈希（超限为近似值）
    created_at     INTEGER NOT NULL,             -- epoch ms
    updated_at     INTEGER NOT NULL              -- epoch ms
);

CREATE TABLE IF NOT EXISTS skill_targets (
    skill_id     INTEGER NOT NULL,
    tool_id      TEXT    NOT NULL,
    path         TEXT    NOT NULL,
    mode         TEXT    NOT NULL,               -- symlink | copy
    state        TEXT    NOT NULL,               -- 上次观测状态（仅作意图参考，非真相）
    synced_at    INTEGER NOT NULL,               -- epoch ms
    source_hash  TEXT,                           -- 部署时正本哈希（CopyDrift 比对）
    UNIQUE (skill_id, tool_id)
);

CREATE TABLE IF NOT EXISTS skill_tags (
    skill_id INTEGER NOT NULL,
    tag      TEXT    NOT NULL,
    PRIMARY KEY (skill_id, tag)
);

CREATE TABLE IF NOT EXISTS scan_cache (
    path         TEXT    PRIMARY KEY,            -- 目录绝对路径
    fingerprint  TEXT    NOT NULL,               -- JSON: {mtimeMs, size, childCount}
    last_seen_at INTEGER NOT NULL                -- epoch ms
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,                    -- epoch ms
    kind    TEXT    NOT NULL,
    target  TEXT    NOT NULL,
    message TEXT    NOT NULL
);
"#;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "migrate_v0_to_v1",
    sql: MIGRATE_V0_TO_V1,
}];

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::database(e.to_string())
}

/// 创建版本表并顺序应用未执行的迁移（每个迁移单事务提交）。
pub fn run_migrations(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT    NOT NULL,
            applied_at INTEGER NOT NULL
        );",
    )
    .map_err(db_err)?;

    for m in MIGRATIONS {
        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
                [m.version],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_err)?
            > 0;
        if applied {
            continue;
        }
        let tx = conn.unchecked_transaction().map_err(db_err)?;
        tx.execute_batch(m.sql).map_err(db_err)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![m.version, m.name, now_ms()],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
    }
    Ok(())
}

/// 当前 schema 版本；空库返回 0。
pub fn current_version(conn: &Connection) -> AppResult<i64> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_err)?
        > 0;
    if !exists {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(db_err)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_reaches_v1() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
        run_migrations(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);
        // 版本表恰好记录一条
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn skill_targets_unique_pair() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO skill_targets (skill_id, tool_id, path, mode, state, synced_at) \
             VALUES (1, 'claude_code', '/x', 'symlink', 'linkToSource', 1)",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO skill_targets (skill_id, tool_id, path, mode, state, synced_at) \
             VALUES (1, 'claude_code', '/y', 'copy', 'realDir', 1)",
            [],
        );
        assert!(dup.is_err(), "UNIQUE(skill_id, tool_id) 必须生效");
    }
}
