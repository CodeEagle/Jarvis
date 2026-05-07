//! OAuth 2.0 Device Authorization Grant client (RFC 8628).
//!
//! Two-stage flow:
//!   1. `start()` POSTs to `device_authorization_endpoint`, returns a
//!      `device_code` (kept by the client) and a `user_code` plus a
//!      `verification_uri` (shown to the user).
//!   2. `poll()` repeats the token-endpoint POST every `interval`
//!      seconds, decoding `authorization_pending` / `slow_down` /
//!      `access_denied` / `expired_token` per RFC 8628 §3.5 until the
//!      user completes the grant or the device code expires.

use std::time::Duration;

use serde::Deserialize;
use tracing::{debug, warn};

use crate::oauth_config::OAuthProviderConfig;
use crate::tokens::{TokenResponse, Tokens};

#[derive(Debug, thiserror::Error)]
pub enum DeviceFlowError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("token endpoint returned {status}: {body}")]
    Upstream { status: u16, body: String },
    #[error("authorization request denied by user (access_denied)")]
    AccessDenied,
    #[error("device code expired before approval (expired_token)")]
    ExpiredToken,
    #[error("unsupported / unexpected oauth error: {0}")]
    Other(String),
    #[error("parse: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Some IdPs (e.g. GitHub) return a "complete" URL that bakes in
    /// the user_code so the user doesn't have to type it.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Seconds until the device_code expires.
    #[serde(default)]
    pub expires_in: Option<i64>,
    /// Minimum polling interval, in seconds.
    #[serde(default)]
    pub interval: Option<i64>,
}

/// Single iteration of polling. Surfacing this distinction lets the
/// CLI render an "approve in your browser…" spinner without coupling
/// to the polling loop's internals.
#[derive(Debug)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    Approved(Tokens),
}

pub struct DeviceFlowClient {
    http: reqwest::Client,
    config: OAuthProviderConfig,
    provider: String,
}

impl DeviceFlowClient {
    pub fn new(provider: impl Into<String>, config: OAuthProviderConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            provider: provider.into(),
        }
    }

    pub fn with_http(
        provider: impl Into<String>,
        config: OAuthProviderConfig,
        http: reqwest::Client,
    ) -> Self {
        Self {
            http,
            config,
            provider: provider.into(),
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Stage 1: ask the IdP for a device_code + user_code.
    pub async fn start(&self) -> Result<DeviceCodeResponse, DeviceFlowError> {
        let mut form: Vec<(&str, &str)> =
            vec![("client_id", self.config.client_id.as_str())];
        if let Some(s) = &self.config.scope {
            form.push(("scope", s));
        }
        if let Some(a) = &self.config.audience {
            form.push(("audience", a));
        }
        let mut req = self
            .http
            .post(&self.config.device_authorization_endpoint)
            .form(&form)
            .header("accept", "application/json");
        if let Some(ua) = &self.config.user_agent {
            req = req.header("user-agent", ua);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(DeviceFlowError::Upstream {
                status: status.as_u16(),
                body,
            });
        }
        serde_json::from_str(&body).map_err(|e| {
            DeviceFlowError::Parse(format!("device_code response: {e}; body: {body}"))
        })
    }

    /// One iteration of token-endpoint polling. RFC 8628 §3.5 maps
    /// `error` strings to control-flow:
    ///   - `authorization_pending` → `Pending`
    ///   - `slow_down`              → `SlowDown` (caller bumps interval)
    ///   - `access_denied`          → `Err(AccessDenied)` (terminal)
    ///   - `expired_token`          → `Err(ExpiredToken)` (terminal)
    ///   - any 2xx with token       → `Approved(Tokens)`
    pub async fn poll_once(
        &self,
        device_code: &str,
    ) -> Result<PollOutcome, DeviceFlowError> {
        let form: Vec<(&str, &str)> = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
            ("client_id", self.config.client_id.as_str()),
        ];
        let mut req = self
            .http
            .post(&self.config.token_endpoint)
            .form(&form)
            .header("accept", "application/json");
        if let Some(secret) = &self.config.client_secret {
            req = req.header(
                "authorization",
                basic_auth(&self.config.client_id, secret),
            );
        }
        if let Some(ua) = &self.config.user_agent {
            req = req.header("user-agent", ua);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        debug!(target: "jarvis-auth", status = %status, body = %body, "poll response");

        if status.is_success() {
            let token: TokenResponse = serde_json::from_str(&body).map_err(|e| {
                DeviceFlowError::Parse(format!("token response: {e}; body: {body}"))
            })?;
            return Ok(PollOutcome::Approved(token.into_tokens(&self.provider)));
        }
        // Non-2xx: parse the OAuth error envelope (RFC 6749 §5.2).
        match serde_json::from_str::<OAuthErrorBody>(&body) {
            Ok(err) => match err.error.as_str() {
                "authorization_pending" => Ok(PollOutcome::Pending),
                "slow_down" => Ok(PollOutcome::SlowDown),
                "access_denied" => Err(DeviceFlowError::AccessDenied),
                "expired_token" => Err(DeviceFlowError::ExpiredToken),
                other => Err(DeviceFlowError::Other(format!(
                    "{other}: {}",
                    err.error_description.unwrap_or_default()
                ))),
            },
            Err(_) => Err(DeviceFlowError::Upstream {
                status: status.as_u16(),
                body,
            }),
        }
    }

    /// Refresh an access token using a previously-issued refresh token.
    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<Tokens, DeviceFlowError> {
        let form: Vec<(&str, &str)> = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
        ];
        let mut req = self
            .http
            .post(&self.config.token_endpoint)
            .form(&form)
            .header("accept", "application/json");
        if let Some(secret) = &self.config.client_secret {
            req = req.header(
                "authorization",
                basic_auth(&self.config.client_id, secret),
            );
        }
        if let Some(ua) = &self.config.user_agent {
            req = req.header("user-agent", ua);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            warn!(target: "jarvis-auth", status = %status, body = %body, "refresh failed");
            return Err(DeviceFlowError::Upstream {
                status: status.as_u16(),
                body,
            });
        }
        let token: TokenResponse = serde_json::from_str(&body).map_err(|e| {
            DeviceFlowError::Parse(format!("refresh response: {e}; body: {body}"))
        })?;
        Ok(token.into_tokens(&self.provider))
    }
}

#[derive(Debug, Deserialize)]
struct OAuthErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn basic_auth(user: &str, password: &str) -> String {
    // Stdlib-only base64 — small enough that pulling a crate isn't
    // worth it for one header.
    let raw = format!("{user}:{password}");
    let mut out = String::from("Basic ");
    base64_encode_into(raw.as_bytes(), &mut out);
    out
}

fn base64_encode_into(input: &[u8], out: &mut String) {
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16)
            | ((input[i + 1] as u32) << 8)
            | (input[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
}

/// Default polling cadence used when the IdP omits `interval`.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
