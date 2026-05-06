//! Sub-task checkpoints (Section 8.11.6).
//!
//! Soft-interrupts persist a snapshot here. On resume, the orchestrator
//! reloads the working set, completed steps, pinned findings, and
//! resume hint, then dispatches the sub-agent again.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskCheckpoint {
    pub id: String,
    pub sub_task_id: String,
    pub agent_id: Option<String>,
    pub completed_steps: Vec<String>,
    pub working_set_snapshot: String,
    pub pinned_findings: Vec<String>,
    pub artifact_ids_so_far: Vec<String>,
    pub resume_hint: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CheckpointDraft<'a> {
    pub sub_task_id: &'a str,
    pub agent_id: Option<&'a str>,
    pub completed_steps: &'a [String],
    pub working_set_snapshot: &'a str,
    pub pinned_findings: &'a [String],
    pub artifact_ids_so_far: &'a [String],
    pub resume_hint: &'a str,
}

#[derive(Clone)]
pub struct CheckpointStore {
    db: Db,
}

impl CheckpointStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn save(&self, draft: CheckpointDraft<'_>) -> DbResult<SubTaskCheckpoint> {
        let id = new_id_with_prefix("ckpt");
        let n = now();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO sub_task_checkpoints
                    (id, sub_task_id, agent_id, completed_steps_json,
                     working_set_snapshot, pinned_findings_json,
                     artifact_ids_so_far_json, resume_hint, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    draft.sub_task_id,
                    draft.agent_id,
                    serde_json::to_string(draft.completed_steps)
                        .unwrap_or_else(|_| "[]".into()),
                    draft.working_set_snapshot,
                    serde_json::to_string(draft.pinned_findings)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(draft.artifact_ids_so_far)
                        .unwrap_or_else(|_| "[]".into()),
                    draft.resume_hint,
                    n.to_rfc3339(),
                ],
            )?;
            Ok(())
        })?;
        Ok(SubTaskCheckpoint {
            id,
            sub_task_id: draft.sub_task_id.to_string(),
            agent_id: draft.agent_id.map(str::to_string),
            completed_steps: draft.completed_steps.to_vec(),
            working_set_snapshot: draft.working_set_snapshot.to_string(),
            pinned_findings: draft.pinned_findings.to_vec(),
            artifact_ids_so_far: draft.artifact_ids_so_far.to_vec(),
            resume_hint: draft.resume_hint.to_string(),
            created_at: n,
        })
    }

    pub fn latest(&self, sub_task_id: &str) -> DbResult<Option<SubTaskCheckpoint>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, sub_task_id, agent_id, completed_steps_json,
                        working_set_snapshot, pinned_findings_json,
                        artifact_ids_so_far_json, resume_hint, created_at
                   FROM sub_task_checkpoints
                  WHERE sub_task_id = ?1
                  ORDER BY created_at DESC
                  LIMIT 1",
            )?;
            let r = stmt
                .query_row(params![sub_task_id], parse_checkpoint)
                .optional()?;
            Ok(r)
        })
    }
}

fn parse_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubTaskCheckpoint> {
    let completed_json: String = row.get(3)?;
    let pinned_json: String = row.get(5)?;
    let artifact_json: String = row.get(6)?;
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
    Ok(SubTaskCheckpoint {
        id: row.get(0)?,
        sub_task_id: row.get(1)?,
        agent_id: row.get(2)?,
        completed_steps: serde_json::from_str(&completed_json).unwrap_or_default(),
        working_set_snapshot: row.get(4)?,
        pinned_findings: serde_json::from_str(&pinned_json).unwrap_or_default(),
        artifact_ids_so_far: serde_json::from_str(&artifact_json).unwrap_or_default(),
        resume_hint: row.get(7)?,
        created_at: parse_dt(row.get(8)?)?,
    })
}
