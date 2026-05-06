//! Memory storage. Writes always pair with `memory_change_log` (Section 12.3).

use chrono::{DateTime, Utc};
use jarvis_core::memory::{
    EmotionPolarity, Memory, MemoryChangeType, MemoryStatus, MemoryType, SourceType,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::conn::Db;
use crate::error::{DbError, DbResult};
use crate::memory_change_log::{self, AppendChange};

pub struct WriteOpts<'a> {
    pub change_type: MemoryChangeType,
    pub source_module: Option<&'a str>,
    pub reason: Option<&'a str>,
}

/// Insert or update a memory and atomically record the change in
/// `memory_change_log`. The two writes happen inside a single transaction:
/// either both succeed or both roll back.
pub fn upsert(db: &Db, mem: &Memory, opts: WriteOpts<'_>) -> DbResult<()> {
    let mem = mem.clone().enforce_emotion_gate();
    db.with(|conn| {
        let tx = conn.unchecked_transaction()?;

        // Capture previous state, if any, so the change-log entry has a
        // before snapshot.
        let before = read_one(&tx, &mem.id)?;

        write_row(&tx, &mem)?;
        memory_change_log::append_with_conn(
            &tx,
            AppendChange {
                change_type: opts.change_type,
                memory_id: &mem.id,
                before: before.as_ref(),
                after: Some(&mem),
                source_module: opts.source_module,
                source_trace_id: mem.source_trace_id.as_deref(),
                source_agent_id: None,
                reason: opts.reason,
            },
        )?;

        tx.commit()?;
        Ok(())
    })
}

pub fn get(db: &Db, id: &str) -> DbResult<Option<Memory>> {
    db.with(|conn| read_one(conn, id))
}

pub fn list_by_scope(db: &Db, scope: &str, limit: usize) -> DbResult<Vec<Memory>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(SELECT_COLS_PREFIX)?;
        let rows = stmt
            .query_map([], parse_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter(|m| m.scope == scope && m.status == MemoryStatus::Approved)
            .take(limit)
            .collect())
    })
}

pub fn count(db: &Db) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        Ok(n)
    })
}

const SELECT_COLS_PREFIX: &str = "SELECT id, type, scope, content, entities_json,
        confidence, trust_score, half_life_days, retrieve_count,
        last_retrieved_at, source_trace_id, source_type,
        conflict_ids_json, status,
        emotion_energy, emotion_polarity, tier,
        expires_at, cluster_member_ids_json,
        created_at, updated_at
   FROM memories";

fn read_one(conn: &Connection, id: &str) -> DbResult<Option<Memory>> {
    let sql = format!("{} WHERE id = ?1", SELECT_COLS_PREFIX);
    let mut stmt = conn.prepare(&sql)?;
    let result = stmt.query_row(params![id], parse_row).optional()?;
    Ok(result)
}

fn write_row(conn: &Connection, mem: &Memory) -> DbResult<()> {
    conn.execute(
        "INSERT INTO memories
            (id, type, scope, content, entities_json,
             confidence, trust_score, half_life_days, retrieve_count,
             last_retrieved_at, source_trace_id, source_type,
             conflict_ids_json, status,
             emotion_energy, emotion_polarity, tier,
             expires_at, cluster_member_ids_json,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(id) DO UPDATE SET
            type = excluded.type,
            scope = excluded.scope,
            content = excluded.content,
            entities_json = excluded.entities_json,
            confidence = excluded.confidence,
            trust_score = excluded.trust_score,
            half_life_days = excluded.half_life_days,
            retrieve_count = excluded.retrieve_count,
            last_retrieved_at = excluded.last_retrieved_at,
            source_trace_id = excluded.source_trace_id,
            source_type = excluded.source_type,
            conflict_ids_json = excluded.conflict_ids_json,
            status = excluded.status,
            emotion_energy = excluded.emotion_energy,
            emotion_polarity = excluded.emotion_polarity,
            tier = excluded.tier,
            expires_at = excluded.expires_at,
            cluster_member_ids_json = excluded.cluster_member_ids_json,
            updated_at = excluded.updated_at",
        params![
            mem.id,
            mem.r#type.as_str(),
            mem.scope,
            mem.content,
            serde_json::to_string(&mem.entities)?,
            mem.confidence,
            mem.trust_score,
            mem.half_life_days,
            mem.retrieve_count,
            mem.last_retrieved_at.map(|t| t.to_rfc3339()),
            mem.source_trace_id,
            mem.source_type.as_str(),
            serde_json::to_string(&mem.conflict_ids)?,
            mem.status.as_str(),
            mem.emotion_energy,
            mem.emotion_polarity.as_str(),
            mem.tier,
            mem.expires_at.map(|t| t.to_rfc3339()),
            serde_json::to_string(&mem.cluster_member_ids)?,
            mem.created_at.to_rfc3339(),
            mem.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    let type_str: String = row.get(1)?;
    let r#type = MemoryType::parse(&type_str).unwrap_or(MemoryType::ScratchMemory);
    let entities_json: String = row.get(4)?;
    let conflict_ids_json: String = row.get(12)?;
    let status_str: String = row.get(13)?;
    let status = match status_str.as_str() {
        "approved" => MemoryStatus::Approved,
        "candidate" => MemoryStatus::Candidate,
        "deprecated" => MemoryStatus::Deprecated,
        "conflict" => MemoryStatus::Conflict,
        _ => MemoryStatus::Approved,
    };
    let polarity_str: String = row.get(15)?;
    let emotion_polarity = match polarity_str.as_str() {
        "positive" => EmotionPolarity::Positive,
        "negative" => EmotionPolarity::Negative,
        _ => EmotionPolarity::Neutral,
    };
    let source_type_str: String = row.get(11)?;
    let source_type = match source_type_str.as_str() {
        "user_explicit" => SourceType::UserExplicit,
        "inferred" => SourceType::Inferred,
        "correction" => SourceType::Correction,
        "task_result" => SourceType::TaskResult,
        "dream_cluster" => SourceType::DreamCluster,
        "dream_inference" => SourceType::DreamInference,
        _ => SourceType::Inferred,
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
    let parse_opt_dt = |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> {
        s.map(parse_dt).transpose()
    };
    let cluster_ids: Vec<String> =
        serde_json::from_str(&conflict_ids_json).unwrap_or_default();
    let cluster_member_ids_json: String = row.get(18)?;
    Ok(Memory {
        id: row.get(0)?,
        r#type,
        scope: row.get(2)?,
        content: row.get(3)?,
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        confidence: row.get(5)?,
        trust_score: row.get(6)?,
        half_life_days: row.get(7)?,
        retrieve_count: row.get::<_, i64>(8)? as u32,
        last_retrieved_at: parse_opt_dt(row.get(9)?)?,
        source_trace_id: row.get(10)?,
        source_type,
        conflict_ids: cluster_ids,
        status,
        emotion_energy: row.get(14)?,
        emotion_polarity,
        tier: row.get::<_, i64>(16)? as u8,
        expires_at: parse_opt_dt(row.get(17)?)?,
        cluster_member_ids: serde_json::from_str(&cluster_member_ids_json)
            .unwrap_or_default(),
        created_at: parse_dt(row.get(19)?)?,
        updated_at: parse_dt(row.get(20)?)?,
    })
}

#[allow(dead_code)]
pub(crate) fn _ensure_unused() -> DbError {
    DbError::NotFound("placeholder".into())
}
