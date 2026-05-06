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
