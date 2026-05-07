//! Persona storage. One row per `scope` (typically `"global"`).
//! Mirrors the macOS-desktop wishlist `POST/GET /persona` API by
//! offering plain `get` / `upsert` over a free-form JSON blob.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone)]
pub struct Persona {
    pub scope: String,
    pub content_json: String,
    pub updated_at: DateTime<Utc>,
}

pub fn upsert(db: &Db, scope: &str, content_json: &str) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO personas (scope, content_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(scope) DO UPDATE SET
                content_json = excluded.content_json,
                updated_at = excluded.updated_at",
            params![
                scope,
                content_json,
                jarvis_core::time::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    })
}

pub fn get(db: &Db, scope: &str) -> DbResult<Option<Persona>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT scope, content_json, updated_at FROM personas WHERE scope = ?1",
        )?;
        let result = stmt
            .query_row(params![scope], |row| {
                let updated: String = row.get(2)?;
                let updated = DateTime::parse_from_rfc3339(&updated)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok(Persona {
                    scope: row.get(0)?,
                    content_json: row.get(1)?,
                    updated_at: updated,
                })
            })
            .optional()?;
        Ok(result)
    })
}
