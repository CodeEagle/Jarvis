//! Re-export of [`jarvis_llm::OAuthProviderConfig`] for ergonomics.
//!
//! The on-disk shape lives in `jarvis-llm` (it's part of `LlmConfig`)
//! so callers can read it from `~/.jarvis/config.toml` without
//! depending on the OAuth machinery; this crate consumes it.

pub use jarvis_llm::OAuthProviderConfig;
