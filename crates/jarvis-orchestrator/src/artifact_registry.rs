//! Artifact Registry. Section 8.5.
//!
//! Subtasks register their outputs; the Orchestrator only sees `title +
//! summary + content_ref` so its context budget stays bounded.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_artifact_id;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    CodePatch,
    Analysis,
    SearchResult,
    Config,
    Note,
    Walkthrough,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::File => "file",
            ArtifactKind::CodePatch => "code_patch",
            ArtifactKind::Analysis => "analysis",
            ArtifactKind::SearchResult => "search_result",
            ArtifactKind::Config => "config",
            ArtifactKind::Note => "note",
            ArtifactKind::Walkthrough => "walkthrough",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "file" => Self::File,
            "code_patch" => Self::CodePatch,
            "analysis" => Self::Analysis,
            "search_result" => Self::SearchResult,
            "config" => Self::Config,
            "note" => Self::Note,
            "walkthrough" => Self::Walkthrough,
            _ => Self::Note,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub task_node_id: Option<String>,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub kind: ArtifactKind,
    pub title: String,
    pub summary: String,
    pub content_ref: String,
    pub size_bytes: Option<u64>,
    pub token_estimate: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSummary {
    pub id: String,
    pub kind: ArtifactKind,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ArtifactRegistry {
    db: Db,
}

impl ArtifactRegistry {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn register(&self, art: &Artifact) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO artifacts
                    (id, task_node_id, session_id, trace_id, type, title, summary,
                     content_ref, size_bytes, token_estimate, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    art.id,
                    art.task_node_id,
                    art.session_id,
                    art.trace_id,
                    art.kind.as_str(),
                    art.title,
                    art.summary,
                    art.content_ref,
                    art.size_bytes.map(|n| n as i64),
                    art.token_estimate,
                    art.created_at.to_rfc3339(),
                    art.expires_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    /// Build an artifact id from a freshly-registered draft.
    pub fn create(
        &self,
        session_id: &str,
        kind: ArtifactKind,
        title: &str,
        summary: &str,
        content_ref: &str,
        task_node_id: Option<&str>,
    ) -> DbResult<Artifact> {
        let art = Artifact {
            id: new_artifact_id(),
            task_node_id: task_node_id.map(str::to_string),
            session_id: session_id.to_string(),
            trace_id: None,
            kind,
            title: title.to_string(),
            summary: summary.to_string(),
            content_ref: content_ref.to_string(),
            size_bytes: None,
            token_estimate: None,
            created_at: now(),
            expires_at: None,
        };
        self.register(&art)?;
        Ok(art)
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Artifact>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, task_node_id, session_id, trace_id, type, title, summary,
                        content_ref, size_bytes, token_estimate, created_at, expires_at
                   FROM artifacts WHERE id = ?1",
            )?;
            let r = stmt.query_row(params![id], parse_artifact).optional()?;
            Ok(r)
        })
    }

    pub fn build_index(&self, session_id: &str) -> DbResult<Vec<ArtifactSummary>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, type, title, summary, created_at
                   FROM artifacts
                  WHERE session_id = ?1
                  ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map(params![session_id], |row| {
                    let ts_str: String = row.get(4)?;
                    let ts = DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let kind_str: String = row.get(1)?;
                    Ok(ArtifactSummary {
                        id: row.get(0)?,
                        kind: ArtifactKind::parse(&kind_str),
                        title: row.get(2)?,
                        summary: row.get(3)?,
                        created_at: ts,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}

fn parse_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    let kind_str: String = row.get(4)?;
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
    let expires_str: Option<String> = row.get(11)?;
    Ok(Artifact {
        id: row.get(0)?,
        task_node_id: row.get(1)?,
        session_id: row.get(2)?,
        trace_id: row.get(3)?,
        kind: ArtifactKind::parse(&kind_str),
        title: row.get(5)?,
        summary: row.get(6)?,
        content_ref: row.get(7)?,
        size_bytes: row.get::<_, Option<i64>>(8)?.map(|n| n as u64),
        token_estimate: row.get::<_, Option<i64>>(9)?.map(|n| n as u32),
        created_at: parse_dt(row.get(10)?)?,
        expires_at: expires_str.map(parse_dt).transpose()?,
    })
}
