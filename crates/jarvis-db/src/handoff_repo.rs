//! Handoff snapshot storage (PRD v1.9 §1.8).
//!
//! Inserts are append-only. Updates are only allowed while
//! `user_decision` is in `pending` or `deferred`; once it lands on
//! `accepted` / `declined`, the SQLite trigger `prevent_handoff_
//! finalised_update` blocks any further mutation. This mirrors the
//! immutability story the rest of §15.7 logs use.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::conn::Db;
use crate::error::DbResult;

/// One handoff record. The two `*_json` fields keep their wire shape
/// caller-side so the orchestrator crate owns the strongly-typed
/// HandoffSections / HandoffColdStart structs without forcing a
/// reverse dep here.
#[derive(Debug, Clone)]
pub struct HandoffRow {
    pub id: String,
    pub source_session_id: String,
    pub target_session_id: Option<String>,
    pub trace_id: Option<String>,
    pub sections_json: String,
    pub cold_start_payload_json: String,
    pub user_decision: String, // pending / accepted / declined / deferred
    pub advisory_level: String,
    pub benefit_score: f32,
    pub pressure_ratio: f32,
    pub generated_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// Insert a new pending row.
pub fn insert(db: &Db, row: &HandoffRow) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO handoff_snapshots
                (id, source_session_id, target_session_id, trace_id,
                 sections_json, cold_start_payload_json,
                 user_decision, advisory_level,
                 benefit_score, pressure_ratio,
                 generated_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                row.id,
                row.source_session_id,
                row.target_session_id,
                row.trace_id,
                row.sections_json,
                row.cold_start_payload_json,
                row.user_decision,
                row.advisory_level,
                row.benefit_score,
                row.pressure_ratio,
                row.generated_at.to_rfc3339(),
                row.decided_at.map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    })
}

pub fn get(db: &Db, id: &str) -> DbResult<Option<HandoffRow>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source_session_id, target_session_id, trace_id,
                    sections_json, cold_start_payload_json,
                    user_decision, advisory_level,
                    benefit_score, pressure_ratio,
                    generated_at, decided_at
               FROM handoff_snapshots WHERE id = ?1",
        )?;
        let r = stmt.query_row(params![id], parse_row).optional()?;
        Ok(r)
    })
}

pub fn list_for_session(
    db: &Db,
    source_session_id: &str,
    limit: usize,
) -> DbResult<Vec<HandoffRow>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source_session_id, target_session_id, trace_id,
                    sections_json, cold_start_payload_json,
                    user_decision, advisory_level,
                    benefit_score, pressure_ratio,
                    generated_at, decided_at
               FROM handoff_snapshots
              WHERE source_session_id = ?1
              ORDER BY generated_at DESC
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![source_session_id, limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn list_pending(db: &Db, limit: usize) -> DbResult<Vec<HandoffRow>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source_session_id, target_session_id, trace_id,
                    sections_json, cold_start_payload_json,
                    user_decision, advisory_level,
                    benefit_score, pressure_ratio,
                    generated_at, decided_at
               FROM handoff_snapshots
              WHERE user_decision IN ('pending', 'deferred')
              ORDER BY generated_at DESC
              LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Mark accepted, attach the new session id, and freeze. Trigger
/// blocks future updates.
pub fn accept(
    db: &Db,
    id: &str,
    target_session_id: &str,
) -> DbResult<()> {
    db.with(|conn| {
        let n = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE handoff_snapshots
                SET target_session_id = ?1,
                    user_decision = 'accepted',
                    decided_at = ?2
              WHERE id = ?3 AND user_decision IN ('pending', 'deferred')",
            params![target_session_id, n, id],
        )?;
        Ok(())
    })
}

pub fn decline(db: &Db, id: &str) -> DbResult<()> {
    db.with(|conn| {
        let n = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE handoff_snapshots
                SET user_decision = 'declined',
                    decided_at = ?1
              WHERE id = ?2 AND user_decision IN ('pending', 'deferred')",
            params![n, id],
        )?;
        Ok(())
    })
}

pub fn defer(db: &Db, id: &str) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "UPDATE handoff_snapshots
                SET user_decision = 'deferred'
              WHERE id = ?1 AND user_decision = 'pending'",
            params![id],
        )?;
        Ok(())
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HandoffRow> {
    let parse_dt = |s: String| -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let parse_opt_dt =
        |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> { s.map(parse_dt).transpose() };
    Ok(HandoffRow {
        id: row.get(0)?,
        source_session_id: row.get(1)?,
        target_session_id: row.get(2)?,
        trace_id: row.get(3)?,
        sections_json: row.get(4)?,
        cold_start_payload_json: row.get(5)?,
        user_decision: row.get(6)?,
        advisory_level: row.get(7)?,
        benefit_score: row.get::<_, f64>(8)? as f32,
        pressure_ratio: row.get::<_, f64>(9)? as f32,
        generated_at: parse_dt(row.get(10)?)?,
        decided_at: parse_opt_dt(row.get(11)?)?,
    })
}
