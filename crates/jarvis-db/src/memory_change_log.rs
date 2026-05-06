//! Section 15.7.3.

use chrono::{DateTime, Utc};
use jarvis_core::memory::{Memory, MemoryChangeType};
use jarvis_core::time::now_iso;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChangeEntry {
    pub seq: i64,
    pub ts: DateTime<Utc>,
    pub memory_id: String,
    pub change_type: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub source_module: Option<String>,
    pub source_trace_id: Option<String>,
    pub source_agent_id: Option<String>,
    pub reason: Option<String>,
}

pub struct AppendChange<'a> {
    pub change_type: MemoryChangeType,
    pub memory_id: &'a str,
    pub before: Option<&'a Memory>,
    pub after: Option<&'a Memory>,
    pub source_module: Option<&'a str>,
    pub source_trace_id: Option<&'a str>,
    pub source_agent_id: Option<&'a str>,
    pub reason: Option<&'a str>,
}

pub fn append(db: &Db, change: AppendChange<'_>) -> DbResult<i64> {
    db.with(|conn| append_with_conn(conn, change))
}

pub(crate) fn append_with_conn(
    conn: &Connection,
    change: AppendChange<'_>,
) -> DbResult<i64> {
    let before_json = change
        .before
        .map(serde_json::to_string)
        .transpose()?;
    let after_json = change
        .after
        .map(serde_json::to_string)
        .transpose()?;
    conn.execute(
        "INSERT INTO memory_change_log
           (ts, memory_id, change_type, before_json, after_json,
            source_module, source_trace_id, source_agent_id, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            now_iso(),
            change.memory_id,
            change.change_type.as_str(),
            before_json,
            after_json,
            change.source_module,
            change.source_trace_id,
            change.source_agent_id,
            change.reason,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn history_for(db: &Db, memory_id: &str) -> DbResult<Vec<MemoryChangeEntry>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, memory_id, change_type,
                    before_json, after_json,
                    source_module, source_trace_id, source_agent_id, reason
               FROM memory_change_log
              WHERE memory_id = ?1
              ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![memory_id], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryChangeEntry> {
    let ts_str: String = row.get(1)?;
    let ts = chrono::DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(MemoryChangeEntry {
        seq: row.get(0)?,
        ts,
        memory_id: row.get(2)?,
        change_type: row.get(3)?,
        before_json: row.get(4)?,
        after_json: row.get(5)?,
        source_module: row.get(6)?,
        source_trace_id: row.get(7)?,
        source_agent_id: row.get(8)?,
        reason: row.get(9)?,
    })
}
