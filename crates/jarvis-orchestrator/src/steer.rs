//! Steer protocol (Section 9.11).
//!
//! Two responsibilities:
//!
//! 1. Frequency throttle — at most 3 steers per sub-task per 60 seconds.
//! 2. Persistence — every accepted steer is persisted to `steer_signals`
//!    plus the immutable `raw_event_log`, so the audit trail in
//!    Section 30.5 / 30.13 always lines up.

use chrono::{DateTime, Duration, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::{raw_event_log, Db, RawEventKind};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteerScope {
    Direction,
    Constraint,
    Priority,
}

impl SteerScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            SteerScope::Direction => "direction",
            SteerScope::Constraint => "constraint",
            SteerScope::Priority => "priority",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectAt {
    NextStep,
    NextToolCall,
    NextCheckpoint,
}

impl InjectAt {
    pub fn as_str(&self) -> &'static str {
        match self {
            InjectAt::NextStep => "next_step",
            InjectAt::NextToolCall => "next_tool_call",
            InjectAt::NextCheckpoint => "next_checkpoint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteerStatus {
    Pending,
    Injected,
    Acknowledged,
    Expired,
    Rejected,
}

impl SteerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SteerStatus::Pending => "pending",
            SteerStatus::Injected => "injected",
            SteerStatus::Acknowledged => "acknowledged",
            SteerStatus::Expired => "expired",
            SteerStatus::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "injected" => Self::Injected,
            "acknowledged" => Self::Acknowledged,
            "expired" => Self::Expired,
            "rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteerSignal {
    pub id: String,
    pub session_id: String,
    pub sub_task_id: String,
    pub trace_id: Option<String>,
    pub content: String,
    pub scope: SteerScope,
    pub inject_at: InjectAt,
    pub status: SteerStatus,
    pub created_at: DateTime<Utc>,
    pub injected_at: Option<DateTime<Utc>>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub enum SteerEnqueueOutcome {
    Accepted(SteerSignal),
    /// Section 9.11.4: throttle.
    RateLimited { recent_count: u32 },
}

#[derive(Debug, Clone, Copy)]
pub struct SteerPolicy {
    pub max_per_window: u32,
    pub window: Duration,
}

impl SteerPolicy {
    pub fn defaults() -> Self {
        Self {
            max_per_window: 3,
            window: Duration::seconds(60),
        }
    }
}

#[derive(Clone)]
pub struct SteerController {
    db: Db,
    policy: SteerPolicy,
}

pub struct EnqueueRequest<'a> {
    pub session_id: &'a str,
    pub sub_task_id: &'a str,
    pub trace_id: Option<&'a str>,
    pub content: &'a str,
    pub scope: SteerScope,
    pub inject_at: InjectAt,
}

impl SteerController {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            policy: SteerPolicy::defaults(),
        }
    }

    pub fn with_policy(db: Db, policy: SteerPolicy) -> Self {
        Self { db, policy }
    }

    pub fn enqueue(&self, req: EnqueueRequest<'_>) -> DbResult<SteerEnqueueOutcome> {
        // Throttle check.
        let window_start = (Utc::now() - self.policy.window).to_rfc3339();
        let recent: u32 = self.db.with(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM steer_signals
                  WHERE sub_task_id = ?1 AND created_at >= ?2",
                params![req.sub_task_id, window_start],
                |r| r.get(0),
            )?;
            Ok(n as u32)
        })?;
        if recent >= self.policy.max_per_window {
            return Ok(SteerEnqueueOutcome::RateLimited {
                recent_count: recent,
            });
        }

        let id = new_id_with_prefix("steer");
        let n = now();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO steer_signals
                    (id, session_id, sub_task_id, trace_id,
                     content, scope, inject_at, status,
                     created_at, injected_at, acknowledged_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL)",
                params![
                    id,
                    req.session_id,
                    req.sub_task_id,
                    req.trace_id,
                    req.content,
                    req.scope.as_str(),
                    req.inject_at.as_str(),
                    SteerStatus::Pending.as_str(),
                    n.to_rfc3339(),
                ],
            )?;
            Ok(())
        })?;

        // Audit to raw_event_log so Section 30.5 audit query works.
        let _ = raw_event_log::append(
            &self.db,
            raw_event_log::AppendEvent {
                event_type: RawEventKind::SteerSignal,
                session_id: Some(req.session_id),
                trace_id: req.trace_id,
                agent_id: None,
                raw_content: &serde_json::json!({
                    "id": id,
                    "sub_task_id": req.sub_task_id,
                    "content": req.content,
                    "scope": req.scope.as_str(),
                    "inject_at": req.inject_at.as_str(),
                })
                .to_string(),
                safe_content: None,
            },
        );

        Ok(SteerEnqueueOutcome::Accepted(SteerSignal {
            id,
            session_id: req.session_id.to_string(),
            sub_task_id: req.sub_task_id.to_string(),
            trace_id: req.trace_id.map(str::to_string),
            content: req.content.to_string(),
            scope: req.scope,
            inject_at: req.inject_at,
            status: SteerStatus::Pending,
            created_at: n,
            injected_at: None,
            acknowledged_at: None,
        }))
    }

    pub fn next_pending(
        &self,
        sub_task_id: &str,
        inject_at: InjectAt,
    ) -> DbResult<Option<SteerSignal>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, sub_task_id, trace_id, content,
                        scope, inject_at, status,
                        created_at, injected_at, acknowledged_at
                   FROM steer_signals
                  WHERE sub_task_id = ?1
                    AND status = 'pending'
                    AND inject_at = ?2
                  ORDER BY created_at ASC
                  LIMIT 1",
            )?;
            let r = stmt
                .query_row(params![sub_task_id, inject_at.as_str()], parse_steer)
                .optional()?;
            Ok(r)
        })
    }

    pub fn mark_injected(&self, id: &str) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE steer_signals
                    SET status = 'injected', injected_at = ?1
                  WHERE id = ?2 AND status = 'pending'",
                params![now().to_rfc3339(), id],
            )?;
            Ok(())
        })
    }

    pub fn mark_acknowledged(&self, id: &str) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE steer_signals
                    SET status = 'acknowledged', acknowledged_at = ?1
                  WHERE id = ?2 AND status = 'injected'",
                params![now().to_rfc3339(), id],
            )?;
            Ok(())
        })
    }

    pub fn count_recent(&self, sub_task_id: &str) -> DbResult<u32> {
        let window_start = (Utc::now() - self.policy.window).to_rfc3339();
        self.db.with(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM steer_signals
                  WHERE sub_task_id = ?1 AND created_at >= ?2",
                params![sub_task_id, window_start],
                |r| r.get(0),
            )?;
            Ok(n as u32)
        })
    }
}

fn parse_steer(row: &rusqlite::Row<'_>) -> rusqlite::Result<SteerSignal> {
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
    let scope_str: String = row.get(5)?;
    let inject_str: String = row.get(6)?;
    let status_str: String = row.get(7)?;
    let injected_str: Option<String> = row.get(9)?;
    let ack_str: Option<String> = row.get(10)?;
    Ok(SteerSignal {
        id: row.get(0)?,
        session_id: row.get(1)?,
        sub_task_id: row.get(2)?,
        trace_id: row.get(3)?,
        content: row.get(4)?,
        scope: match scope_str.as_str() {
            "direction" => SteerScope::Direction,
            "priority" => SteerScope::Priority,
            _ => SteerScope::Constraint,
        },
        inject_at: match inject_str.as_str() {
            "next_tool_call" => InjectAt::NextToolCall,
            "next_checkpoint" => InjectAt::NextCheckpoint,
            _ => InjectAt::NextStep,
        },
        status: SteerStatus::parse(&status_str),
        created_at: parse_dt(row.get(8)?)?,
        injected_at: injected_str.map(parse_dt).transpose()?,
        acknowledged_at: ack_str.map(parse_dt).transpose()?,
    })
}
