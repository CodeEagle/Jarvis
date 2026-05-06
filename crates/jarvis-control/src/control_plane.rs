//! Control Plane. Section 9.10.2.
//!
//! Receives every user input, hands it to the Task Plane (the Router for
//! v0.1), and guarantees a response within `fallback_ack`. If the Task
//! Plane runs over budget, the Control Plane returns a fallback message
//! immediately and lets the real result settle in the background.

use std::sync::Arc;
use std::time::Instant;

use jarvis_core::route::RouteDecision;
use jarvis_db::Db;
use jarvis_router::{Router, RouterDiagnostics, RouterInput};
use tokio::time::timeout;

use crate::fallback::{fallback_message, TaskPlaneState};
use crate::sla::{ResponseKind, ResponseSla};

#[derive(Debug, Clone)]
pub enum HandledResponse {
    /// The Task Plane returned within budget.
    Resolved {
        decision: Box<RouteDecision>,
        diagnostics_summary: String,
        kind: ResponseKind,
        elapsed_ms: u128,
    },
    /// We hit the SLA before the Task Plane finished. The user gets the
    /// fallback ack now; the work continues in the background.
    Fallback {
        message: String,
        elapsed_ms: u128,
    },
}

pub struct ControlPlane {
    router: Arc<Router>,
    sla: ResponseSla,
}

impl ControlPlane {
    pub fn new(db: Db) -> Self {
        Self {
            router: Arc::new(Router::new(db)),
            sla: ResponseSla::defaults(),
        }
    }

    pub fn with_sla(db: Db, sla: ResponseSla) -> Self {
        Self {
            router: Arc::new(Router::new(db)),
            sla,
        }
    }

    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    /// Handle a user input under the SLA budget.
    pub async fn handle_user_input(
        &self,
        user_input: String,
        session_id_hint: Option<String>,
        running_agent_types: Vec<String>,
    ) -> HandledResponse {
        let started = Instant::now();
        let router = self.router.clone();
        let budget = self.sla.fallback_ack;

        // Run the Router on a blocking thread (rusqlite is sync).
        let task = tokio::task::spawn_blocking(move || {
            router.route(RouterInput {
                user_input: &user_input,
                session_id_hint: session_id_hint.as_deref(),
                running_agent_types: &running_agent_types,
            })
        });

        match timeout(budget, task).await {
            Ok(Ok(Ok((decision, diag)))) => HandledResponse::Resolved {
                kind: classify(&decision),
                decision: Box::new(decision),
                diagnostics_summary: summarize(&diag),
                elapsed_ms: started.elapsed().as_millis(),
            },
            Ok(Ok(Err(e))) => HandledResponse::Fallback {
                message: format!(
                    "{} (router error: {})",
                    fallback_message(TaskPlaneState::Stuck, None),
                    e
                ),
                elapsed_ms: started.elapsed().as_millis(),
            },
            Ok(Err(join_err)) => HandledResponse::Fallback {
                message: format!(
                    "{} (task plane crashed: {})",
                    fallback_message(TaskPlaneState::Stuck, None),
                    join_err
                ),
                elapsed_ms: started.elapsed().as_millis(),
            },
            Err(_timeout) => HandledResponse::Fallback {
                message: fallback_message(TaskPlaneState::Executing, None),
                elapsed_ms: started.elapsed().as_millis(),
            },
        }
    }
}

fn classify(d: &RouteDecision) -> ResponseKind {
    if d.override_action.as_deref() == Some("steer") {
        ResponseKind::SubAgentReply
    } else {
        ResponseKind::Normal
    }
}

fn summarize(d: &RouterDiagnostics) -> String {
    format!(
        "rule_hits={} mention={:?} explicit_ref={} raw_seq={}",
        d.rule_hints.len(),
        d.mention.mention_mode,
        d.had_explicit_reference,
        d.raw_event_seq,
    )
}
