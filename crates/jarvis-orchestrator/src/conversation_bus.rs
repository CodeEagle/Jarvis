//! ConversationBus + ownership state machine (Section 8.11).

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    Listening,
    Executing,
    WaitingUser,
}

impl InteractionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            InteractionMode::Listening => "listening",
            InteractionMode::Executing => "executing",
            InteractionMode::WaitingUser => "waiting_user",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "listening" => Self::Listening,
            "executing" => Self::Executing,
            "waiting_user" => Self::WaitingUser,
            _ => Self::Listening,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipRecord {
    pub id: String,
    pub session_id: String,
    pub agent_id: String,
    pub agent_type: String,
    pub sub_task_id: Option<String>,
    pub interaction_mode: InteractionMode,
    pub acquired_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubChannelStatus {
    Running,
    WaitingUser,
    Suspended,
    Done,
}

impl SubChannelStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubChannelStatus::Running => "running",
            SubChannelStatus::WaitingUser => "waiting_user",
            SubChannelStatus::Suspended => "suspended",
            SubChannelStatus::Done => "done",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "waiting_user" => Self::WaitingUser,
            "suspended" => Self::Suspended,
            "done" => Self::Done,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptCost {
    Free,
    Low,
    High,
}

impl InterruptCost {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterruptCost::Free => "free",
            InterruptCost::Low => "low",
            InterruptCost::High => "high",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubChannel {
    pub id: String,
    pub session_id: String,
    pub sub_task_id: String,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub status: SubChannelStatus,
    pub can_interrupt: bool,
    pub interrupt_cost: InterruptCost,
    pub tentacle_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ConversationBus {
    db: Db,
}

impl ConversationBus {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Acquire ownership for `agent_id`. Any prior active ownership in
    /// the same session is closed in the same transaction.
    pub fn acquire_ownership(
        &self,
        session_id: &str,
        agent_id: &str,
        agent_type: &str,
        sub_task_id: Option<&str>,
        mode: InteractionMode,
    ) -> DbResult<OwnershipRecord> {
        self.db.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            let now_iso = now().to_rfc3339();
            tx.execute(
                "UPDATE conversation_ownerships
                    SET released_at = ?1
                  WHERE session_id = ?2 AND released_at IS NULL",
                params![now_iso, session_id],
            )?;
            let id = new_id_with_prefix("own");
            tx.execute(
                "INSERT INTO conversation_ownerships
                    (id, session_id, agent_id, agent_type, sub_task_id,
                     interaction_mode, acquired_at, released_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![
                    id,
                    session_id,
                    agent_id,
                    agent_type,
                    sub_task_id,
                    mode.as_str(),
                    now_iso,
                ],
            )?;
            tx.commit()?;
            Ok(OwnershipRecord {
                id,
                session_id: session_id.to_string(),
                agent_id: agent_id.to_string(),
                agent_type: agent_type.to_string(),
                sub_task_id: sub_task_id.map(str::to_string),
                interaction_mode: mode,
                acquired_at: now(),
                released_at: None,
            })
        })
    }

    pub fn current_ownership(&self, session_id: &str) -> DbResult<Option<OwnershipRecord>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, agent_id, agent_type, sub_task_id,
                        interaction_mode, acquired_at, released_at
                   FROM conversation_ownerships
                  WHERE session_id = ?1 AND released_at IS NULL
                  ORDER BY acquired_at DESC
                  LIMIT 1",
            )?;
            let r = stmt
                .query_row(params![session_id], parse_ownership)
                .optional()?;
            Ok(r)
        })
    }

    pub fn release_ownership(&self, ownership_id: &str) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE conversation_ownerships
                    SET released_at = ?1
                  WHERE id = ?2 AND released_at IS NULL",
                params![now().to_rfc3339(), ownership_id],
            )?;
            Ok(())
        })
    }

    pub fn open_sub_channel(
        &self,
        session_id: &str,
        sub_task_id: &str,
        agent_type: &str,
        tentacle_path: Option<&str>,
    ) -> DbResult<SubChannel> {
        let id = new_id_with_prefix("subch");
        let n = now();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO sub_channels
                    (id, session_id, sub_task_id, agent_id, agent_type,
                     status, can_interrupt, interrupt_cost, tentacle_path,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5, 1, 'low', ?6, ?7, ?7)",
                params![
                    id,
                    session_id,
                    sub_task_id,
                    agent_type,
                    SubChannelStatus::Running.as_str(),
                    tentacle_path,
                    n.to_rfc3339(),
                ],
            )?;
            Ok(())
        })?;
        Ok(SubChannel {
            id,
            session_id: session_id.to_string(),
            sub_task_id: sub_task_id.to_string(),
            agent_id: None,
            agent_type: Some(agent_type.to_string()),
            status: SubChannelStatus::Running,
            can_interrupt: true,
            interrupt_cost: InterruptCost::Low,
            tentacle_path: tentacle_path.map(str::to_string),
            created_at: n,
            updated_at: n,
        })
    }

    pub fn update_sub_channel_status(
        &self,
        channel_id: &str,
        status: SubChannelStatus,
    ) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE sub_channels SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now().to_rfc3339(), channel_id],
            )?;
            Ok(())
        })
    }

    pub fn list_active_sub_channels(&self, session_id: &str) -> DbResult<Vec<SubChannel>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, sub_task_id, agent_id, agent_type,
                        status, can_interrupt, interrupt_cost, tentacle_path,
                        created_at, updated_at
                   FROM sub_channels
                  WHERE session_id = ?1 AND status != 'done'
                  ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map(params![session_id], parse_sub_channel)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Section 8.11.4 user-message router. Returns the routing decision
    /// without performing any work — the caller dispatches based on
    /// the result.
    pub fn classify_user_message(&self, content: &str) -> UserMessageRoute {
        let lower = content.to_lowercase();
        let interrupt_phrases = [
            "停一下", "先暂停", "取消", "stop", "cancel", "等等",
        ];
        if interrupt_phrases.iter().any(|p| lower.contains(p)) {
            return UserMessageRoute::Interrupt;
        }
        let progress_phrases =
            ["做到哪了", "进度", "还要多久", "在干什么", "status"];
        if progress_phrases.iter().any(|p| lower.contains(p)) {
            return UserMessageRoute::ProgressQuery;
        }
        let steer_phrases = [
            "记得", "注意", "顺便", "对了", "另外", "别忘了", "不要改", "保持",
        ];
        if steer_phrases.iter().any(|p| content.contains(p)) {
            return UserMessageRoute::Steer;
        }
        UserMessageRoute::Normal
    }
}

fn parse_ownership(row: &rusqlite::Row<'_>) -> rusqlite::Result<OwnershipRecord> {
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
    let mode_str: String = row.get(5)?;
    let released_str: Option<String> = row.get(7)?;
    Ok(OwnershipRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        agent_id: row.get(2)?,
        agent_type: row.get(3)?,
        sub_task_id: row.get(4)?,
        interaction_mode: InteractionMode::parse(&mode_str),
        acquired_at: parse_dt(row.get(6)?)?,
        released_at: released_str.map(parse_dt).transpose()?,
    })
}

fn parse_sub_channel(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubChannel> {
    let status_str: String = row.get(5)?;
    let interrupt_cost_str: String = row.get(7)?;
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
    Ok(SubChannel {
        id: row.get(0)?,
        session_id: row.get(1)?,
        sub_task_id: row.get(2)?,
        agent_id: row.get(3)?,
        agent_type: row.get(4)?,
        status: SubChannelStatus::parse(&status_str),
        can_interrupt: row.get::<_, i64>(6)? != 0,
        interrupt_cost: match interrupt_cost_str.as_str() {
            "free" => InterruptCost::Free,
            "high" => InterruptCost::High,
            _ => InterruptCost::Low,
        },
        tentacle_path: row.get(8)?,
        created_at: parse_dt(row.get(9)?)?,
        updated_at: parse_dt(row.get(10)?)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMessageRoute {
    Interrupt,
    Steer,
    ProgressQuery,
    Normal,
}
