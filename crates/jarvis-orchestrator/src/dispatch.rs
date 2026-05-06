//! Sub-agent dispatch (Section 8.6 / 9.4).
//!
//! Defines the abstract `SubAgentDriver` trait so the Orchestrator can
//! dispatch a `SubTaskEnvelope` without caring whether the sub-agent
//! runs in-process, in a worker process, or against a long-running
//! app-server (e.g. Codex). v0.2 ships the in-process driver so we
//! can wire the orchestration loop end-to-end. Worker-process and
//! app-server drivers follow in v0.3.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use jarvis_core::ids::new_agent_id;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;

use crate::artifact_registry::ArtifactRegistry;
use crate::conversation_bus::{ConversationBus, SubChannelStatus};
use crate::sub_task::{Escalation, SubTaskEnvelope, SubTaskResult, SubTaskStatus};
use crate::task_tree::TaskTreeStore;

/// A heartbeat sink the dispatcher pings while a sub-agent is
/// executing. Concrete implementations come from `jarvis-control`'s
/// Watchdog (and are pluggable so this crate stays free of any
/// runtime concerns).
pub trait Heartbeat: Send + Sync {
    fn beat(&self, agent_id: &str);
    fn forget(&self, agent_id: &str);
}

/// No-op heartbeat — used by callers that don't care.
pub struct NoopHeartbeat;
impl Heartbeat for NoopHeartbeat {
    fn beat(&self, _agent_id: &str) {}
    fn forget(&self, _agent_id: &str) {}
}

/// Pluggable execution backend. Implementors return a SubTaskResult
/// per envelope. `Send` + `Sync` because the Orchestrator may dispatch
/// in parallel.
#[async_trait]
pub trait SubAgentDriver: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, envelope: SubTaskEnvelope) -> SubTaskResult;
}

pub type DriverHandle = Arc<dyn SubAgentDriver>;
pub type HeartbeatHandle = Arc<dyn Heartbeat>;

/// In-process closure-driven driver.
///
/// Useful for tests and for "trivial" sub-tasks where a worker process
/// is overkill. The closure receives the envelope and returns the
/// result — Orchestrator wraps the call with cancellation / timeout
/// logic later.
pub struct InProcessDriver<F>
where
    F: Fn(SubTaskEnvelope) -> SubTaskResult + Send + Sync + 'static,
{
    name: &'static str,
    handler: F,
}

impl<F> InProcessDriver<F>
where
    F: Fn(SubTaskEnvelope) -> SubTaskResult + Send + Sync + 'static,
{
    pub fn new(name: &'static str, handler: F) -> Self {
        Self { name, handler }
    }
}

#[async_trait]
impl<F> SubAgentDriver for InProcessDriver<F>
where
    F: Fn(SubTaskEnvelope) -> SubTaskResult + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, envelope: SubTaskEnvelope) -> SubTaskResult {
        (self.handler)(envelope)
    }
}

/// Build a SubTaskResult that signals catastrophic failure. Used by
/// the dispatcher when the driver panics or returns no result.
pub fn failed_result(sub_task_id: &str, reason: &str) -> SubTaskResult {
    SubTaskResult {
        sub_task_id: sub_task_id.to_string(),
        status: SubTaskStatus::Failed,
        summary: format!("driver failed: {reason}"),
        artifact_ids: vec![],
        escalation: Some(Escalation {
            reason: reason.to_string(),
            options: vec![],
        }),
        token_used: 0,
        tool_calls_count: 0,
        completed_at: Utc::now(),
    }
}

/// Glue layer: open a sub-channel, drive the agent, persist the
/// resulting status. Used by the higher-level orchestrator loop.
pub struct SubAgentDispatcher {
    bus: ConversationBus,
    tree: TaskTreeStore,
    artifacts: ArtifactRegistry,
}

impl SubAgentDispatcher {
    pub fn new(db: Db) -> Self {
        Self {
            bus: ConversationBus::new(db.clone()),
            tree: TaskTreeStore::new(db.clone()),
            artifacts: ArtifactRegistry::new(db),
        }
    }

    /// Run an envelope through `driver` and persist the lifecycle:
    /// 1. open SubChannel
    /// 2. mark TaskNode running (caller pre-creates the node)
    /// 3. driver.execute (with optional heartbeat sink)
    /// 4. mark SubChannel done + TaskNode success/failed
    pub async fn dispatch(
        &self,
        session_id: &str,
        node_id: &str,
        agent_type: &str,
        envelope: SubTaskEnvelope,
        driver: DriverHandle,
    ) -> DbResult<SubTaskResult> {
        self.dispatch_with_heartbeat(
            session_id,
            node_id,
            agent_type,
            envelope,
            driver,
            Arc::new(NoopHeartbeat),
        )
        .await
    }

    /// Same as `dispatch`, but pulses `heartbeat` every 5s while the
    /// driver is running. The Watchdog's stale → dead state machine
    /// can then identify wedged drivers.
    pub async fn dispatch_with_heartbeat(
        &self,
        session_id: &str,
        node_id: &str,
        agent_type: &str,
        envelope: SubTaskEnvelope,
        driver: DriverHandle,
        heartbeat: HeartbeatHandle,
    ) -> DbResult<SubTaskResult> {
        let channel = self.bus.open_sub_channel(
            session_id,
            &envelope.sub_task_id,
            agent_type,
            envelope.tentacle_path.as_deref(),
        )?;

        // Stamp running on the TaskNode so TaskTreeView reflects state.
        self.tree.update_status(
            node_id,
            jarvis_core::task::TaskStatus::Running,
            None,
            None,
        )?;

        // Drive — clone envelope id since we'll need it on failure.
        let sub_task_id = envelope.sub_task_id.clone();
        let agent_id = new_agent_id();
        heartbeat.beat(&agent_id);
        let beat_handle = {
            let hb = heartbeat.clone();
            let id = agent_id.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
                tick.set_missed_tick_behavior(
                    tokio::time::MissedTickBehavior::Delay,
                );
                loop {
                    tick.tick().await;
                    hb.beat(&id);
                }
            })
        };
        let result = driver.execute(envelope).await;
        beat_handle.abort();
        heartbeat.forget(&agent_id);

        // Update task tree based on outcome.
        let status = match result.status {
            SubTaskStatus::Success => jarvis_core::task::TaskStatus::Success,
            SubTaskStatus::Partial => jarvis_core::task::TaskStatus::Success,
            SubTaskStatus::Failed | SubTaskStatus::NeedClarification => {
                jarvis_core::task::TaskStatus::Failed
            }
        };
        self.tree.update_status(
            node_id,
            status,
            Some(&result.summary),
            result.escalation.as_ref().map(|e| e.reason.as_str()),
        )?;
        self.bus
            .update_sub_channel_status(&channel.id, SubChannelStatus::Done)?;
        let _ = (now(), self.artifacts.build_index(session_id)); // touch unused
        let _ = sub_task_id;
        Ok(result)
    }
}
