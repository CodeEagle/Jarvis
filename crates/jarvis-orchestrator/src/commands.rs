//! commands.json (Section 8.17).
//!
//! Encodes the user-facing one-click command catalogue and provides the
//! state machine for executing the steps. v0.2 ships parsing + execution
//! state tracking; concrete handlers (sub-task dispatch, parallel
//! explore CompareAgent) wire in as those subsystems land.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCatalogue {
    pub commands: Vec<CommandDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub steps: Vec<CommandStep>,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub on_conflict_uncertain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStep {
    pub agent: String,
    pub instruction: String,
    #[serde(default)]
    pub inject_at: Option<String>,
}

impl CommandCatalogue {
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn get(&self, id: &str) -> Option<&CommandDefinition> {
        self.commands.iter().find(|c| c.id == id)
    }
}

/// Default catalogue mirroring Section 8.17.2.
pub fn defaults() -> CommandCatalogue {
    let json = r#"{
      "commands": [
        {"id":"sync_and_resolve","label":"拉主分支 + 解冲突","icon":"🔀",
         "description":"拉取主分支，解冲突",
         "steps":[
           {"agent":"devops","instruction":"git pull origin main"},
           {"agent":"coding","instruction":"分析冲突文件并解决"},
           {"agent":"verifier","instruction":"验证合并后代码可编译"}
         ],
         "on_conflict_uncertain":"ask_user"},
        {"id":"walkthrough_and_pr","label":"生成 Walkthrough + 创建 PR","icon":"📋",
         "steps":[
           {"agent":"walkthrough","instruction":"为本 session 所有产出生成 Walkthrough"},
           {"agent":"verifier","instruction":"验证所有 WalkthroughDoc"},
           {"agent":"devops","instruction":"创建 PR"}
         ],
         "require_approval":true},
        {"id":"regression_check","label":"发版前回归检查","icon":"🧪",
         "steps":[
           {"agent":"regression","instruction":"加载所有 approved WalkthroughDoc 并并行验证"},
           {"agent":"regression","instruction":"分析差异并分类"}
         ],
         "parallel":true},
        {"id":"unblock_agent","label":"助手卡住了","icon":"🆘",
         "steps":[
           {"agent":"current","instruction":"总结你当前的困境与可选项"}
         ]},
        {"id":"steer_current","label":"方向调整","icon":"🎯",
         "steps":[
           {"agent":"current","instruction":"steer","inject_at":"next_step"}
         ]},
        {"id":"parallel_explore","label":"并行探索","icon":"⚡",
         "config":{"parallel_count":3,
                   "compare_dimensions":["正确性","代码质量","测试覆盖率","设计简洁度"]}}
      ]
    }"#;
    CommandCatalogue::from_json(json).expect("default catalogue parses")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Running,
    Completed,
    Failed,
    WaitingUser,
}

impl CommandStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandStatus::Running => "running",
            CommandStatus::Completed => "completed",
            CommandStatus::Failed => "failed",
            CommandStatus::WaitingUser => "waiting_user",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "waiting_user" => Self::WaitingUser,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecution {
    pub id: String,
    pub session_id: String,
    pub command_id: String,
    pub command_label: String,
    pub status: CommandStatus,
    pub triggered_by: String,
    pub steps: Vec<StepProgress>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepProgress {
    pub index: u32,
    pub agent: String,
    pub instruction: String,
    pub status: StepStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

impl StepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepStatus::Pending => "pending",
            StepStatus::Running => "running",
            StepStatus::Done => "done",
            StepStatus::Failed => "failed",
            StepStatus::Skipped => "skipped",
        }
    }
}

#[derive(Clone)]
pub struct CommandRunner {
    db: Db,
    catalogue: CommandCatalogue,
}

impl CommandRunner {
    pub fn new(db: Db, catalogue: CommandCatalogue) -> Self {
        Self { db, catalogue }
    }

    pub fn catalogue(&self) -> &CommandCatalogue {
        &self.catalogue
    }

    /// Begin executing a command. Returns the CommandExecution row that
    /// the caller will mutate as steps progress.
    pub fn start(
        &self,
        session_id: &str,
        command_id: &str,
        triggered_by: &str,
    ) -> DbResult<CommandExecution> {
        let cmd = self
            .catalogue
            .get(command_id)
            .ok_or_else(|| {
                jarvis_db::error::DbError::NotFound(format!("command {command_id}"))
            })?
            .clone();
        let n = now();
        let exec = CommandExecution {
            id: new_id_with_prefix("cmd"),
            session_id: session_id.to_string(),
            command_id: cmd.id.clone(),
            command_label: cmd.label.clone(),
            status: CommandStatus::Running,
            triggered_by: triggered_by.to_string(),
            steps: cmd
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| StepProgress {
                    index: i as u32,
                    agent: s.agent.clone(),
                    instruction: s.instruction.clone(),
                    status: StepStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    note: None,
                })
                .collect(),
            started_at: n,
            completed_at: None,
        };
        if exec.steps.is_empty() {
            // Commands with no explicit steps (e.g. parallel_explore
            // driven entirely by `config`) still need a tracker row.
        }
        self.persist(&exec)?;
        Ok(exec)
    }

    pub fn record_step(
        &self,
        execution_id: &str,
        step_index: u32,
        status: StepStatus,
        note: Option<&str>,
    ) -> DbResult<CommandExecution> {
        let mut exec = self.get(execution_id)?.ok_or_else(|| {
            jarvis_db::error::DbError::NotFound(format!("execution {execution_id}"))
        })?;
        if let Some(step) = exec.steps.iter_mut().find(|s| s.index == step_index) {
            if matches!(status, StepStatus::Running) && step.started_at.is_none() {
                step.started_at = Some(now());
            }
            if matches!(status, StepStatus::Done | StepStatus::Failed | StepStatus::Skipped) {
                step.completed_at = Some(now());
            }
            step.status = status;
            if let Some(n) = note {
                step.note = Some(n.to_string());
            }
        }
        // Aggregate execution status.
        let any_failed = exec
            .steps
            .iter()
            .any(|s| matches!(s.status, StepStatus::Failed));
        let all_done_or_skipped = exec
            .steps
            .iter()
            .all(|s| matches!(s.status, StepStatus::Done | StepStatus::Skipped));
        if any_failed {
            exec.status = CommandStatus::Failed;
            exec.completed_at = Some(now());
        } else if !exec.steps.is_empty() && all_done_or_skipped {
            exec.status = CommandStatus::Completed;
            exec.completed_at = Some(now());
        }
        self.persist(&exec)?;
        Ok(exec)
    }

    pub fn mark_waiting_user(&self, execution_id: &str) -> DbResult<()> {
        let mut exec = self.get(execution_id)?.ok_or_else(|| {
            jarvis_db::error::DbError::NotFound(format!("execution {execution_id}"))
        })?;
        exec.status = CommandStatus::WaitingUser;
        self.persist(&exec)
    }

    pub fn get(&self, execution_id: &str) -> DbResult<Option<CommandExecution>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, command_id, command_label, status,
                        triggered_by, steps_json, started_at, completed_at
                   FROM command_executions
                  WHERE id = ?1",
            )?;
            let r = stmt.query_row(params![execution_id], parse_exec).optional()?;
            Ok(r)
        })
    }

    fn persist(&self, exec: &CommandExecution) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO command_executions
                    (id, session_id, command_id, command_label,
                     status, triggered_by, steps_json,
                     started_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    status = excluded.status,
                    steps_json = excluded.steps_json,
                    completed_at = excluded.completed_at",
                params![
                    exec.id,
                    exec.session_id,
                    exec.command_id,
                    exec.command_label,
                    exec.status.as_str(),
                    exec.triggered_by,
                    serde_json::to_string(&exec.steps).unwrap_or_else(|_| "[]".into()),
                    exec.started_at.to_rfc3339(),
                    exec.completed_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }
}

fn parse_exec(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandExecution> {
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
    let steps_json: String = row.get(6)?;
    let completed: Option<String> = row.get(8)?;
    Ok(CommandExecution {
        id: row.get(0)?,
        session_id: row.get(1)?,
        command_id: row.get(2)?,
        command_label: row.get(3)?,
        status: CommandStatus::parse(row.get::<_, String>(4)?.as_str()),
        triggered_by: row.get(5)?,
        steps: serde_json::from_str(&steps_json).unwrap_or_default(),
        started_at: parse_dt(row.get(7)?)?,
        completed_at: completed.map(parse_dt).transpose()?,
    })
}
