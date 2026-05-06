//! Interrupt protocol (Section 8.11.6).
//!
//! Three flavours, all routed through `InterruptController`:
//!
//! - **Soft**: signal the sub-agent at its next checkpoint, give it
//!   `grace_period_ms` to settle, then escalate to hard if it ignores
//!   the signal. Section 30.2.3.
//! - **Hard**: force-close the sub-channel immediately. Already
//!   produced artefacts stay registered; the sub-task node is marked
//!   failed with `reason=user_cancelled`.
//! - **Async**: keep the sub-agent running but tag the channel so the
//!   conversation bus knows the user is no longer waiting on it.
//!
//! The controller doesn't kill processes itself — that's the driver's
//! job. We expose the *protocol* (state transitions + persistence) so
//! drivers can subscribe and act.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now_iso;
use jarvis_db::error::DbResult;
use jarvis_db::{raw_event_log, Db, RawEventKind};
use serde::{Deserialize, Serialize};

use crate::checkpoint::{CheckpointDraft, CheckpointStore, SubTaskCheckpoint};
use crate::conversation_bus::{ConversationBus, SubChannelStatus};
use crate::task_tree::TaskTreeStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptKind {
    Soft,
    Hard,
    Async,
}

impl InterruptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterruptKind::Soft => "soft",
            InterruptKind::Hard => "hard",
            InterruptKind::Async => "async",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptOutcome {
    /// Soft signal accepted; sub-agent expected to checkpoint within
    /// the grace period.
    SoftAcked,
    /// Soft signal expired without a checkpoint; controller escalated.
    EscalatedToHard,
    /// Hard interrupt applied. Channel closed.
    HardApplied,
    /// Async tag set; sub-agent continues to run.
    AsyncTagged,
}

#[derive(Debug, Clone, Copy)]
pub struct InterruptPolicy {
    pub soft_grace: Duration,
    /// Maximum time we leave a soft signal pending before escalating
    /// when the sub-agent never acknowledges.
    pub soft_escalation_timeout: Duration,
}

impl InterruptPolicy {
    pub const fn defaults() -> Self {
        Self {
            soft_grace: Duration::from_secs(10),
            soft_escalation_timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Debug)]
struct PendingSoft {
    sub_task_id: String,
    issued_at: Instant,
}

pub struct InterruptController {
    db: Db,
    bus: ConversationBus,
    tree: TaskTreeStore,
    checkpoints: CheckpointStore,
    policy: InterruptPolicy,
    pending: Mutex<Vec<PendingSoft>>,
}

impl InterruptController {
    pub fn new(db: Db) -> Self {
        Self {
            bus: ConversationBus::new(db.clone()),
            tree: TaskTreeStore::new(db.clone()),
            checkpoints: CheckpointStore::new(db.clone()),
            db,
            policy: InterruptPolicy::defaults(),
            pending: Mutex::new(Vec::new()),
        }
    }

    pub fn with_policy(db: Db, policy: InterruptPolicy) -> Self {
        Self {
            bus: ConversationBus::new(db.clone()),
            tree: TaskTreeStore::new(db.clone()),
            checkpoints: CheckpointStore::new(db.clone()),
            db,
            policy,
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Issue a soft interrupt. The driver should checkpoint and then
    /// call `acknowledge_soft`. If `evaluate_pending` ever observes an
    /// unacknowledged soft signal older than `soft_escalation_timeout`,
    /// it returns `EscalatedToHard` and applies the hard path.
    pub fn soft(
        &self,
        session_id: &str,
        sub_task_id: &str,
        node_id: Option<&str>,
    ) -> DbResult<InterruptOutcome> {
        self.audit(session_id, sub_task_id, InterruptKind::Soft);
        self.pending
            .lock()
            .expect("pending lock")
            .push(PendingSoft {
                sub_task_id: sub_task_id.to_string(),
                issued_at: Instant::now(),
            });
        if let Some(n) = node_id {
            // Mark node as Running with a hint that it's interrupting.
            self.tree.update_status(
                n,
                jarvis_core::task::TaskStatus::Running,
                None,
                Some("soft interrupt requested"),
            )?;
        }
        Ok(InterruptOutcome::SoftAcked)
    }

    /// Driver calls this once it has saved a checkpoint. The pending
    /// soft signal is cleared.
    pub fn acknowledge_soft(
        &self,
        sub_task_id: &str,
        checkpoint: CheckpointDraft<'_>,
    ) -> DbResult<SubTaskCheckpoint> {
        let saved = self.checkpoints.save(checkpoint)?;
        self.pending
            .lock()
            .expect("pending lock")
            .retain(|p| p.sub_task_id != sub_task_id);
        Ok(saved)
    }

    /// Apply a hard interrupt. Closes the sub-channel and marks the
    /// task node as failed. Returns the channel id when one matched.
    pub fn hard(
        &self,
        session_id: &str,
        sub_task_id: &str,
        node_id: Option<&str>,
    ) -> DbResult<InterruptOutcome> {
        self.audit(session_id, sub_task_id, InterruptKind::Hard);
        let active = self.bus.list_active_sub_channels(session_id)?;
        for ch in active {
            if ch.sub_task_id == sub_task_id {
                self.bus
                    .update_sub_channel_status(&ch.id, SubChannelStatus::Done)?;
            }
        }
        if let Some(n) = node_id {
            self.tree.update_status(
                n,
                jarvis_core::task::TaskStatus::Failed,
                None,
                Some("user_cancelled"),
            )?;
        }
        // Drop any pending soft for this sub_task.
        self.pending
            .lock()
            .expect("pending lock")
            .retain(|p| p.sub_task_id != sub_task_id);
        Ok(InterruptOutcome::HardApplied)
    }

    /// Tag a sub-channel as async — the user is no longer waiting on
    /// it but the sub-agent keeps running. Suspended channels become
    /// the sentinel state used by the conversation bus to know it can
    /// route incoming messages elsewhere.
    pub fn async_tag(
        &self,
        session_id: &str,
        sub_task_id: &str,
    ) -> DbResult<InterruptOutcome> {
        self.audit(session_id, sub_task_id, InterruptKind::Async);
        let active = self.bus.list_active_sub_channels(session_id)?;
        for ch in active {
            if ch.sub_task_id == sub_task_id {
                self.bus
                    .update_sub_channel_status(&ch.id, SubChannelStatus::Suspended)?;
            }
        }
        Ok(InterruptOutcome::AsyncTagged)
    }

    /// Walk pending soft signals and escalate the ones that have
    /// timed out. Returns the list of escalated `(sub_task_id,
    /// outcome)` pairs.
    pub fn evaluate_pending(
        &self,
        session_id: &str,
        node_lookup: impl Fn(&str) -> Option<String>,
    ) -> DbResult<Vec<(String, InterruptOutcome)>> {
        let now_inst = Instant::now();
        let mut to_escalate: Vec<String> = Vec::new();
        {
            let mut pending = self.pending.lock().expect("pending lock");
            pending.retain(|p| {
                if now_inst.duration_since(p.issued_at) >= self.policy.soft_escalation_timeout {
                    to_escalate.push(p.sub_task_id.clone());
                    false
                } else {
                    true
                }
            });
        }
        let mut out = Vec::new();
        for st in to_escalate {
            let node_id = node_lookup(&st);
            self.hard(session_id, &st, node_id.as_deref())?;
            out.push((st, InterruptOutcome::EscalatedToHard));
        }
        Ok(out)
    }

    fn audit(&self, session_id: &str, sub_task_id: &str, kind: InterruptKind) {
        let _ = raw_event_log::append(
            &self.db,
            raw_event_log::AppendEvent {
                event_type: RawEventKind::Interrupt,
                session_id: Some(session_id),
                trace_id: None,
                agent_id: None,
                raw_content: &format!(
                    "{{\"id\":\"{}\",\"sub_task_id\":\"{}\",\"kind\":\"{}\",\"ts\":\"{}\"}}",
                    new_id_with_prefix("intr"),
                    sub_task_id,
                    kind.as_str(),
                    now_iso(),
                ),
                safe_content: None,
            },
        );
    }
}
