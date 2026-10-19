//! # skillbox-store
//!
//! SQLite 索引层（TASKBOOK §3.2）。v1 = 6 张表 + migration 框架。
//!
//! 【硬约束】`skillbox.db` 禁止存放 skill 正文、frontmatter 全文
//!（只存 name / description 摘要）、任何密钥。

pub mod migrate;

pub use migrate::{current_version, run_migrations};

use std::path::Path;

use rusqlite::Connection;
use skillbox_domain::{AppError, AppResult};

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::database(e.to_string())
}

/// 打开（或创建）索引库并跑到最新 schema。
/// 启用 WAL 与外键约束；`skillbox.db` 位于 `~/.skillbox/`。
pub fn open(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).map_err(db_err)?;
    conn.pragma_update(None, "journal_mode", "WAL").map_err(db_err)?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(db_err)?;
    conn.pragma_update(None, "busy_timeout", 5000).map_err(db_err)?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// 内存库（测试用）。
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory().map_err(db_err)?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(db_err)?;
    run_migrations(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_six_tables() {
        let conn = open_in_memory().unwrap();
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' \
                     AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations' \
                     ORDER BY name",
                )
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(
            tables,
            vec![
                "audit_log",
                "scan_cache",
                "settings",
                "skill_tags",
                "skill_targets",
                "skills",
            ]
        );
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = open_in_memory().unwrap();
        // 再跑一遍不报错、版本不变。
        run_migrations(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 1);
    }

    #[test]
    fn skills_unique_canonical_path() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO skills (name, description, origin_kind, canonical_path, created_at, updated_at) \
             VALUES ('a', '', 'local', '/x/a', 1, 1)",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO skills (name, description, origin_kind, canonical_path, created_at, updated_at) \
             VALUES ('b', '', 'local', '/x/a', 1, 1)",
            [],
        );
        assert!(dup.is_err(), "canonical_path 必须 UNIQUE");
    }
}
