//! GrowthEvent collector. Persists events into `growth_events` and
//! exposes typed query helpers.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now_iso;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::params;
use serde_json::Value;

use crate::artifact::{ArtifactStatus, ArtifactType, GrowthArtifact};
use crate::event::{GrowthEvent, SourceModule};

#[derive(Clone)]
pub struct Collector {
    db: Db,
}

impl Collector {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Record an event. Returns the assigned event id.
    pub fn emit(
        &self,
        source: SourceModule,
        event_type: &str,
        trace_id: Option<&str>,
        payload: Value,
    ) -> DbResult<String> {
        let id = new_id_with_prefix("ge");
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO growth_events
                    (id, ts, trace_id, source_module, event_type, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    now_iso(),
                    trace_id,
                    source.as_str(),
                    event_type,
                    payload.to_string(),
                ],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    /// Count events of a given type.
    pub fn count_of(&self, event_type: &str) -> DbResult<i64> {
        self.db.with(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM growth_events WHERE event_type = ?1",
                params![event_type],
                |r| r.get(0),
            )?;
            Ok(n)
        })
    }

    /// Persist a candidate artifact. If an id collision happens we
    /// upsert and bump the version.
    pub fn put_artifact(&self, art: &GrowthArtifact) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO growth_artifacts
                    (id, type, status, version, scope, confidence,
                     evidence_trace_ids, payload_json, created_at, promoted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                    status = excluded.status,
                    version = excluded.version,
                    scope = excluded.scope,
                    confidence = excluded.confidence,
                    evidence_trace_ids = excluded.evidence_trace_ids,
                    payload_json = excluded.payload_json,
                    promoted_at = excluded.promoted_at",
                params![
                    art.id,
                    art.r#type.as_str(),
                    art.status.as_str(),
                    art.version,
                    art.scope,
                    art.confidence,
                    serde_json::to_string(&art.evidence_trace_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    art.payload_json,
                    art.created_at.to_rfc3339(),
                    art.promoted_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_artifact(&self, id: &str) -> DbResult<Option<GrowthArtifact>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, type, status, version, scope, confidence,
                        evidence_trace_ids, payload_json, created_at, promoted_at
                   FROM growth_artifacts WHERE id = ?1",
            )?;
            let r = stmt
                .query_row(params![id], parse_artifact_row)
                .ok();
            Ok(r)
        })
    }

    pub fn list_artifacts(
        &self,
        type_filter: Option<ArtifactType>,
        status_filter: Option<ArtifactStatus>,
    ) -> DbResult<Vec<GrowthArtifact>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, type, status, version, scope, confidence,
                        evidence_trace_ids, payload_json, created_at, promoted_at
                   FROM growth_artifacts
                  ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map([], parse_artifact_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows
                .into_iter()
                .filter(|a| type_filter.is_none_or(|t| t == a.r#type))
                .filter(|a| status_filter.is_none_or(|s| s == a.status))
                .collect())
        })
    }

    pub fn list_events_for_event_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> DbResult<Vec<GrowthEvent>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, ts, trace_id, source_module, event_type, payload_json
                   FROM growth_events
                  WHERE event_type = ?1
                  ORDER BY ts DESC
                  LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![event_type, limit as i64], parse_event_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}

fn parse_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GrowthArtifact> {
    let type_str: String = row.get(1)?;
    let status_str: String = row.get(2)?;
    let evidence_str: String = row.get(6)?;
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
    let promoted_str: Option<String> = row.get(9)?;
    Ok(GrowthArtifact {
        id: row.get(0)?,
        r#type: ArtifactType::parse(&type_str).unwrap_or(ArtifactType::RoutingExample),
        status: match status_str.as_str() {
            "candidate" => ArtifactStatus::Candidate,
            "testing" => ArtifactStatus::Testing,
            "promoted" => ArtifactStatus::Promoted,
            "rejected" => ArtifactStatus::Rejected,
            "deprecated" => ArtifactStatus::Deprecated,
            _ => ArtifactStatus::Candidate,
        },
        version: row.get::<_, i64>(3)? as u32,
        scope: row.get(4)?,
        confidence: row.get(5)?,
        evidence_trace_ids: serde_json::from_str(&evidence_str).unwrap_or_default(),
        payload_json: row.get(7)?,
        created_at: parse_dt(row.get(8)?)?,
        promoted_at: promoted_str.map(parse_dt).transpose()?,
    })
}

fn parse_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GrowthEvent> {
    let ts_str: String = row.get(1)?;
    let ts = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let source_str: String = row.get(3)?;
    let source = match source_str.as_str() {
        "router" => SourceModule::Router,
        "session" => SourceModule::Session,
        "agent" => SourceModule::Agent,
        "tool" => SourceModule::Tool,
        "memory" => SourceModule::Memory,
        "runtime" => SourceModule::Runtime,
        "compressor" => SourceModule::Compressor,
        "ui" => SourceModule::Ui,
        _ => SourceModule::Runtime,
    };
    Ok(GrowthEvent {
        id: row.get(0)?,
        ts,
        trace_id: row.get(2)?,
        source_module: source,
        event_type: row.get(4)?,
        payload_json: row.get(5)?,
    })
}
