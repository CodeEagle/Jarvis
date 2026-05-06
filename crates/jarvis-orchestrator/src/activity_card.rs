//! ActivityCard storage (Section 8.13).
//!
//! Cards are the data layer for the collaboration panel. v0.2 ships the
//! storage + state transitions; the renderer that turns ActivityEvent +
//! AgentDefinition templates into card strings will land alongside the
//! UI work.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    Pending,
    Running,
    WaitingUser,
    Success,
    Failed,
    Suspended,
}

impl CardStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardStatus::Pending => "pending",
            CardStatus::Running => "running",
            CardStatus::WaitingUser => "waiting_user",
            CardStatus::Success => "success",
            CardStatus::Failed => "failed",
            CardStatus::Suspended => "suspended",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "waiting_user" => Self::WaitingUser,
            "success" => Self::Success,
            "failed" => Self::Failed,
            "suspended" => Self::Suspended,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityCard {
    pub id: String,
    pub session_id: String,
    pub sub_task_id: Option<String>,
    pub trace_id: Option<String>,
    pub agent_type: String,
    pub agent_display_name: String,
    pub agent_avatar_emoji: String,
    pub status: CardStatus,
    pub title: String,
    pub current_action: Option<String>,
    pub progress_text: Option<String>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub interaction_json: Option<String>,
    pub artifacts_json: Option<String>,
    pub is_expanded: bool,
    pub can_interrupt: bool,
    pub interrupt_cost: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CardDraft<'a> {
    pub session_id: &'a str,
    pub sub_task_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub agent_type: &'a str,
    pub agent_display_name: &'a str,
    pub agent_avatar_emoji: &'a str,
    pub title: &'a str,
}

#[derive(Clone)]
pub struct ActivityCardStore {
    db: Db,
}

impl ActivityCardStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn create(&self, draft: CardDraft<'_>) -> DbResult<ActivityCard> {
        let id = new_id_with_prefix("ac");
        let n = now();
        let card = ActivityCard {
            id: id.clone(),
            session_id: draft.session_id.to_string(),
            sub_task_id: draft.sub_task_id.map(str::to_string),
            trace_id: draft.trace_id.map(str::to_string),
            agent_type: draft.agent_type.to_string(),
            agent_display_name: draft.agent_display_name.to_string(),
            agent_avatar_emoji: draft.agent_avatar_emoji.to_string(),
            status: CardStatus::Pending,
            title: draft.title.to_string(),
            current_action: None,
            progress_text: None,
            result_summary: None,
            error_message: None,
            interaction_json: None,
            artifacts_json: None,
            is_expanded: true,
            can_interrupt: true,
            interrupt_cost: "low".to_string(),
            started_at: n,
            updated_at: n,
            completed_at: None,
        };
        self.upsert(&card)?;
        Ok(card)
    }

    pub fn upsert(&self, card: &ActivityCard) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO activity_cards
                    (id, session_id, sub_task_id, trace_id, agent_type,
                     agent_display_name, agent_avatar_emoji, status, title,
                     current_action, progress_text, result_summary,
                     error_message, interaction_json, artifacts_json,
                     is_expanded, can_interrupt, interrupt_cost,
                     started_at, updated_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                         ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(id) DO UPDATE SET
                    status = excluded.status,
                    current_action = excluded.current_action,
                    progress_text = excluded.progress_text,
                    result_summary = excluded.result_summary,
                    error_message = excluded.error_message,
                    interaction_json = excluded.interaction_json,
                    artifacts_json = excluded.artifacts_json,
                    is_expanded = excluded.is_expanded,
                    can_interrupt = excluded.can_interrupt,
                    interrupt_cost = excluded.interrupt_cost,
                    updated_at = excluded.updated_at,
                    completed_at = excluded.completed_at",
                params![
                    card.id,
                    card.session_id,
                    card.sub_task_id,
                    card.trace_id,
                    card.agent_type,
                    card.agent_display_name,
                    card.agent_avatar_emoji,
                    card.status.as_str(),
                    card.title,
                    card.current_action,
                    card.progress_text,
                    card.result_summary,
                    card.error_message,
                    card.interaction_json,
                    card.artifacts_json,
                    if card.is_expanded { 1 } else { 0 },
                    if card.can_interrupt { 1 } else { 0 },
                    card.interrupt_cost,
                    card.started_at.to_rfc3339(),
                    card.updated_at.to_rfc3339(),
                    card.completed_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<ActivityCard>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(SELECT_CARD)?;
            let r = stmt.query_row(params![id], parse_card).optional()?;
            Ok(r)
        })
    }

    pub fn update_status(
        &self,
        id: &str,
        status: CardStatus,
        progress_text: Option<&str>,
    ) -> DbResult<()> {
        let mut card = match self.get(id)? {
            Some(c) => c,
            None => {
                return Err(jarvis_db::error::DbError::NotFound(format!(
                    "activity card {id}"
                )))
            }
        };
        card.status = status;
        card.updated_at = now();
        if let Some(t) = progress_text {
            card.progress_text = Some(t.to_string());
        }
        if matches!(
            status,
            CardStatus::Success | CardStatus::Failed | CardStatus::Suspended
        ) {
            card.completed_at = Some(now());
            card.is_expanded = matches!(status, CardStatus::Failed);
        }
        if status == CardStatus::WaitingUser {
            card.is_expanded = true;
        }
        self.upsert(&card)
    }

    pub fn set_progress(&self, id: &str, action: &str, progress: &str) -> DbResult<()> {
        let mut card = match self.get(id)? {
            Some(c) => c,
            None => {
                return Err(jarvis_db::error::DbError::NotFound(format!(
                    "activity card {id}"
                )))
            }
        };
        card.current_action = Some(action.to_string());
        card.progress_text = Some(progress.to_string());
        card.status = CardStatus::Running;
        card.updated_at = now();
        self.upsert(&card)
    }

    pub fn list_for_session(&self, session_id: &str) -> DbResult<Vec<ActivityCard>> {
        self.db.with(|conn| {
            let sql = format!(
                "{} WHERE session_id = ?1 ORDER BY started_at DESC",
                SELECT_CARD_PREFIX
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params![session_id], parse_card)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}

const SELECT_CARD_PREFIX: &str = "SELECT id, session_id, sub_task_id, trace_id,
        agent_type, agent_display_name, agent_avatar_emoji,
        status, title, current_action, progress_text,
        result_summary, error_message, interaction_json, artifacts_json,
        is_expanded, can_interrupt, interrupt_cost,
        started_at, updated_at, completed_at
   FROM activity_cards";

const SELECT_CARD: &str = "SELECT id, session_id, sub_task_id, trace_id,
        agent_type, agent_display_name, agent_avatar_emoji,
        status, title, current_action, progress_text,
        result_summary, error_message, interaction_json, artifacts_json,
        is_expanded, can_interrupt, interrupt_cost,
        started_at, updated_at, completed_at
   FROM activity_cards WHERE id = ?1";

fn parse_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActivityCard> {
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
    let completed_str: Option<String> = row.get(20)?;
    Ok(ActivityCard {
        id: row.get(0)?,
        session_id: row.get(1)?,
        sub_task_id: row.get(2)?,
        trace_id: row.get(3)?,
        agent_type: row.get(4)?,
        agent_display_name: row.get(5)?,
        agent_avatar_emoji: row.get(6)?,
        status: CardStatus::parse(row.get::<_, String>(7)?.as_str()),
        title: row.get(8)?,
        current_action: row.get(9)?,
        progress_text: row.get(10)?,
        result_summary: row.get(11)?,
        error_message: row.get(12)?,
        interaction_json: row.get(13)?,
        artifacts_json: row.get(14)?,
        is_expanded: row.get::<_, i64>(15)? != 0,
        can_interrupt: row.get::<_, i64>(16)? != 0,
        interrupt_cost: row.get(17)?,
        started_at: parse_dt(row.get(18)?)?,
        updated_at: parse_dt(row.get(19)?)?,
        completed_at: completed_str.map(parse_dt).transpose()?,
    })
}
