use crate::completion::{
    ChatMessage, ChatRole, Completion, CompletionError, CompletionRequest, CompletionResponse,
};
use crate::config::{LlmConfig, ModelId, ModelIdParseError, ProviderConfig};
use crate::config_loader::{
    load_from_path, provider_authed_full, provider_authed_with, provider_env_var,
    provider_oauth_binary, save_to_path,
};
use crate::router::CompletionRouter;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

// ── ModelId parsing ─────────────────────────────────────────────────────

#[test]
fn parse_provider_slash_model() {
    let m = ModelId::parse("anthropic/claude-sonnet-4-6").unwrap();
    assert_eq!(m.provider, "anthropic");
    assert_eq!(m.model, "claude-sonnet-4-6");
}

#[test]
fn parse_trims_outer_whitespace() {
    let m = ModelId::parse("  openai/gpt-4o-mini  ").unwrap();
    assert_eq!(m.provider, "openai");
    assert_eq!(m.model, "gpt-4o-mini");
}

#[test]
fn parse_rejects_missing_slash() {
    let err = ModelId::parse("claude-sonnet-4-6").unwrap_err();
    assert!(matches!(err, ModelIdParseError::Format(_)));
}

#[test]
fn parse_rejects_empty_provider() {
    let err = ModelId::parse("/model").unwrap_err();
    assert_eq!(err, ModelIdParseError::EmptyProvider);
}

#[test]
fn parse_rejects_empty_model() {
    let err = ModelId::parse("provider/").unwrap_err();
    assert_eq!(err, ModelIdParseError::EmptyModel);
}

#[test]
fn to_genai_uses_double_colon() {
    let m = ModelId::parse("anthropic/claude-sonnet-4-6").unwrap();
    assert_eq!(m.to_genai(), "anthropic::claude-sonnet-4-6");
}

#[test]
fn display_round_trips() {
    let s = "openai/gpt-4o-mini";
    let m = ModelId::parse(s).unwrap();
    assert_eq!(m.to_string(), s);
}

// ── Config (de)serialization round-trip ─────────────────────────────────

#[test]
fn config_roundtrips_through_toml_via_json() {
    // We don't depend on toml in this crate; assert serde_json round-trip
    // and shape, which is the relevant invariant for the loader.
    let mut cfg = LlmConfig {
        default_model: Some("anthropic/claude-sonnet-4-6".into()),
        ..Default::default()
    };
    cfg.providers.insert(
        "anthropic".into(),
        ProviderConfig {
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            ..Default::default()
        },
    );
    let s = serde_json::to_string(&cfg).unwrap();
    let back: LlmConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn config_parses_minimal_json() {
    let s = r#"{"default_model":"openai/gpt-4o-mini"}"#;
    let cfg: LlmConfig = serde_json::from_str(s).unwrap();
    assert_eq!(cfg.default_model.as_deref(), Some("openai/gpt-4o-mini"));
    assert!(cfg.providers.is_empty());
}

// ── Trait object usability ──────────────────────────────────────────────

#[test]
fn completion_trait_is_object_safe_and_accepts_messages() {
    // Compile-time check: trait can be boxed dynamically.
    fn _take(_c: Box<dyn crate::Completion>) {}

    let req = CompletionRequest::new(
        "anthropic/claude-sonnet-4-6",
        vec![
            ChatMessage::system("you are a helpful assistant"),
            ChatMessage::user("hi"),
        ],
    )
    .with_max_tokens(256)
    .with_temperature(0.2);
    assert_eq!(req.messages.len(), 2);
    assert_eq!(req.messages[0].role, ChatRole::System);
    assert_eq!(req.max_tokens, Some(256));
}

// ── Config loader (TOML) ────────────────────────────────────────────────

#[test]
fn config_save_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");

    let mut cfg = LlmConfig {
        default_model: Some("anthropic/claude-sonnet-4-6".into()),
        ..Default::default()
    };
    cfg.providers.insert(
        "anthropic".into(),
        ProviderConfig {
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            ..Default::default()
        },
    );
    save_to_path(&path, &cfg).unwrap();
    let loaded = load_from_path(&path).unwrap();
    assert_eq!(loaded, cfg);
}

#[test]
fn config_load_returns_parse_error_on_garbage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "not = valid = toml = =").unwrap();
    let err = load_from_path(&path).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.starts_with("parse:"), "got: {msg}");
}

#[test]
fn provider_env_var_uses_config_override() {
    let mut cfg = LlmConfig::default();
    cfg.providers.insert(
        "anthropic".into(),
        ProviderConfig {
            api_key_env: Some("MY_CUSTOM_KEY".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        provider_env_var("anthropic", &cfg).as_deref(),
        Some("MY_CUSTOM_KEY")
    );
}

#[test]
fn provider_env_var_falls_back_to_well_known() {
    let cfg = LlmConfig::default();
    assert_eq!(
        provider_env_var("openai", &cfg).as_deref(),
        Some("OPENAI_API_KEY")
    );
    assert_eq!(
        provider_env_var("anthropic", &cfg).as_deref(),
        Some("ANTHROPIC_API_KEY")
    );
}

#[test]
fn provider_env_var_unknown_returns_none() {
    let cfg = LlmConfig::default();
    assert_eq!(provider_env_var("randomprovider", &cfg), None);
}

#[test]
fn provider_authed_reflects_env_presence() {
    let cfg = LlmConfig::default();
    let mut env: HashMap<&str, &str> = HashMap::new();
    let get = |k: &str| env.get(k).map(|s| s.to_string());

    assert!(!provider_authed_with("anthropic", &cfg, &get));

    env.insert("ANTHROPIC_API_KEY", "sk-ant-...");
    let get2 = |k: &str| env.get(k).map(|s| s.to_string());
    assert!(provider_authed_with("anthropic", &cfg, &get2));
}

#[test]
fn provider_authed_local_provider_needs_no_env() {
    let cfg = LlmConfig::default();
    let none = |_: &str| None::<String>;
    assert!(provider_authed_with("ollama", &cfg, &none));
}

// ── OAuth provider readiness ────────────────────────────────────────────

#[test]
fn provider_oauth_binary_known_for_claude_cli() {
    assert_eq!(provider_oauth_binary("claude-cli"), Some("claude"));
    assert_eq!(provider_oauth_binary("anthropic"), None);
    assert_eq!(provider_oauth_binary("randomprovider"), None);
}

#[test]
fn provider_authed_full_oauth_ready_when_binary_present() {
    let cfg = LlmConfig::default();
    let no_env = |_: &str| None::<String>;
    let bin_present = |name: &str| name == "claude";
    assert!(provider_authed_full(
        "claude-cli",
        &cfg,
        &no_env,
        &bin_present
    ));
}

#[test]
fn provider_authed_full_oauth_unready_when_binary_missing() {
    let cfg = LlmConfig::default();
    let no_env = |_: &str| None::<String>;
    let bin_absent = |_: &str| false;
    assert!(!provider_authed_full(
        "claude-cli",
        &cfg,
        &no_env,
        &bin_absent
    ));
}

#[test]
fn provider_authed_full_oauth_skips_env_var_path() {
    // Even when the env var is set, claude-cli readiness must come
    // from the binary check — the OAuth path is the source of truth.
    let cfg = LlmConfig::default();
    let env_set = |_: &str| Some("sk-ant-...".to_string());
    let bin_absent = |_: &str| false;
    assert!(!provider_authed_full(
        "claude-cli",
        &cfg,
        &env_set,
        &bin_absent
    ));
}

// ── CompletionRouter dispatch ───────────────────────────────────────────

struct RecordingBackend {
    label: &'static str,
    seen: Arc<Mutex<Vec<String>>>,
}

impl RecordingBackend {
    fn new(label: &'static str) -> (Self, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                label,
                seen: seen.clone(),
            },
            seen,
        )
    }
}

#[async_trait]
impl Completion for RecordingBackend {
    async fn chat(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, CompletionError> {
        self.seen.lock().unwrap().push(req.model.clone());
        Ok(CompletionResponse {
            model: req.model,
            text: format!("{}-reply", self.label),
            usage: None,
        })
    }
}

#[tokio::test]
async fn router_dispatches_to_registered_provider() {
    let (claude_cli, claude_seen) = RecordingBackend::new("claude-cli");
    let (genai, genai_seen) = RecordingBackend::new("genai");
    let router = CompletionRouter::new(Box::new(genai))
        .with_provider("claude-cli", Box::new(claude_cli));

    let resp = router
        .chat(CompletionRequest::new(
            "claude-cli/sonnet",
            vec![ChatMessage::user("hi")],
        ))
        .await
        .unwrap();
    assert_eq!(resp.text, "claude-cli-reply");
    assert_eq!(claude_seen.lock().unwrap().len(), 1);
    assert!(genai_seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn router_falls_back_to_default_for_unknown_provider() {
    let (claude_cli, claude_seen) = RecordingBackend::new("claude-cli");
    let (genai, genai_seen) = RecordingBackend::new("genai");
    let router = CompletionRouter::new(Box::new(genai))
        .with_provider("claude-cli", Box::new(claude_cli));

    let resp = router
        .chat(CompletionRequest::new(
            "anthropic/claude-sonnet-4-6",
            vec![ChatMessage::user("hi")],
        ))
        .await
        .unwrap();
    assert_eq!(resp.text, "genai-reply");
    assert!(claude_seen.lock().unwrap().is_empty());
    assert_eq!(genai_seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn router_rejects_invalid_model_id() {
    let (genai, _) = RecordingBackend::new("genai");
    let router = CompletionRouter::new(Box::new(genai));
    let err = router
        .chat(CompletionRequest::new(
            "no-slash-here",
            vec![ChatMessage::user("hi")],
        ))
        .await
        .expect_err("should fail");
    assert!(matches!(err, CompletionError::InvalidModelId(_)));
}
