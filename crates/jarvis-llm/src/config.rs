//! Provider/model configuration types.
//!
//! User-facing model ids use `<provider>/<model>` (OpenClaw style).
//! The genai backend rewrites that to `<provider>::<model>`, which is
//! genai's namespaced form for explicit adapter routing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelId {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelIdParseError {
    #[error("expected `<provider>/<model>`, got `{0}`")]
    Format(String),
    #[error("provider segment empty")]
    EmptyProvider,
    #[error("model segment empty")]
    EmptyModel,
}

impl ModelId {
    pub fn parse(s: &str) -> Result<Self, ModelIdParseError> {
        let trimmed = s.trim();
        let (p, m) = match trimmed.split_once('/') {
            Some(parts) => parts,
            None => return Err(ModelIdParseError::Format(trimmed.to_string())),
        };
        if p.is_empty() {
            return Err(ModelIdParseError::EmptyProvider);
        }
        if m.is_empty() {
            return Err(ModelIdParseError::EmptyModel);
        }
        Ok(Self {
            provider: p.to_string(),
            model: m.to_string(),
        })
    }

    /// genai's explicit adapter form: `<provider>::<model>`.
    pub fn to_genai(&self) -> String {
        format!("{}::{}", self.provider, self.model)
    }

    pub fn display(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Name of the env var holding the API key. Preferred — keeps
    /// secrets out of the config file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Inline API key. Avoid in version-controlled configs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Override base URL for OpenAI-compatible endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Native OAuth 2.0 Device Authorization Grant configuration. When
    /// present, the provider authenticates via `jarvis model login
    /// <name>` (token stored in `~/.jarvis/auth/<name>.json`) instead
    /// of an API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthProviderConfig>,
}

/// OAuth 2.0 Device Authorization Grant config (RFC 8628).
///
/// Lives under `[providers.<name>.oauth]` in `~/.jarvis/config.toml`.
/// Consumed by `jarvis-auth` to drive the device flow; declared here
/// so `LlmConfig` round-trips cleanly through TOML without pulling
/// the OAuth machinery as a dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// Device authorization endpoint (RFC 8628 §3.1).
    pub device_authorization_endpoint: String,
    /// Token endpoint (RFC 8628 §3.4 / RFC 6749 §3.2).
    pub token_endpoint: String,
    /// Public client identifier.
    pub client_id: String,
    /// Some IdPs require a client secret on the token endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Space-delimited scope string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Optional `audience` parameter (Auth0-style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    /// User-Agent header sent on auth requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default model id, e.g. `"anthropic/claude-sonnet-4-6"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, ProviderConfig>,
}
