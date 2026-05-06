#[cfg(test)]
use crate::*;
#[cfg(test)]
use jarvis_core::tool::ToolScope;
#[cfg(test)]
use jarvis_db::Db;
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
fn registry_with_simple_tools() -> registry::ToolRegistry {
    let mut r = registry::ToolRegistry::new();
    r.register_fn("read_file", |args| async move {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        result::ToolResult::ok(
            "read_file",
            format!("read {} (mock)", path),
            json!({ "path": path, "contents": "stub" }),
        )
    });
    r.register_fn("shell.exec", |_args| async move {
        result::ToolResult::ok("shell.exec", "ran (mock)", json!({}))
    });
    r.register_fn("slow_tool", |_args| async move {
        // Sleep beyond any reasonable test timeout.
        tokio::time::sleep(Duration::from_secs(60)).await;
        result::ToolResult::ok("slow_tool", "should not reach", json!({}))
    });
    r
}

#[tokio::test]
async fn allowed_tool_succeeds() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let res = rt
        .call(
            "read_file",
            json!({ "path": "/tmp/x" }),
            CallContext::new(&scope),
        )
        .await;
    assert_eq!(res.status, result::ToolStatus::Success);
}

#[tokio::test]
async fn tool_outside_scope_denied() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let res = rt
        .call("shell.exec", json!({}), CallContext::new(&scope))
        .await;
    assert_eq!(res.status, result::ToolStatus::PermissionDenied);
}

#[tokio::test]
async fn blocked_tool_overrides_allow_list() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["shell.exec".into()],
        blocked_tools: vec!["shell.exec".into()],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let res = rt
        .call("shell.exec", json!({}), CallContext::new(&scope))
        .await;
    assert_eq!(res.status, result::ToolStatus::BlockedByScope);
}

#[tokio::test]
async fn confirmation_required_pauses_until_user_confirms() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["shell.exec".into()],
        blocked_tools: vec![],
        requires_confirmation: vec!["shell.exec".into()],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let pending = rt
        .call("shell.exec", json!({}), CallContext::new(&scope))
        .await;
    assert_eq!(pending.status, result::ToolStatus::AwaitingConfirmation);

    // Re-call with confirmed.
    let confirmed = rt
        .call(
            "shell.exec",
            json!({}),
            CallContext::new(&scope).confirmed(),
        )
        .await;
    assert_eq!(confirmed.status, result::ToolStatus::Success);
}

#[tokio::test]
async fn unknown_tool_is_unavailable() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["mysterious".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let res = rt
        .call("mysterious", json!({}), CallContext::new(&scope))
        .await;
    assert_eq!(res.status, result::ToolStatus::Unavailable);
}

#[tokio::test]
async fn slow_tool_times_out() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db);
    let scope = ToolScope {
        allowed_tools: vec!["slow_tool".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    let res = rt
        .call(
            "slow_tool",
            json!({}),
            CallContext::new(&scope).with_timeout(Duration::from_millis(50)),
        )
        .await;
    assert_eq!(res.status, result::ToolStatus::Timeout);
}

#[tokio::test]
async fn every_call_writes_to_raw_event_log() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db.clone());
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    rt.call("read_file", json!({ "path": "/x" }), CallContext::new(&scope))
        .await;

    // Should have at least 2 events: tool_call + tool_result.
    let n = jarvis_db::raw_event_log::count(&db).unwrap();
    assert!(n >= 2, "expected ≥2 raw events, got {n}");
}

#[tokio::test]
async fn denied_call_still_audited() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db.clone());
    let scope = ToolScope {
        allowed_tools: vec![],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 0,
        max_parallel_tools: 0,
    };
    rt.call("shell.exec", json!({}), CallContext::new(&scope))
        .await;
    let n = jarvis_db::raw_event_log::count(&db).unwrap();
    assert!(n >= 2);
}

#[tokio::test]
async fn tool_call_creates_audit_log_entry() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db.clone());
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    rt.call(
        "read_file",
        json!({ "path": "/x" }),
        CallContext::new(&scope).with_session("sess_audit"),
    )
    .await;
    let entries =
        jarvis_db::audit_log::list_for_session(&db, "sess_audit", 10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "tool.call");
    assert_eq!(entries[0].target.as_deref(), Some("read_file"));
    assert_eq!(entries[0].status, jarvis_db::audit_log::AuditStatus::Success);
}

#[tokio::test]
async fn denied_call_records_denied_audit_status() {
    let db = Db::in_memory().unwrap();
    let rt = ToolRuntime::new(registry_with_simple_tools(), db.clone());
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 10,
        max_parallel_tools: 1,
    };
    rt.call(
        "shell.exec",
        json!({}),
        CallContext::new(&scope).with_session("sess_denied"),
    )
    .await;
    let entries =
        jarvis_db::audit_log::list_for_session(&db, "sess_denied", 10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, jarvis_db::audit_log::AuditStatus::Denied);
}

// ── sandbox spec ───────────────────────────────────────────────────────

#[test]
fn sandbox_default_denies_everything() {
    let s = sandbox::SandboxSpec::deny_all();
    let err = s.validate("ls", &[], None).unwrap_err();
    assert_eq!(err, sandbox::SandboxDenial::ProgramNotAllowed("ls".into()));
}

#[test]
fn sandbox_allow_program_lets_it_through() {
    let s = sandbox::SandboxSpec::deny_all().allow_program("ls");
    s.validate("ls", &["-la".into()], None).unwrap();
}

#[test]
fn sandbox_rejects_forbidden_pattern_in_args() {
    let s = sandbox::SandboxSpec::deny_all().allow_program("sh");
    let err = s
        .validate("sh", &["-c".into(), "rm -rf / yolo".into()], None)
        .unwrap_err();
    matches!(err, sandbox::SandboxDenial::ForbiddenPattern(_))
        .then_some(())
        .expect("expected ForbiddenPattern");
}

#[test]
fn sandbox_args_too_long_denied() {
    let s = sandbox::SandboxSpec {
        allowed_programs: vec!["sh".into()],
        forbidden_patterns: vec![],
        max_args_length: Some(8),
        allowed_cwds: vec![],
    };
    let err = s
        .validate("sh", &["-c".into(), "echo hellooooo world".into()], None)
        .unwrap_err();
    matches!(err, sandbox::SandboxDenial::ArgsTooLong(_, _))
        .then_some(())
        .expect("expected ArgsTooLong");
}

#[test]
fn sandbox_cwd_must_be_allowed_when_restricted() {
    let s = sandbox::SandboxSpec::deny_all()
        .allow_program("ls")
        .allow_cwd("/tmp/jarvis");
    let err = s.validate("ls", &[], Some("/etc")).unwrap_err();
    matches!(err, sandbox::SandboxDenial::CwdNotAllowed(_))
        .then_some(())
        .expect("expected CwdNotAllowed");
    s.validate("ls", &[], Some("/tmp/jarvis/sub")).unwrap();
}

// ── plugin registry (Section 24.5) ─────────────────────────────────────

#[cfg(test)]
struct EchoPlugin {
    id: &'static str,
    label: &'static str,
}

#[async_trait::async_trait]
impl plugin::Plugin for EchoPlugin {
    fn id(&self) -> &str { self.id }
    fn label(&self) -> &str { self.label }
    async fn build_tools(&self) -> Result<Vec<registry::ToolBox>, plugin::PluginError> {
        let mut reg = registry::ToolRegistry::new();
        reg.register_fn("echo.tool", |args| async move {
            result::ToolResult::ok("echo.tool", "echoed", args)
        });
        Ok(reg.names()
            .into_iter()
            .filter_map(|n| reg.get(&n).cloned())
            .collect())
    }
}

#[tokio::test]
async fn plugin_install_registers_its_tools() {
    let pr = plugin::PluginRegistry::new();
    let mut tools = registry::ToolRegistry::new();
    let loaded = pr
        .install(std::sync::Arc::new(EchoPlugin { id: "echo", label: "Echo" }), &mut tools)
        .await
        .unwrap();
    assert_eq!(loaded.id, "echo");
    assert!(loaded.tool_names.contains(&"echo.tool".to_string()));
    assert!(tools.get("echo.tool").is_some());
    assert_eq!(pr.provenance_of("echo.tool").as_deref(), Some("echo"));
}

#[tokio::test]
async fn plugin_install_rejects_duplicate_id() {
    let pr = plugin::PluginRegistry::new();
    let mut tools = registry::ToolRegistry::new();
    pr.install(std::sync::Arc::new(EchoPlugin { id: "echo", label: "" }), &mut tools)
        .await
        .unwrap();
    let err = pr
        .install(std::sync::Arc::new(EchoPlugin { id: "echo", label: "" }), &mut tools)
        .await
        .unwrap_err();
    matches!(err, plugin::PluginError::AlreadyRegistered(_))
        .then_some(())
        .expect("expected AlreadyRegistered");
}

#[tokio::test]
async fn plugin_unregister_removes_provenance() {
    let pr = plugin::PluginRegistry::new();
    let mut tools = registry::ToolRegistry::new();
    pr.install(std::sync::Arc::new(EchoPlugin { id: "echo", label: "" }), &mut tools)
        .await
        .unwrap();
    let removed = pr.unregister("echo").unwrap();
    assert!(removed.contains(&"echo.tool".to_string()));
    assert!(pr.provenance_of("echo.tool").is_none());
}
