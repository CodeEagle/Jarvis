//! End-to-end orchestration pipeline.
//!
//! Glue layer that chains the sub-agent dispatch path together with
//! Walkthrough generation and the auto-approval flow described in
//! Sections 8.4 / 8.14. Concrete callers wire in:
//!
//! - a `SubAgentDriver` that knows how to actually run the agent;
//! - a `WalkthroughBuilder` (closure or trait) that synthesises the
//!   doc from the SubTaskResult.
//!
//! Verifier hooks are deliberately optional: a callsite may already
//! have run verification before the pipeline returns, or it may defer
//! to the daily Regression Orchestrator.

use std::sync::Arc;

use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::task::TaskNode;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;

use crate::dispatch::{DriverHandle, SubAgentDispatcher};
use crate::sub_task::{SubTaskEnvelope, SubTaskResult, SubTaskStatus};
use crate::task_tree::TaskTreeStore;
use crate::walkthrough::{
    AutoApprovalDecision, WalkthroughDoc, WalkthroughStore,
};

pub type WalkthroughBuilder = Arc<
    dyn Fn(&SubTaskResult) -> WalkthroughDoc + Send + Sync,
>;

#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    pub sub_task_result: SubTaskResult,
    pub walkthrough_doc_id: Option<String>,
    pub auto_decision: Option<AutoApprovalDecision>,
}

pub struct OrchestrationPipeline {
    db: Db,
    tree: TaskTreeStore,
    walkthrough: WalkthroughStore,
}

impl OrchestrationPipeline {
    pub fn new(db: Db) -> Self {
        Self {
            tree: TaskTreeStore::new(db.clone()),
            walkthrough: WalkthroughStore::new(db.clone()),
            db,
        }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Run a single sub-task end-to-end.
    /// 1. Create a TaskTree if `tree_id` is None, plus the TaskNode.
    /// 2. Dispatch through the supplied driver.
    /// 3. If success, ask the builder to mint a WalkthroughDoc and
    ///    auto-review it.
    pub async fn run_sub_task(
        &self,
        session_id: &str,
        agent_type: &str,
        envelope: SubTaskEnvelope,
        driver: DriverHandle,
        builder: Option<WalkthroughBuilder>,
    ) -> DbResult<PipelineOutcome> {
        let tree = self
            .tree
            .create_tree(&envelope.parent_task_id, session_id, Some(&envelope.trace_id))?;
        let node_id = new_id_with_prefix("node");
        self.tree.add_node(
            &tree.id,
            &TaskNode {
                id: node_id.clone(),
                parent_id: None,
                task_id: envelope.parent_task_id.clone(),
                title: envelope.title.clone(),
                intent: format!("{}.generate", agent_type),
                status: jarvis_core::task::TaskStatus::Pending,
                assigned_agent_type: agent_type.to_string(),
                assigned_agent_id: None,
                depends_on: vec![],
                result_summary: None,
                result_artifact_ids: vec![],
                error_summary: None,
                created_at: now(),
                completed_at: None,
            },
        )?;

        let dispatcher = SubAgentDispatcher::new(self.db.clone());
        let sub_task_id_for_log = envelope.sub_task_id.clone();
        let result = dispatcher
            .dispatch(session_id, &node_id, agent_type, envelope, driver)
            .await?;

        let mut outcome = PipelineOutcome {
            sub_task_result: result.clone(),
            walkthrough_doc_id: None,
            auto_decision: None,
        };

        // Walkthrough only when builder supplied AND outcome is non-failed.
        if matches!(
            result.status,
            SubTaskStatus::Success | SubTaskStatus::Partial
        ) {
            if let Some(b) = builder {
                let mut doc = b(&result);
                doc.sub_task_id = sub_task_id_for_log;
                doc.session_id = session_id.to_string();
                self.walkthrough.save(&doc)?;
                let decision = self.walkthrough.auto_review(&doc.id)?;
                outcome.walkthrough_doc_id = Some(doc.id);
                outcome.auto_decision = Some(decision);
            }
        }
        Ok(outcome)
    }
}
