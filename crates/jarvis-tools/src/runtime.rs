//! Tool execution. Section 11.3 / 30.6.

use std::time::Duration;

use jarvis_core::tool::ToolScope;
use jarvis_db::{raw_event_log, Db, RawEventKind};
use serde_json::Value;
use tokio::time::timeout;

use crate::registry::ToolRegistry;
use crate::result::{ToolResult, ToolStatus};

#[derive(Debug, Clone)]
pub struct CallContext<'a> {
    pub tool_scope: &'a ToolScope,
    pub session_id: Option<&'a str>,
    pub trace_id: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub timeout: Duration,
    /// Whether the user has already granted explicit confirmation for
    /// this tool call. Required when the scope marks the tool as
    /// `requires_confirmation` (Section 11.3 rule 2).
    pub confirmed: bool,
}

impl<'a> CallContext<'a> {
    pub fn new(scope: &'a ToolScope) -> Self {
        Self {
            tool_scope: scope,
            session_id: None,
            trace_id: None,
            agent_id: None,
            timeout: Duration::from_secs(30),
            confirmed: false,
        }
    }

    pub fn with_session(mut self, session_id: &'a str) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_trace(mut self, trace_id: &'a str) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn confirmed(mut self) -> Self {
        self.confirmed = true;
        self
    }
}

#[derive(Clone)]
pub struct ToolRuntime {
    registry: ToolRegistry,
    db: Db,
}

impl ToolRuntime {
    pub fn new(registry: ToolRegistry, db: Db) -> Self {
        Self { registry, db }
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Section 11.3: scope check → confirmation gate → invoke (with
    /// timeout) → audit. Every outcome is appended to `raw_event_log`
    /// twice: once for the call itself and once for the result.
    pub async fn call(&self, name: &str, args: Value, ctx: CallContext<'_>) -> ToolResult {
        // 1. Audit the call request.
        let _ = raw_event_log::append(
            &self.db,
            raw_event_log::AppendEvent {
                event_type: RawEventKind::ToolCall,
                session_id: ctx.session_id,
                trace_id: ctx.trace_id,
                agent_id: ctx.agent_id,
                raw_content: &serde_json::json!({
                    "tool": name,
                    "args": args,
                })
                .to_string(),
                safe_content: None,
            },
        );

        // 2. Scope checks.
        if ctx.tool_scope.blocked_tools.iter().any(|t| t == name) {
            return self.audit_result(
                ctx,
                ToolResult::deny(name, ToolStatus::BlockedByScope, "tool is blocked"),
            );
        }
        if !ctx.tool_scope.is_allowed(name) {
            return self.audit_result(
                ctx,
                ToolResult::deny(
                    name,
                    ToolStatus::PermissionDenied,
                    "tool not in scope.allowed_tools",
                ),
            );
        }
        if ctx.tool_scope.requires_confirmation(name) && !ctx.confirmed {
            return self.audit_result(
                ctx,
                ToolResult::deny(
                    name,
                    ToolStatus::AwaitingConfirmation,
                    "user confirmation required",
                ),
            );
        }

        // 3. Resolve and invoke with a timeout.
        let Some(tool) = self.registry.get(name).cloned() else {
            return self.audit_result(
                ctx,
                ToolResult::deny(name, ToolStatus::Unavailable, "tool not registered"),
            );
        };

        let invoke = tool.invoke(args);
        let result = match timeout(ctx.timeout, invoke).await {
            Ok(r) => r,
            Err(_) => ToolResult::deny(name, ToolStatus::Timeout, "tool call exceeded budget"),
        };
        self.audit_result(ctx, result)
    }

    fn audit_result(&self, ctx: CallContext<'_>, result: ToolResult) -> ToolResult {
        let payload = serde_json::json!({
            "tool": result.tool,
            "status": result.status.as_str(),
            "summary": result.output_summary,
            "error": result.error,
        });
        let _ = raw_event_log::append(
            &self.db,
            raw_event_log::AppendEvent {
                event_type: RawEventKind::ToolResult,
                session_id: ctx.session_id,
                trace_id: ctx.trace_id,
                agent_id: ctx.agent_id,
                raw_content: &payload.to_string(),
                safe_content: None,
            },
        );
        result
    }
}
