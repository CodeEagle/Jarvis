//! Outbox for cross-device replication (Section 24.5).
//!
//! Producers `enqueue` mutations they want replicated; the
//! replication daemon `take_pending` to drain a batch, ships them
//! over the wire, and `mark_delivered` once the peer ACKs.
//! `delivered_at` is the only mutable column — everything else stays
//! append-only.

use chrono::{DateTime, Utc};
use jarvis_core::time::now_iso;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub seq: i64,
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub target_table: String,
    pub target_id: String,
    pub payload_json: String,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct EnqueueRequest<'a> {
    pub kind: &'a str,
    pub target_table: &'a str,
    pub target_id: &'a str,
    pub payload_json: &'a str,
}

pub fn enqueue(db: &Db, req: EnqueueRequest<'_>) -> DbResult<i64> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO outbox
                (ts, kind, target_table, target_id, payload_json, delivered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                now_iso(),
                req.kind,
                req.target_table,
                req.target_id,
                req.payload_json,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn take_pending(db: &Db, limit: usize) -> DbResult<Vec<OutboxEntry>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, kind, target_table, target_id, payload_json, delivered_at
               FROM outbox
              WHERE delivered_at IS NULL
              ORDER BY seq ASC
              LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn mark_delivered(db: &Db, seq: i64) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "UPDATE outbox SET delivered_at = ?1 WHERE seq = ?2",
            params![now_iso(), seq],
        )?;
        Ok(())
    })
}

pub fn pending_count(db: &Db) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE delivered_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n)
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxEntry> {
    let ts_str: String = row.get(1)?;
    let ts = DateTime::parse_from_rfc3339(&ts_str)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let delivered_str: Option<String> = row.get(6)?;
    let delivered_at = delivered_str
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
        })
        .transpose()?;
    Ok(OutboxEntry {
        seq: row.get(0)?,
        ts,
        kind: row.get(2)?,
        target_table: row.get(3)?,
        target_id: row.get(4)?,
        payload_json: row.get(5)?,
        delivered_at,
    })
}
