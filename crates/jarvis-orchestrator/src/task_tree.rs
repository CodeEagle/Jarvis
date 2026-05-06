//! TaskTree + TaskNode persistence and view construction. Section 8.4.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::task::{TaskNode, TaskStatus};
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeStatus {
    Running,
    Completed,
    Failed,
    Partial,
}

impl TreeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TreeStatus::Running => "running",
            TreeStatus::Completed => "completed",
            TreeStatus::Failed => "failed",
            TreeStatus::Partial => "partial",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "partial" => Self::Partial,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTreeRow {
    pub id: String,
    pub root_task_id: String,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub status: TreeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTreeView {
    pub completed_count: u32,
    pub failed_count: u32,
    pub pending_count: u32,
    pub running_count: u32,
    pub active_nodes: Vec<TaskNodeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNodeSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub result_summary: Option<String>,
    pub error_summary: Option<String>,
    pub depends_on: Vec<String>,
}

#[derive(Clone)]
pub struct TaskTreeStore {
    db: Db,
}

impl TaskTreeStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn create_tree(
        &self,
        root_task_id: &str,
        session_id: &str,
        trace_id: Option<&str>,
    ) -> DbResult<TaskTreeRow> {
        let id = new_id_with_prefix("tree");
        let n = now();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO task_trees
                    (id, root_task_id, session_id, trace_id, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![
                    id,
                    root_task_id,
                    session_id,
                    trace_id,
                    TreeStatus::Running.as_str(),
                    n.to_rfc3339(),
                ],
            )?;
            Ok(())
        })?;
        Ok(TaskTreeRow {
            id,
            root_task_id: root_task_id.to_string(),
            session_id: session_id.to_string(),
            trace_id: trace_id.map(str::to_string),
            status: TreeStatus::Running,
            created_at: n,
            updated_at: n,
        })
    }

    pub fn add_node(&self, tree_id: &str, node: &TaskNode) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO task_nodes
                    (id, tree_id, parent_id, task_id, title, intent, status,
                     assigned_agent_type, assigned_agent_id, depends_on_json,
                     result_summary, result_artifact_ids_json, error_summary,
                     created_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    node.id,
                    tree_id,
                    node.parent_id,
                    node.task_id,
                    node.title,
                    node.intent,
                    serialize_status(node.status),
                    node.assigned_agent_type,
                    node.assigned_agent_id,
                    serde_json::to_string(&node.depends_on).unwrap_or_else(|_| "[]".into()),
                    node.result_summary,
                    serde_json::to_string(&node.result_artifact_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    node.error_summary,
                    node.created_at.to_rfc3339(),
                    node.completed_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    pub fn update_status(
        &self,
        node_id: &str,
        status: TaskStatus,
        result_summary: Option<&str>,
        error_summary: Option<&str>,
    ) -> DbResult<()> {
        self.db.with(|conn| {
            let completed_at = match status {
                TaskStatus::Success | TaskStatus::Failed | TaskStatus::Skipped => {
                    Some(now().to_rfc3339())
                }
                _ => None,
            };
            conn.execute(
                "UPDATE task_nodes
                    SET status = ?1,
                        result_summary = COALESCE(?2, result_summary),
                        error_summary  = COALESCE(?3, error_summary),
                        completed_at   = COALESCE(?4, completed_at)
                  WHERE id = ?5",
                params![
                    serialize_status(status),
                    result_summary,
                    error_summary,
                    completed_at,
                    node_id,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_node(&self, node_id: &str) -> DbResult<Option<TaskNode>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(NODE_SELECT)?;
            let r = stmt
                .query_row(params![node_id], parse_node)
                .optional()?;
            Ok(r)
        })
    }

    pub fn list_nodes(&self, tree_id: &str) -> DbResult<Vec<TaskNode>> {
        self.db.with(|conn| {
            let sql = format!("{} WHERE tree_id = ?1 ORDER BY created_at ASC", NODE_SELECT_BY_TREE_PREFIX);
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params![tree_id], parse_node)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Build the on-the-fly view used to inject into the Orchestrator's
    /// context. (Section 8.4 TaskTreeView.)
    pub fn build_view(&self, tree_id: &str) -> DbResult<TaskTreeView> {
        let nodes = self.list_nodes(tree_id)?;
        let mut completed_count = 0;
        let mut failed_count = 0;
        let mut pending_count = 0;
        let mut running_count = 0;
        let mut active = Vec::new();
        let mut last_completed: Option<TaskNodeSummary> = None;

        for n in nodes {
            match n.status {
                TaskStatus::Success | TaskStatus::Skipped => {
                    completed_count += 1;
                    last_completed = Some(TaskNodeSummary {
                        id: n.id.clone(),
                        title: n.title.clone(),
                        status: serialize_status(n.status).into(),
                        result_summary: n.result_summary.clone(),
                        error_summary: n.error_summary.clone(),
                        depends_on: n.depends_on.clone(),
                    });
                }
                TaskStatus::Failed => {
                    failed_count += 1;
                    active.push(TaskNodeSummary {
                        id: n.id,
                        title: n.title,
                        status: serialize_status(n.status).into(),
                        result_summary: n.result_summary,
                        error_summary: n.error_summary,
                        depends_on: n.depends_on,
                    });
                }
                TaskStatus::Running => {
                    running_count += 1;
                    active.push(TaskNodeSummary {
                        id: n.id,
                        title: n.title,
                        status: serialize_status(n.status).into(),
                        result_summary: n.result_summary,
                        error_summary: n.error_summary,
                        depends_on: n.depends_on,
                    });
                }
                TaskStatus::Pending => {
                    pending_count += 1;
                    active.push(TaskNodeSummary {
                        id: n.id,
                        title: n.title,
                        status: serialize_status(n.status).into(),
                        result_summary: n.result_summary,
                        error_summary: n.error_summary,
                        depends_on: n.depends_on,
                    });
                }
            }
        }

        if let Some(last) = last_completed {
            active.insert(0, last);
        }

        Ok(TaskTreeView {
            completed_count,
            failed_count,
            pending_count,
            running_count,
            active_nodes: active,
        })
    }

    pub fn finalize_tree(&self, tree_id: &str, status: TreeStatus) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE task_trees SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now().to_rfc3339(), tree_id],
            )?;
            Ok(())
        })
    }
}

const NODE_SELECT_BY_TREE_PREFIX: &str = "SELECT id, parent_id, task_id, title, intent, status,
            assigned_agent_type, assigned_agent_id, depends_on_json,
            result_summary, result_artifact_ids_json, error_summary,
            created_at, completed_at
       FROM task_nodes";

const NODE_SELECT: &str = "SELECT id, parent_id, task_id, title, intent, status,
            assigned_agent_type, assigned_agent_id, depends_on_json,
            result_summary, result_artifact_ids_json, error_summary,
            created_at, completed_at
       FROM task_nodes
      WHERE id = ?1";

fn parse_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskNode> {
    let status_str: String = row.get(5)?;
    let status = match status_str.as_str() {
        "pending" => TaskStatus::Pending,
        "running" => TaskStatus::Running,
        "success" => TaskStatus::Success,
        "failed" => TaskStatus::Failed,
        "skipped" => TaskStatus::Skipped,
        _ => TaskStatus::Pending,
    };
    let depends_on_json: String = row.get(8)?;
    let artifact_ids_json: String = row.get(10)?;
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
    let completed_str: Option<String> = row.get(13)?;
    Ok(TaskNode {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        task_id: row.get(2)?,
        title: row.get(3)?,
        intent: row.get(4)?,
        status,
        assigned_agent_type: row.get(6)?,
        assigned_agent_id: row.get(7)?,
        depends_on: serde_json::from_str(&depends_on_json).unwrap_or_default(),
        result_summary: row.get(9)?,
        result_artifact_ids: serde_json::from_str(&artifact_ids_json).unwrap_or_default(),
        error_summary: row.get(11)?,
        created_at: parse_dt(row.get(12)?)?,
        completed_at: completed_str.map(parse_dt).transpose()?,
    })
}

fn serialize_status(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Success => "success",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
    }
}
