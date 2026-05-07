//! Token records and the wire shape we deserialize from token endpoints.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Tokens persisted to disk. The `obtained_at` + `expires_in` form
/// produced by the token endpoint is normalised here into an absolute
/// `expires_at`, which is more useful for staleness checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub provider: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub obtained_at: DateTime<Utc>,
}

fn default_token_type() -> String {
    "Bearer".into()
}

impl Tokens {
    /// True iff `expires_at` is in the past. Tokens with no declared
    /// expiry are treated as non-expiring.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(t) => Utc::now() >= t,
            None => false,
        }
    }

    /// True when the token will expire within `leeway`. Use this to
    /// trigger a proactive refresh before sending the next API call.
    pub fn needs_refresh(&self, leeway: Duration) -> bool {
        match self.expires_at {
            Some(t) => Utc::now() + leeway >= t,
            None => false,
        }
    }
}

/// Wire shape returned by RFC 6749 §5.1 token endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub token_type: Option<String>,
    /// Lifetime in seconds.
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

impl TokenResponse {
    /// Convert wire response into a persisted [`Tokens`] record.
    pub fn into_tokens(self, provider: impl Into<String>) -> Tokens {
        let now = Utc::now();
        let expires_at = self.expires_in.map(|s| now + Duration::seconds(s));
        Tokens {
            provider: provider.into(),
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_type: self.token_type.unwrap_or_else(default_token_type),
            scope: self.scope,
            expires_at,
            obtained_at: now,
        }
    }
}
