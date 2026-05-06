use super::*;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;

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

fn config(addr: SocketAddr) -> OpenAiConfig {
    OpenAiConfig {
        api_key: "test".into(),
        base_url: format!("http://{addr}"),
        model: "gpt-4o-mini".into(),
        timeout: std::time::Duration::from_secs(2),
        max_tokens: 256,
    }
}

fn inputs<'a>(s: &'a str) -> JudgeInputs<'a> {
    JudgeInputs {
        user_input: s,
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
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "{\"primary_intent\":\"coding.debug\",\"secondary_intents\":[],\"domain\":\"coding\",\"topic\":\"x\",\"session_action\":\"create_new\",\"agent_type\":\"coding\",\"confidence\":0.88,\"clarification_needed\":false,\"router_notes\":\"ok\"}"
            }
        }]
    });
    let body_bytes = body.to_string();
    let h: Handler = Arc::new(move |_req: Request<Incoming>| {
        let b = body_bytes.clone();
        Box::pin(async move {
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(b)))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(h).await;
    let judge = OpenAiJudge::new(config(addr));
    let outcome = judge.judge(inputs("openwrt error")).await.unwrap();
    assert_eq!(outcome.primary_intent, "coding.debug");
    server.abort();
}

#[tokio::test]
async fn judge_returns_none_on_500() {
    let h: Handler = Arc::new(|_req: Request<Incoming>| {
        Box::pin(async {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from("boom")))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(h).await;
    let judge = OpenAiJudge::new(config(addr));
    assert!(judge.judge(inputs("anything")).await.is_none());
    server.abort();
}

#[tokio::test]
async fn judge_sends_bearer_authorization() {
    use std::sync::Mutex;
    use http_body_util::BodyExt;
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    let h: Handler = Arc::new(move |req: Request<Incoming>| {
        let cap = captured_clone.clone();
        Box::pin(async move {
            let auth = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            *cap.lock().unwrap() = auth;
            // drain body to keep keep-alive happy
            let _ = req.into_body().collect().await;
            let body = serde_json::json!({
                "choices":[{"message":{"role":"assistant","content":"{\"primary_intent\":\"chat\",\"secondary_intents\":[],\"domain\":\"chat\",\"topic\":\"x\",\"session_action\":\"create_new\",\"agent_type\":\"general\",\"confidence\":0.5,\"clarification_needed\":true,\"router_notes\":\"ok\"}"}}]
            });
            Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(body.to_string())))
                .unwrap()
        })
    });
    let (addr, server) = spawn_mock(h).await;
    let judge = OpenAiJudge::new(config(addr));
    let _ = judge.judge(inputs("hi")).await;
    let auth = captured.lock().unwrap().clone().expect("captured");
    assert!(auth.starts_with("Bearer "));
    server.abort();
}
