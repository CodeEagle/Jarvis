//! Main Router (Section 5.1).

use jarvis_core::agent::{AgentDefinition, RuntimeMode};
use jarvis_core::ids::{new_task_id, new_trace_id};
use jarvis_core::intent::IntentHint;
use jarvis_core::mention::{MentionMode, MentionParseResult};
use jarvis_core::route::{RouteDecision, SessionAction};
use jarvis_core::time::now;
use jarvis_core::tool::ToolScope;
use jarvis_db::{raw_event_log, Db};
use jarvis_growth::{Collector, SourceModule};

use crate::agent_registry::{builtin_agents, match_agent};
use crate::intent::{apply_rules, dominant_hint};
use crate::mention::{looks_like_steer, parse_mentions};
use crate::session_resolver::{
    decide, has_explicit_reference, rank_sessions, SessionDecision,
};

/// Inputs callers hand to the router. Equivalent to the TS `RouterInput`.
#[derive(Debug, Clone)]
pub struct RouterInput<'a> {
    pub user_input: &'a str,
    pub session_id_hint: Option<&'a str>,
    /// Currently-running sub-agents (used for steer detection).
    pub running_agent_types: &'a [String],
}

/// Diagnostics emitted alongside a `RouteDecision`. Useful for tracing,
/// tests, and the future Growth Engine collector.
#[derive(Debug, Clone)]
pub struct RouterDiagnostics {
    pub rule_hints: Vec<IntentHint>,
    pub mention: MentionParseResult,
    pub mention_warning: Option<String>,
    pub had_explicit_reference: bool,
    pub raw_event_seq: i64,
}

pub struct Router {
    db: Db,
    registry: Vec<AgentDefinition>,
    growth: Collector,
}

impl Router {
    pub fn new(db: Db) -> Self {
        let growth = Collector::new(db.clone());
        Self {
            db,
            registry: builtin_agents(),
            growth,
        }
    }

    pub fn with_registry(db: Db, registry: Vec<AgentDefinition>) -> Self {
        let growth = Collector::new(db.clone());
        Self { db, registry, growth }
    }

    pub fn registry(&self) -> &[AgentDefinition] {
        &self.registry
    }

    pub fn growth(&self) -> &Collector {
        &self.growth
    }

    /// Route a user input.
    ///
    /// CRITICAL invariant (Section 23.1): the user input is written to
    /// `raw_event_log` before any classification. Even if downstream logic
    /// panics, the original input survives.
    pub fn route(
        &self,
        input: RouterInput<'_>,
    ) -> jarvis_db::error::DbResult<(RouteDecision, RouterDiagnostics)> {
        let trace_id = new_trace_id();
        let task_id = new_task_id();

        // Step 0: persist raw input first.
        let raw_event_seq = raw_event_log::append(
            &self.db,
            raw_event_log::AppendEvent {
                event_type: jarvis_db::RawEventKind::UserMessage,
                session_id: input.session_id_hint,
                trace_id: Some(&trace_id),
                agent_id: None,
                raw_content: input.user_input,
                safe_content: None,
            },
        )?;

        // Step 0a: parse @mentions.
        let mention = parse_mentions(input.user_input, &self.registry);
        log_mention(&self.db, &mention, input.session_id_hint, &trace_id)?;
        let mention_warning = build_mention_warning(&mention, &self.registry);

        // Step 1: rule-layer hints.
        let rule_hints = apply_rules(&mention.clean_input);
        let primary_intent = dominant_hint(&rule_hints)
            .map(|h| h.hint.clone())
            .unwrap_or_else(|| "chat".into());
        let domain = derive_domain(&primary_intent);

        // Step 2: session resolution.
        let session_action;
        let target_session_id;
        let mut clarification_needed = false;
        if has_explicit_reference(input.user_input) {
            // Section 6.4 — explicit reference overrides scoring.
            match input.session_id_hint {
                Some(hint) => {
                    session_action = SessionAction::ContinueExisting;
                    target_session_id = Some(hint.to_string());
                }
                None => {
                    let recent = jarvis_db::session_repo::list_recent(&self.db, 5)?;
                    if let Some(top) = recent.first() {
                        session_action = SessionAction::ContinueExisting;
                        target_session_id = Some(top.id.clone());
                    } else {
                        session_action = SessionAction::CreateNew;
                        target_session_id = None;
                        clarification_needed = true;
                    }
                }
            }
        } else {
            let recent = jarvis_db::session_repo::list_recent(&self.db, 10)?;
            let scored = rank_sessions(input.user_input, &recent, now());
            let (top_id, top_score) = scored
                .first()
                .map(|s| (Some(s.session.id.as_str()), s.score))
                .unwrap_or((None, 0.0));
            match decide(top_id, top_score) {
                SessionDecision::ContinueExisting {
                    session_id,
                    clarification_needed: c,
                } => {
                    session_action = SessionAction::ContinueExisting;
                    target_session_id = Some(session_id);
                    clarification_needed = c;
                }
                SessionDecision::CreateNew | SessionDecision::NeedsClarification => {
                    session_action = SessionAction::CreateNew;
                    target_session_id = None;
                }
            }
        }

        // Step 3: agent selection — applies @mention overrides.
        let chosen_agent_type;
        let mut forced_sub_agents: Vec<String> = Vec::new();
        let mut mention_override = false;
        let mut override_action: Option<String> = None;
        let mut steer_target_sub_task_id: Option<String> = None;
        let mut steer_content: Option<String> = None;

        let valid_mentions: Vec<&jarvis_core::mention::AgentMention> =
            mention.mentions.iter().filter(|m| !m.unresolved).collect();

        match mention.mention_mode {
            MentionMode::Single => {
                let ty = valid_mentions[0]
                    .resolved_type
                    .clone()
                    .expect("mention is valid");
                // Steer mode: if mentioned agent is currently running and
                // the message looks like a directional nudge, mark it for
                // steer rather than dispatch.
                if input
                    .running_agent_types
                    .iter()
                    .any(|t| t == &ty)
                    && looks_like_steer(&mention.clean_input)
                {
                    chosen_agent_type = ty.clone();
                    override_action = Some("steer".into());
                    // sub_task_id is not known here; the conversation bus
                    // is responsible for matching to the running task.
                    steer_target_sub_task_id = None;
                    steer_content = Some(mention.clean_input.clone());
                } else {
                    chosen_agent_type = ty;
                    mention_override = true;
                }
            }
            MentionMode::Multi => {
                chosen_agent_type = "orchestrator".into();
                forced_sub_agents = valid_mentions
                    .iter()
                    .filter_map(|m| m.resolved_type.clone())
                    .collect();
                mention_override = true;
            }
            // Steer detection logic above uses Single + running heuristic.
            // The Steer variant of MentionMode is never produced by the
            // parser yet; we keep parity with the protocol for forward
            // compatibility.
            MentionMode::Steer | MentionMode::None => {
                let agent = match_agent(&primary_intent, &domain, &self.registry);
                chosen_agent_type = agent.r#type.clone();
            }
        }

        let chosen_def = self
            .registry
            .iter()
            .find(|a| a.r#type == chosen_agent_type)
            .cloned()
            .unwrap_or_else(|| {
                self.registry
                    .iter()
                    .find(|a| a.r#type == "general")
                    .expect("general agent")
                    .clone()
            });

        let confidence = base_confidence(&rule_hints, mention_override, clarification_needed);
        let topic = derive_topic(&mention.clean_input);

        let router_notes = build_notes(
            mention_override,
            &chosen_def.r#type,
            &rule_hints,
            override_action.as_deref(),
        );

        let decision = RouteDecision {
            trace_id: trace_id.clone(),
            task_id,
            primary_intent: primary_intent.clone(),
            secondary_intents: vec![],
            domain: domain.clone(),
            topic,
            session_action,
            target_session_id,
            agent_type: chosen_def.r#type.clone(),
            preferred_runtime_mode: chosen_def.preferred_runtime_mode,
            confidence,
            clarification_needed,
            memory_read: true,
            memory_write: matches!(primary_intent.as_str(), "memory.update"),
            skill_candidates: vec![],
            tool_scope: agent_default_scope(&chosen_def, &domain),
            router_notes,
            mention_override,
            forced_sub_agents,
            override_action,
            steer_target_sub_task_id,
            steer_content,
            fallback_used: false,
        };

        // Emit a route_decision event for the Growth Engine. We swallow
        // failures here because the PRD is explicit (Section 5.5):
        // Growth Engine outages must not break routing.
        let event_payload = serde_json::json!({
            "agent_type": decision.agent_type,
            "primary_intent": decision.primary_intent,
            "confidence": decision.confidence,
            "session_action": decision.session_action.as_str(),
            "mention_override": decision.mention_override,
            "fallback_used": decision.fallback_used,
        });
        let _ = self.growth.emit(
            SourceModule::Router,
            "route_decision",
            Some(&trace_id),
            event_payload,
        );

        Ok((
            decision,
            RouterDiagnostics {
                rule_hints,
                mention,
                mention_warning,
                had_explicit_reference: has_explicit_reference(input.user_input),
                raw_event_seq,
            },
        ))
    }
}

fn derive_domain(intent: &str) -> String {
    intent
        .split('.')
        .next()
        .unwrap_or("chat")
        .to_string()
}

fn derive_topic(clean_input: &str) -> String {
    clean_input.chars().take(20).collect::<String>()
}

fn base_confidence(
    hints: &[IntentHint],
    mention_override: bool,
    clarification_needed: bool,
) -> f32 {
    let raw = if hints.is_empty() {
        0.30
    } else {
        let max_w = hints.iter().map(|h| h.weight).fold(0.0_f32, f32::max);
        // Promote slightly when multiple rules agree.
        let bonus = if hints.len() > 1 { 0.10 } else { 0.0 };
        (0.50 + max_w * 0.5 + bonus).min(0.92)
    };
    let with_mention = if mention_override {
        (raw + 0.10).min(0.95)
    } else {
        raw
    };
    if clarification_needed {
        with_mention.min(0.59)
    } else {
        with_mention
    }
}

fn agent_default_scope(agent: &AgentDefinition, _domain: &str) -> ToolScope {
    // For v0.1 we hand back the agent's default. Section 5.2 lets Router
    // narrow this further; tightening rules will arrive with v0.2.
    let _ = agent.preferred_runtime_mode;
    let _ = RuntimeMode::InProcess;
    agent.default_tool_scope.clone()
}

fn build_notes(
    mention_override: bool,
    agent_type: &str,
    hints: &[IntentHint],
    override_action: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(act) = override_action {
        parts.push(format!("[mention override: {act}]"));
    }
    if mention_override {
        parts.push(format!("[user @ specified {agent_type}]"));
    }
    if let Some(top) = dominant_hint(hints) {
        parts.push(format!("rule hint: {} (w={})", top.hint, top.weight));
    } else {
        parts.push("no rule layer hits — routed via fallback".into());
    }
    if parts.is_empty() {
        "default routing".into()
    } else {
        parts.join(" · ")
    }
}

fn build_mention_warning(
    mention: &MentionParseResult,
    registry: &[AgentDefinition],
) -> Option<String> {
    if mention.mentions.is_empty() {
        return None;
    }
    if mention.mentions.iter().all(|m| !m.unresolved) {
        return None;
    }
    let mentionable = crate::mention::format_mentionable_list(registry);
    let unresolved: Vec<_> = mention
        .mentions
        .iter()
        .filter(|m| m.unresolved)
        .map(|m| m.raw_text.clone())
        .collect();
    Some(format!(
        "未识别的助手：{}。当前可以 @ 的助手：{}",
        unresolved.join(", "),
        mentionable
    ))
}

fn log_mention(
    db: &Db,
    mention: &MentionParseResult,
    session_id: Option<&str>,
    trace_id: &str,
) -> jarvis_db::error::DbResult<()> {
    if mention.mentions.is_empty() {
        return Ok(());
    }
    let mode = match mention.mention_mode {
        MentionMode::None => "none",
        MentionMode::Single => "single",
        MentionMode::Multi => "multi",
        MentionMode::Steer => "steer",
    };
    for m in &mention.mentions {
        jarvis_db::mention_log::append(
            db,
            jarvis_db::mention_log::MentionLogEntry {
                session_id,
                trace_id: Some(trace_id),
                raw_text: &m.raw_text,
                resolved_type: m.resolved_type.as_deref(),
                unresolved: m.unresolved,
                mention_mode: mode,
            },
        )?;
    }
    Ok(())
}
