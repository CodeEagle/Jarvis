use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now_iso;
use rusqlite::params;

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone)]
pub struct MentionLogEntry<'a> {
    pub session_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub raw_text: &'a str,
    pub resolved_type: Option<&'a str>,
    pub unresolved: bool,
    pub mention_mode: &'a str,
}

pub fn append(db: &Db, e: MentionLogEntry<'_>) -> DbResult<()> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO mention_log
                (id, session_id, trace_id, raw_text, resolved_type,
                 unresolved, mention_mode, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new_id_with_prefix("ment"),
                e.session_id.unwrap_or(""),
                e.trace_id,
                e.raw_text,
                e.resolved_type,
                if e.unresolved { 1 } else { 0 },
                e.mention_mode,
                now_iso(),
            ],
        )?;
        Ok(())
    })
}

pub fn count_unresolved(db: &Db) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mention_log WHERE unresolved = 1",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    })
}
