//! Control Plane. Section 9.10.2.
//!
//! Receives every user input, hands it to the Task Plane (the Router for
//! v0.1), and guarantees a response within `fallback_ack`. If the Task
//! Plane runs over budget, the Control Plane returns a fallback message
//! immediately and lets the real result settle in the background.

use std::sync::Arc;
use std::time::Instant;

use jarvis_core::route::{RouteDecision, SessionAction};
use jarvis_db::Db;
use jarvis_memory::compression::{CompressionStore, TurnSummaryDraft};
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
    db: Db,
    sla: ResponseSla,
}

impl ControlPlane {
    pub fn new(db: Db) -> Self {
        Self {
            router: Arc::new(Router::new(db.clone())),
            db,
            sla: ResponseSla::defaults(),
        }
    }

    pub fn with_sla(db: Db, sla: ResponseSla) -> Self {
        Self {
            router: Arc::new(Router::new(db.clone())),
            db,
            sla,
        }
    }

    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    /// Same as `handle_user_input` but consults an external `LlmJudge`
    /// before settling the decision. Used by the chat REPL when the
    /// caller has selected a non-rule judge (e.g. `JARVIS_JUDGE=codex`).
    ///
    /// The judge's wall-clock cost (e.g. ~13s for codex) typically
    /// exceeds the default 2s SLA, so callers should either pass a
    /// generous SLA via [`ControlPlane::with_sla`] or accept that the
    /// user sees a fallback ack while the judge is still working.
    pub async fn handle_user_input_with_judge<J>(
        &self,
        user_input: String,
        session_id_hint: Option<String>,
        running_agent_types: Vec<String>,
        judge: std::sync::Arc<J>,
    ) -> HandledResponse
    where
        J: jarvis_router::LlmJudge + 'static,
    {
        let started = Instant::now();
        let router = self.router.clone();
        let budget = self.sla.fallback_ack;
        let original_input = user_input.clone();

        let task = tokio::spawn(async move {
            router
                .route_with_judge(
                    RouterInput {
                        user_input: &user_input,
                        session_id_hint: session_id_hint.as_deref(),
                        running_agent_types: &running_agent_types,
                    },
                    &*judge,
                )
                .await
        });

        match timeout(budget, task).await {
            Ok(Ok(Ok((decision, diag)))) => {
                record_turn_summary(&self.db, &original_input, &decision, &diag);
                HandledResponse::Resolved {
                    kind: classify(&decision),
                    decision: Box::new(decision),
                    diagnostics_summary: summarize(&diag),
                    elapsed_ms: started.elapsed().as_millis(),
                }
            }
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
        let original_input = user_input.clone();

        // Run the Router on a blocking thread (rusqlite is sync).
        let task = tokio::task::spawn_blocking(move || {
            router.route(RouterInput {
                user_input: &user_input,
                session_id_hint: session_id_hint.as_deref(),
                running_agent_types: &running_agent_types,
            })
        });

        match timeout(budget, task).await {
            Ok(Ok(Ok((decision, diag)))) => {
                record_turn_summary(&self.db, &original_input, &decision, &diag);
                HandledResponse::Resolved {
                    kind: classify(&decision),
                    decision: Box::new(decision),
                    diagnostics_summary: summarize(&diag),
                    elapsed_ms: started.elapsed().as_millis(),
                }
            }
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

/// Best-effort write of a TurnSummary record after a Resolved
/// route. Captures the input + router decision so PRD §14.4's
/// Rolling Summary can later be regenerated by Dream / scheduler
/// without re-reading raw_event_log. Failure is intentionally
/// swallowed — this hook must never break the main response path.
fn record_turn_summary(
    db: &Db,
    user_input: &str,
    decision: &RouteDecision,
    diag: &RouterDiagnostics,
) {
    let session_id = match decision.session_action {
        SessionAction::ContinueExisting | SessionAction::Merge => {
            decision.target_session_id.clone()
        }
        SessionAction::CreateNew | SessionAction::Temporary => None,
    };
    let Some(session_id) = session_id else {
        return;
    };
    let store = CompressionStore::new(db.clone());
    let draft = TurnSummaryDraft {
        session_id,
        trace_id: Some(decision.trace_id.clone()),
        task_id: Some(decision.task_id.clone()),
        user_goal: user_input.chars().take(200).collect(),
        actions_taken: vec![format!(
            "router → {} (confidence={:.2})",
            decision.agent_type, decision.confidence
        )],
        result: decision.router_notes.clone(),
        entities: decision.secondary_intents.clone(),
        unresolved: vec![],
        source_message_ids: vec![format!("raw:{}", diag.raw_event_seq)],
        token_count: user_input.len() as u32 / 4,
    };
    if let Err(e) = store.write_turn_summary(&draft) {
        tracing::warn!(target: "control_plane", "turn_summary write failed: {e}");
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
