//! End-to-end tests for device flow + token store.
//!
//! Device-flow tests use a hand-rolled hyper server that responds
//! according to a small scripted state machine. Store tests use a
//! tempdir.

use super::*;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use chrono::Duration as ChronoDuration;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use jarvis_llm::OAuthProviderConfig;

type SimpleHandler = Arc<
    dyn Fn(
            String,
            String,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response<Full<Bytes>>> + Send>>
        + Send
        + Sync,
>;

async fn spawn_simple(
    handler: SimpleHandler,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let join = tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            let h = handler.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<Incoming>| {
                    let h = h.clone();
                    async move {
                        let path = req.uri().path().to_string();
                        let body = req
                            .into_body()
                            .collect()
                            .await
                            .map(|b| b.to_bytes())
                            .unwrap_or_default();
                        let body_s = String::from_utf8_lossy(&body).into_owned();
                        Ok::<_, Infallible>(h(path, body_s).await)
                    }
                });
                let _ = http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });
    (addr, join)
}

fn cfg(addr: SocketAddr) -> OAuthProviderConfig {
    OAuthProviderConfig {
        device_authorization_endpoint: format!("http://{addr}/device/code"),
        token_endpoint: format!("http://{addr}/token"),
        client_id: "test-client".into(),
        client_secret: None,
        scope: Some("read".into()),
        audience: None,
        user_agent: Some("jarvis-auth-test".into()),
    }
}

fn json_resp(status: u16, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap())
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

// ── device flow ─────────────────────────────────────────────────────────

#[tokio::test]
async fn start_returns_device_code_response() {
    let handler: SimpleHandler = Arc::new(|path, body| {
        Box::pin(async move {
            assert_eq!(path, "/device/code");
            assert!(body.contains("client_id=test-client"));
            assert!(body.contains("scope=read"));
            json_resp(
                200,
                r#"{"device_code":"DC","user_code":"U-CODE","verification_uri":"https://example.com/device","verification_uri_complete":"https://example.com/device?user_code=U-CODE","expires_in":900,"interval":5}"#,
            )
        })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("provider-x", cfg(addr));
    let resp = client.start().await.unwrap();
    assert_eq!(resp.device_code, "DC");
    assert_eq!(resp.user_code, "U-CODE");
    assert_eq!(resp.expires_in, Some(900));
    assert_eq!(resp.interval, Some(5));
    assert_eq!(
        resp.verification_uri_complete.as_deref(),
        Some("https://example.com/device?user_code=U-CODE")
    );
}

#[tokio::test]
async fn poll_once_pending_then_approved() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_h = counter.clone();
    let handler: SimpleHandler = Arc::new(move |path, body| {
        let counter = counter_h.clone();
        Box::pin(async move {
            assert_eq!(path, "/token");
            assert!(body.contains("grant_type=urn"));
            assert!(body.contains("device_code=DC"));
            let n = counter.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                json_resp(400, r#"{"error":"authorization_pending"}"#)
            } else {
                json_resp(
                    200,
                    r#"{"access_token":"AT","token_type":"Bearer","expires_in":3600,"refresh_token":"RT","scope":"read"}"#,
                )
            }
        })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("provider-x", cfg(addr));
    let first = client.poll_once("DC").await.unwrap();
    assert!(matches!(first, PollOutcome::Pending));
    let second = client.poll_once("DC").await.unwrap();
    match second {
        PollOutcome::Approved(t) => {
            assert_eq!(t.provider, "provider-x");
            assert_eq!(t.access_token, "AT");
            assert_eq!(t.refresh_token.as_deref(), Some("RT"));
            assert_eq!(t.token_type, "Bearer");
            assert!(t.expires_at.is_some());
            assert!(!t.is_expired());
        }
        other => panic!("expected Approved, got {other:?}"),
    }
}

#[tokio::test]
async fn poll_once_slow_down() {
    let handler: SimpleHandler = Arc::new(|_path, _body| {
        Box::pin(async move {
            json_resp(400, r#"{"error":"slow_down","error_description":"chill"}"#)
        })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("p", cfg(addr));
    let outcome = client.poll_once("DC").await.unwrap();
    assert!(matches!(outcome, PollOutcome::SlowDown));
}

#[tokio::test]
async fn poll_once_access_denied_is_terminal() {
    let handler: SimpleHandler = Arc::new(|_path, _body| {
        Box::pin(async move { json_resp(400, r#"{"error":"access_denied"}"#) })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("p", cfg(addr));
    let err = client.poll_once("DC").await.unwrap_err();
    assert!(matches!(err, DeviceFlowError::AccessDenied));
}

#[tokio::test]
async fn poll_once_expired_token_is_terminal() {
    let handler: SimpleHandler = Arc::new(|_path, _body| {
        Box::pin(async move { json_resp(400, r#"{"error":"expired_token"}"#) })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("p", cfg(addr));
    let err = client.poll_once("DC").await.unwrap_err();
    assert!(matches!(err, DeviceFlowError::ExpiredToken));
}

#[tokio::test]
async fn poll_once_unparseable_body_yields_upstream_error() {
    let handler: SimpleHandler =
        Arc::new(|_, _| Box::pin(async move { json_resp(500, "boom") }));
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("p", cfg(addr));
    let err = client.poll_once("DC").await.unwrap_err();
    match err {
        DeviceFlowError::Upstream { status, .. } => assert_eq!(status, 500),
        other => panic!("got {other:?}"),
    }
}

#[tokio::test]
async fn refresh_returns_fresh_tokens() {
    let handler: SimpleHandler = Arc::new(|path, body| {
        Box::pin(async move {
            assert_eq!(path, "/token");
            assert!(body.contains("grant_type=refresh_token"));
            assert!(body.contains("refresh_token=OLD"));
            json_resp(
                200,
                r#"{"access_token":"NEW_AT","token_type":"Bearer","expires_in":1800,"refresh_token":"NEW_RT"}"#,
            )
        })
    });
    let (addr, _) = spawn_simple(handler).await;
    let client = DeviceFlowClient::new("p", cfg(addr));
    let t = client.refresh("OLD").await.unwrap();
    assert_eq!(t.access_token, "NEW_AT");
    assert_eq!(t.refresh_token.as_deref(), Some("NEW_RT"));
}

// ── Tokens / TokenResponse ──────────────────────────────────────────────

#[test]
fn token_response_into_tokens_sets_absolute_expiry() {
    let r = TokenResponse {
        access_token: "AT".into(),
        token_type: Some("Bearer".into()),
        expires_in: Some(60),
        refresh_token: None,
        scope: None,
    };
    let t = r.into_tokens("p");
    let expires_at = t.expires_at.expect("set");
    let delta = expires_at - t.obtained_at;
    assert_eq!(delta.num_seconds(), 60);
}

#[test]
fn tokens_is_expired_compares_against_now() {
    let now = chrono::Utc::now();
    let past = Tokens {
        provider: "p".into(),
        access_token: "AT".into(),
        refresh_token: None,
        token_type: "Bearer".into(),
        scope: None,
        expires_at: Some(now - ChronoDuration::seconds(10)),
        obtained_at: now - ChronoDuration::seconds(60),
    };
    assert!(past.is_expired());
    let future = Tokens {
        expires_at: Some(now + ChronoDuration::seconds(60)),
        ..past.clone()
    };
    assert!(!future.is_expired());
    let no_expiry = Tokens {
        expires_at: None,
        ..past
    };
    assert!(!no_expiry.is_expired());
}

#[test]
fn tokens_needs_refresh_respects_leeway() {
    let now = chrono::Utc::now();
    let near = Tokens {
        provider: "p".into(),
        access_token: "AT".into(),
        refresh_token: None,
        token_type: "Bearer".into(),
        scope: None,
        expires_at: Some(now + ChronoDuration::seconds(30)),
        obtained_at: now,
    };
    assert!(near.needs_refresh(ChronoDuration::seconds(60)));
    assert!(!near.needs_refresh(ChronoDuration::seconds(10)));
}

// ── TokenStore ──────────────────────────────────────────────────────────

#[test]
fn store_save_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    let t = Tokens {
        provider: "myprov".into(),
        access_token: "AT".into(),
        refresh_token: Some("RT".into()),
        token_type: "Bearer".into(),
        scope: Some("read".into()),
        expires_at: None,
        obtained_at: chrono::Utc::now(),
    };
    let path = store.save(&t).unwrap();
    assert!(path.exists());
    let loaded = store.load("myprov").unwrap().unwrap();
    assert_eq!(loaded.access_token, "AT");
    assert_eq!(loaded.refresh_token.as_deref(), Some("RT"));
}

#[test]
fn store_load_missing_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    assert!(store.load("nope").unwrap().is_none());
}

#[test]
fn store_delete_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    let t = Tokens {
        provider: "x".into(),
        access_token: "AT".into(),
        refresh_token: None,
        token_type: "Bearer".into(),
        scope: None,
        expires_at: None,
        obtained_at: chrono::Utc::now(),
    };
    store.save(&t).unwrap();
    assert!(store.delete("x").unwrap());
    assert!(!store.delete("x").unwrap());
}

#[test]
fn store_rejects_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    let traversals = ["..", "../etc", "a/b", "a\\b", ".hidden", ""];
    for bad in traversals {
        assert!(
            matches!(
                store.path_for(bad),
                Err(TokenStoreError::InvalidProviderName(_))
            ),
            "{bad:?} should be rejected"
        );
    }
}

#[cfg(unix)]
#[test]
fn store_writes_owner_only_perms() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    let t = Tokens {
        provider: "permcheck".into(),
        access_token: "AT".into(),
        refresh_token: None,
        token_type: "Bearer".into(),
        scope: None,
        expires_at: None,
        obtained_at: chrono::Utc::now(),
    };
    let path = store.save(&t).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0o600 got {:o}", mode);
}

#[test]
fn store_list_providers_alphabetical() {
    let dir = tempfile::tempdir().unwrap();
    let store = TokenStore::new(dir.path());
    for n in ["zzz", "aaa", "mmm"] {
        let t = Tokens {
            provider: n.into(),
            access_token: "AT".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            scope: None,
            expires_at: None,
            obtained_at: chrono::Utc::now(),
        };
        store.save(&t).unwrap();
    }
    let names = store.list_providers().unwrap();
    assert_eq!(names, vec!["aaa", "mmm", "zzz"]);
}

#[test]
fn store_default_dir_respects_env_override() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("JARVIS_AUTH_DIR", dir.path());
    let store = TokenStore::default_dir();
    assert_eq!(store.dir(), dir.path());
    std::env::remove_var("JARVIS_AUTH_DIR");
}

// ── jarvis_llm OAuthProviderConfig round-trip ─────────────────────────

#[test]
fn oauth_config_round_trips_through_toml() {
    let cfg = OAuthProviderConfig {
        device_authorization_endpoint: "https://idp/dc".into(),
        token_endpoint: "https://idp/t".into(),
        client_id: "abc".into(),
        client_secret: None,
        scope: Some("read write".into()),
        audience: None,
        user_agent: Some("jarvis/0.1".into()),
    };
    let mut llm_cfg = jarvis_llm::LlmConfig::default();
    llm_cfg.providers.insert(
        "myprov".into(),
        jarvis_llm::ProviderConfig {
            oauth: Some(cfg.clone()),
            ..Default::default()
        },
    );
    let s = toml::to_string_pretty(&llm_cfg).unwrap();
    assert!(s.contains("[providers.myprov.oauth]"), "got:\n{s}");
    let back: jarvis_llm::LlmConfig = toml::from_str(&s).unwrap();
    assert_eq!(back, llm_cfg);
}
