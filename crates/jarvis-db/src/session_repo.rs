use chrono::{DateTime, Utc};
use jarvis_core::session::{Message, MessageRole, Session, SessionStatus};
use jarvis_core::time::now;
use rusqlite::{params, OptionalExtension};

use crate::conn::Db;
use crate::error::{DbError, DbResult};

pub fn upsert_session(db: &Db, sess: &Session) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO sessions
                (id, title, domain, topic, summary, long_summary,
                 active_entities_json, resolved_json, unresolved_json,
                 status, created_at, updated_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                domain = excluded.domain,
                topic = excluded.topic,
                summary = excluded.summary,
                long_summary = excluded.long_summary,
                active_entities_json = excluded.active_entities_json,
                resolved_json = excluded.resolved_json,
                unresolved_json = excluded.unresolved_json,
                status = excluded.status,
                updated_at = excluded.updated_at,
                last_active_at = excluded.last_active_at",
            params![
                sess.id,
                sess.title,
                sess.domain,
                sess.topic,
                sess.summary,
                sess.long_summary,
                serde_json::to_string(&sess.active_entities)?,
                serde_json::to_string(&sess.resolved)?,
                serde_json::to_string(&sess.unresolved)?,
                sess.status.as_str(),
                sess.created_at.to_rfc3339(),
                sess.updated_at.to_rfc3339(),
                sess.last_active_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    })
}

pub fn get_session(db: &Db, id: &str) -> DbResult<Option<Session>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, domain, topic, summary, long_summary,
                    active_entities_json, resolved_json, unresolved_json,
                    status, created_at, updated_at, last_active_at
               FROM sessions WHERE id = ?1",
        )?;
        let result = stmt
            .query_row(params![id], parse_session)
            .optional()?;
        Ok(result)
    })
}

pub fn list_recent(db: &Db, limit: usize) -> DbResult<Vec<Session>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, domain, topic, summary, long_summary,
                    active_entities_json, resolved_json, unresolved_json,
                    status, created_at, updated_at, last_active_at
               FROM sessions
              WHERE status = 'active'
              ORDER BY last_active_at DESC
              LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], parse_session)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn parse_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let active_entities_json: String = row.get(6)?;
    let resolved_json: String = row.get(7)?;
    let unresolved_json: String = row.get(8)?;
    let status_str: String = row.get(9)?;
    let status = match status_str.as_str() {
        "active" => SessionStatus::Active,
        "paused" => SessionStatus::Paused,
        "archived" => SessionStatus::Archived,
        _ => SessionStatus::Active,
    };
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
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        domain: row.get(2)?,
        topic: row.get(3)?,
        summary: row.get(4)?,
        long_summary: row.get(5)?,
        active_entities: serde_json::from_str(&active_entities_json).unwrap_or_default(),
        resolved: serde_json::from_str(&resolved_json).unwrap_or_default(),
        unresolved: serde_json::from_str(&unresolved_json).unwrap_or_default(),
        recent_message_ids: vec![],
        memory_refs: vec![],
        skill_refs: vec![],
        status,
        created_at: parse_dt(row.get(10)?)?,
        updated_at: parse_dt(row.get(11)?)?,
        last_active_at: parse_dt(row.get(12)?)?,
    })
}

// ── messages ────────────────────────────────────────────────────────────

pub fn append_message(db: &Db, msg: &Message) -> DbResult<()> {
    db.with(|conn| {
        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        };
        conn.execute(
            "INSERT INTO messages
                (id, session_id, trace_id, role, content,
                 token_count, summary_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                msg.id,
                msg.session_id,
                msg.trace_id,
                role,
                msg.content,
                msg.token_count,
                msg.summary_id,
                msg.created_at.to_rfc3339(),
            ],
        )?;
        // touch session.last_active_at
        conn.execute(
            "UPDATE sessions SET last_active_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now().to_rfc3339(), msg.session_id],
        )?;
        Ok(())
    })
}

pub fn recent_messages(
    db: &Db,
    session_id: &str,
    limit: usize,
) -> DbResult<Vec<Message>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, trace_id, role, content,
                    token_count, summary_id, created_at
               FROM messages
              WHERE session_id = ?1
              ORDER BY created_at DESC
              LIMIT ?2",
        )?;
        let mut rows = stmt
            .query_map(params![session_id, limit as i64], parse_message)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.reverse(); // chronological
        Ok(rows)
    })
}

fn parse_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let role_str: String = row.get(3)?;
    let role = match role_str.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "system" => MessageRole::System,
        "tool" => MessageRole::Tool,
        _ => MessageRole::User,
    };
    let created_at_str: String = row.get(7)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        trace_id: row.get(2)?,
        role,
        content: row.get(4)?,
        token_count: row.get(5)?,
        summary_id: row.get(6)?,
        created_at,
    })
}

#[allow(dead_code)]
pub(crate) fn _ensure_unused() -> DbError {
    DbError::NotFound("placeholder".into())
}
