//! Audit log (Section 15.5).
//!
//! Distinct from `raw_event_log`. Where the raw log captures *every*
//! input / output / tool call for forensic replay, the audit log
//! captures *significant actions* — file modifications (with
//! before/after sha256), tool authorisations, external calls, and
//! user-confirmation decisions. Both are immutable; both write at
//! Section 15.7-class durability.
//!
//! The redactor does **not** rewrite audit fields — by spec the audit
//! log gets a separate retention policy. Callers are expected to pass
//! pre-sanitised summaries into `data_json`.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now_iso;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    Success,
    Denied,
    Failed,
    Skipped,
    AwaitingConfirmation,
}

impl AuditStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "success",
            AuditStatus::Denied => "denied",
            AuditStatus::Failed => "failed",
            AuditStatus::Skipped => "skipped",
            AuditStatus::AwaitingConfirmation => "awaiting_confirmation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub trace_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub status: AuditStatus,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub data_json: String,
}

#[derive(Debug, Clone)]
pub struct AppendAudit<'a> {
    pub trace_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub actor: &'a str,
    pub action: &'a str,
    pub target: Option<&'a str>,
    pub status: AuditStatus,
    pub input_summary: Option<&'a str>,
    pub output_summary: Option<&'a str>,
    pub before_hash: Option<&'a str>,
    pub after_hash: Option<&'a str>,
    pub data_json: Option<&'a str>,
}

pub fn append(db: &Db, entry: AppendAudit<'_>) -> DbResult<String> {
    let id = new_id_with_prefix("aud");
    let ts = now_iso();
    db.with(|conn| {
        conn.execute(
            "INSERT INTO audit_log
                (id, ts, trace_id, session_id, task_id, actor, action,
                 target, status, input_summary, output_summary,
                 before_hash, after_hash, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                     ?12, ?13, ?14)",
            params![
                id,
                ts,
                entry.trace_id,
                entry.session_id,
                entry.task_id,
                entry.actor,
                entry.action,
                entry.target,
                entry.status.as_str(),
                entry.input_summary,
                entry.output_summary,
                entry.before_hash,
                entry.after_hash,
                entry.data_json.unwrap_or("{}"),
            ],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn get(db: &Db, id: &str) -> DbResult<Option<AuditEntry>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(SELECT_PREFIX_BY_ID)?;
        let r = stmt.query_row(params![id], parse_row).optional()?;
        Ok(r)
    })
}

pub fn list_for_session(db: &Db, session_id: &str, limit: usize) -> DbResult<Vec<AuditEntry>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, ts, trace_id, session_id, task_id, actor, action,
                    target, status, input_summary, output_summary,
                    before_hash, after_hash, data_json
               FROM audit_log
              WHERE session_id = ?1
              ORDER BY ts DESC
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![session_id, limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn list_for_actor(db: &Db, actor: &str, limit: usize) -> DbResult<Vec<AuditEntry>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, ts, trace_id, session_id, task_id, actor, action,
                    target, status, input_summary, output_summary,
                    before_hash, after_hash, data_json
               FROM audit_log
              WHERE actor = ?1
              ORDER BY ts DESC
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![actor, limit as i64], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

const SELECT_PREFIX_BY_ID: &str = "SELECT id, ts, trace_id, session_id, task_id, actor, action,
       target, status, input_summary, output_summary,
       before_hash, after_hash, data_json
  FROM audit_log WHERE id = ?1";

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    let ts_str: String = row.get(1)?;
    let ts = chrono::DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let status_str: String = row.get(8)?;
    Ok(AuditEntry {
        id: row.get(0)?,
        ts,
        trace_id: row.get(2)?,
        session_id: row.get(3)?,
        task_id: row.get(4)?,
        actor: row.get(5)?,
        action: row.get(6)?,
        target: row.get(7)?,
        status: match status_str.as_str() {
            "denied" => AuditStatus::Denied,
            "failed" => AuditStatus::Failed,
            "skipped" => AuditStatus::Skipped,
            "awaiting_confirmation" => AuditStatus::AwaitingConfirmation,
            _ => AuditStatus::Success,
        },
        input_summary: row.get(9)?,
        output_summary: row.get(10)?,
        before_hash: row.get(11)?,
        after_hash: row.get(12)?,
        data_json: row.get(13)?,
    })
}

/// Convenience helper for the most common pattern: a tool call audit
/// entry. Tool runtime calls this after every authorisation decision.
pub fn record_tool_call(
    db: &Db,
    session_id: Option<&str>,
    trace_id: Option<&str>,
    actor: &str,
    tool: &str,
    status: AuditStatus,
    summary: Option<&str>,
) -> DbResult<String> {
    append(
        db,
        AppendAudit {
            trace_id,
            session_id,
            task_id: None,
            actor,
            action: "tool.call",
            target: Some(tool),
            status,
            input_summary: None,
            output_summary: summary,
            before_hash: None,
            after_hash: None,
            data_json: None,
        },
    )
}
