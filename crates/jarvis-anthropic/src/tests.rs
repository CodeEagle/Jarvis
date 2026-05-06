//! Tests use a hand-rolled hyper server to simulate Anthropic's API.

use super::*;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use jarvis_router::llm_judge::{JudgeInputs, LlmJudge};

type Handler = Arc<
    dyn Fn(Request<Incoming>) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Response<Full<Bytes>>> + Send>,
        > + Send
        + Sync,
>;

async fn spawn_mock(handler: Handler) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let join = tokio::spawn(async move {
        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            let h = handler.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<Incoming>| {
                    let h = h.clone();
                    async move { Ok::<_, Infallible>(h(req).await) }
                });
                let _ = http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });
    (addr, join)
}

fn config(addr: SocketAddr) -> AnthropicConfig {
    AnthropicConfig {
        api_key: "test-key".into(),
        base_url: format!("http://{addr}"),
        model: "claude-sonnet-4-6".into(),
        timeout: std::time::Duration::from_secs(2),
        max_tokens: 256,
        prompt_caching: true,
    }
}

fn judge_inputs<'a>(input: &'a str) -> JudgeInputs<'a> {
    JudgeInputs {
        user_input: input,
        trace_id: "trc",
        task_id: "task",
        rule_hints: &[],
        recent_session_titles: &[],
        allowed_agents: &[],
    }
}

#[tokio::test]
async fn judge_parses_well_formed_response() {
    let body = serde_json::json!({
        "content": [
            {"type":"text","text":"{\"primary_intent\":\"coding.debug\",\"secondary_intents\":[],\"domain\":\"coding\",\"topic\":\"x\",\"session_action\":\"create_new\",\"agent_type\":\"coding\",\"confidence\":0.9,\"clarification_needed\":false,\"router_notes\":\"ok\"}"}
        ]
    });
    let body_bytes = body.to_string();
    let handler: Handler = Arc::new(move |_req: Request<Incoming>| {
        let body_bytes = body_bytes.clone();
        Box::pin(async move {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(body_bytes)))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(handler).await;
    let judge = AnthropicJudge::new(config(addr));
    let outcome = judge.judge(judge_inputs("openwrt 报错")).await.unwrap();
    assert_eq!(outcome.primary_intent, "coding.debug");
    assert_eq!(outcome.agent_type, "coding");
    server.abort();
}

#[tokio::test]
async fn judge_returns_none_on_500() {
    let handler: Handler = Arc::new(|_req: Request<Incoming>| {
        Box::pin(async {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from("server boom")))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(handler).await;
    let judge = AnthropicJudge::new(config(addr));
    let outcome = judge.judge(judge_inputs("anything")).await;
    assert!(outcome.is_none());
    server.abort();
}

#[tokio::test]
async fn judge_returns_none_on_garbage_text() {
    let body = serde_json::json!({
        "content": [{"type":"text","text":"not json at all"}]
    });
    let body_bytes = body.to_string();
    let handler: Handler = Arc::new(move |_req: Request<Incoming>| {
        let body_bytes = body_bytes.clone();
        Box::pin(async move {
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(body_bytes)))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(handler).await;
    let judge = AnthropicJudge::new(config(addr));
    let outcome = judge.judge(judge_inputs("anything")).await;
    assert!(outcome.is_none());
    server.abort();
}

#[tokio::test]
async fn judge_handles_code_fence_wrapped_json() {
    let body = serde_json::json!({
        "content": [
            {"type":"text","text":"```json\n{\"primary_intent\":\"chat\",\"secondary_intents\":[],\"domain\":\"chat\",\"topic\":\"x\",\"session_action\":\"create_new\",\"agent_type\":\"general\",\"confidence\":0.5,\"clarification_needed\":true,\"router_notes\":\"meh\"}\n```"}
        ]
    });
    let body_bytes = body.to_string();
    let handler: Handler = Arc::new(move |_req: Request<Incoming>| {
        let body_bytes = body_bytes.clone();
        Box::pin(async move {
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(body_bytes)))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(handler).await;
    let judge = AnthropicJudge::new(config(addr));
    let outcome = judge.judge(judge_inputs("anything")).await.unwrap();
    assert_eq!(outcome.agent_type, "general");
    assert!(outcome.clarification_needed);
    server.abort();
}

#[tokio::test]
async fn judge_sends_prompt_cache_header() {
    use std::sync::Mutex;
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    let handler: Handler = Arc::new(move |req: Request<Incoming>| {
        let captured = captured_clone.clone();
        Box::pin(async move {
            let bytes = req.into_body().collect().await.unwrap().to_bytes();
            *captured.lock().unwrap() = Some(String::from_utf8_lossy(&bytes).to_string());
            let body = serde_json::json!({
                "content": [{"type":"text","text":"{\"primary_intent\":\"chat\",\"secondary_intents\":[],\"domain\":\"chat\",\"topic\":\"x\",\"session_action\":\"create_new\",\"agent_type\":\"general\",\"confidence\":0.5,\"clarification_needed\":true,\"router_notes\":\"meh\"}"}]
            });
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(body.to_string())))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(handler).await;
    let judge = AnthropicJudge::new(config(addr));
    let _ = judge.judge(judge_inputs("hi")).await;
    let body = captured.lock().unwrap().clone().expect("captured");
    assert!(
        body.contains("\"cache_control\""),
        "expected cache_control in system block: {body}"
    );
    assert!(body.contains("\"ephemeral\""));
    server.abort();
}
