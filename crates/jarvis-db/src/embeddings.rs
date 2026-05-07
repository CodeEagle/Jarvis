//! Persistent embedding store backing `jarvis_memory::vectors::VectorStore`.
//!
//! In-memory cosine search keeps the runtime simple, but losing the
//! index on every restart is wasteful. We mirror it to a single
//! `memory_embeddings` table keyed by memory id and let callers
//! `load_into` an empty VectorStore at boot.

use jarvis_core::time::now_iso;
use rusqlite::{params, OptionalExtension};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone)]
pub struct EmbeddingRow {
    pub memory_id: String,
    pub dim: u32,
    pub vector: Vec<f32>,
}

pub fn upsert(db: &Db, memory_id: &str, vector: &[f32]) -> DbResult<()> {
    let body = serde_json::to_string(vector)?;
    db.with(|conn| {
        conn.execute(
            "INSERT INTO memory_embeddings (memory_id, dim, vector_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(memory_id) DO UPDATE SET
                dim = excluded.dim,
                vector_json = excluded.vector_json,
                updated_at = excluded.updated_at",
            params![memory_id, vector.len() as i64, body, now_iso()],
        )?;
        Ok(())
    })
}

pub fn delete(db: &Db, memory_id: &str) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "DELETE FROM memory_embeddings WHERE memory_id = ?1",
            params![memory_id],
        )?;
        Ok(())
    })
}

pub fn get(db: &Db, memory_id: &str) -> DbResult<Option<EmbeddingRow>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT memory_id, dim, vector_json FROM memory_embeddings WHERE memory_id = ?1",
        )?;
        let r = stmt
            .query_row(params![memory_id], parse_row)
            .optional()?;
        Ok(r)
    })
}

pub fn list_all(db: &Db) -> DbResult<Vec<EmbeddingRow>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT memory_id, dim, vector_json FROM memory_embeddings",
        )?;
        let rows = stmt
            .query_map([], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn count(db: &Db) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 =
            conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))?;
        Ok(n)
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmbeddingRow> {
    let body: String = row.get(2)?;
    let vector: Vec<f32> = serde_json::from_str(&body).unwrap_or_default();
    Ok(EmbeddingRow {
        memory_id: row.get(0)?,
        dim: row.get::<_, i64>(1)? as u32,
        vector,
    })
}
