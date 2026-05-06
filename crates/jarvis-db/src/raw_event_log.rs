//! `raw_event_log`: the immutable, append-only record of everything
//! the system received and emitted.
//!
//! Section 15.7.2. Critical guarantees:
//!
//! 1. Append only — UPDATE / DELETE blocked by SQL triggers.
//! 2. Each row carries a sha256 checksum so external tampering can be
//!    detected during replay.
//! 3. Writes happen *before* any downstream processing (Router calls
//!    `append` before classifying intent — see Section 23.1).

use chrono::{DateTime, Utc};
use jarvis_core::time::now_iso;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::conn::Db;
use crate::error::{DbError, DbResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawEventKind {
    UserMessage,
    AgentMessage,
    SystemEvent,
    ToolCall,
    ToolResult,
    SteerSignal,
    Interrupt,
    OwnershipChange,
}

impl RawEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RawEventKind::UserMessage => "user_message",
            RawEventKind::AgentMessage => "agent_message",
            RawEventKind::SystemEvent => "system_event",
            RawEventKind::ToolCall => "tool_call",
            RawEventKind::ToolResult => "tool_result",
            RawEventKind::SteerSignal => "steer_signal",
            RawEventKind::Interrupt => "interrupt",
            RawEventKind::OwnershipChange => "ownership_change",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppendEvent<'a> {
    pub event_type: RawEventKind,
    pub session_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub raw_content: &'a str,
    pub safe_content: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub seq: i64,
    pub ts: DateTime<Utc>,
    pub event_type: String,
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
    pub agent_id: Option<String>,
    pub raw_content: String,
    pub safe_content: Option<String>,
    pub checksum: String,
}

impl RawEvent {
    /// Verify checksum still matches stored content. Returns `Ok(true)`
    /// when the row is intact, `Ok(false)` when content has been tampered
    /// with despite the immutability triggers (e.g. `UPDATE` issued via a
    /// raw connection that bypassed triggers).
    pub fn verify(&self) -> bool {
        compute_checksum(&self.ts.to_rfc3339(), &self.event_type, &self.raw_content)
            == self.checksum
    }
}

fn compute_checksum(ts: &str, event_type: &str, raw_content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ts.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(event_type.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(raw_content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Append a row. Returns the assigned `seq`.
pub fn append(db: &Db, ev: AppendEvent<'_>) -> DbResult<i64> {
    db.with(|conn| append_with_conn(conn, ev))
}

pub(crate) fn append_with_conn(conn: &Connection, ev: AppendEvent<'_>) -> DbResult<i64> {
    let ts = now_iso();
    let checksum = compute_checksum(&ts, ev.event_type.as_str(), ev.raw_content);
    // Auto-redact when callers don't supply safe_content. The raw
    // value still hits raw_content untouched — that's the spec.
    let derived: Option<String> = match ev.safe_content {
        Some(s) => Some(s.to_string()),
        None => {
            let (safe, hits) = crate::redactor::redact(ev.raw_content);
            if hits.is_empty() {
                None
            } else {
                Some(safe)
            }
        }
    };
    conn.execute(
        "INSERT INTO raw_event_log
            (ts, event_type, session_id, trace_id, agent_id,
             raw_content, safe_content, checksum)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            ts,
            ev.event_type.as_str(),
            ev.session_id,
            ev.trace_id,
            ev.agent_id,
            ev.raw_content,
            derived,
            checksum,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(db: &Db, seq: i64) -> DbResult<RawEvent> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, event_type, session_id, trace_id, agent_id,
                    raw_content, safe_content, checksum
               FROM raw_event_log WHERE seq = ?1",
        )?;
        let mut rows = stmt.query(params![seq])?;
        let row = rows
            .next()?
            .ok_or_else(|| DbError::NotFound(format!("raw_event_log seq={}", seq)))?;
        Ok(parse_row(row)?)
    })
}

pub fn list_for_session(db: &Db, session_id: &str, limit: usize) -> DbResult<Vec<RawEvent>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, event_type, session_id, trace_id, agent_id,
                    raw_content, safe_content, checksum
               FROM raw_event_log
              WHERE session_id = ?1
              ORDER BY seq ASC
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![session_id, limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn count(db: &Db) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 =
            conn.query_row("SELECT COUNT(*) FROM raw_event_log", [], |r| r.get(0))?;
        Ok(n)
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEvent> {
    let ts_str: String = row.get(1)?;
    let ts = chrono::DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(RawEvent {
        seq: row.get(0)?,
        ts,
        event_type: row.get(2)?,
        session_id: row.get(3)?,
        trace_id: row.get(4)?,
        agent_id: row.get(5)?,
        raw_content: row.get(6)?,
        safe_content: row.get(7)?,
        checksum: row.get(8)?,
    })
}
