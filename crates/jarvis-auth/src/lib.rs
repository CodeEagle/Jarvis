//! Native OAuth 2.0 Device Authorization Grant (RFC 8628) for Jarvis.
//!
//! Lets a provider declare an `oauth` block in `~/.jarvis/config.toml`
//! and run `jarvis model login <provider>` from a terminal — the user
//! is shown a verification URL and a short user code, completes the
//! grant in their browser, and the resulting access/refresh tokens
//! land in `~/.jarvis/auth/<provider>.json` (mode 0o600).
//!
//! Independent of any specific provider — call sites supply the
//! `OAuthProviderConfig` (authorization endpoint, token endpoint,
//! client_id, scope, …). A `Completion` impl that wants to use OAuth
//! pulls the access token from the [`TokenStore`] and refreshes
//! transparently when the cached token has expired.

pub mod device_flow;
pub mod oauth_config;
pub mod store;
pub mod tokens;

pub use device_flow::{
    DeviceCodeResponse, DeviceFlowClient, DeviceFlowError, PollOutcome,
};
pub use oauth_config::OAuthProviderConfig;
pub use store::{TokenStore, TokenStoreError};
pub use tokens::{TokenResponse, Tokens};

#[cfg(test)]
mod tests;
