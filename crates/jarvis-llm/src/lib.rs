//! Multi-provider LLM completion abstraction.
//!
//! Defines a `Completion` trait and a default backend that routes
//! requests through `genai` so the caller picks a provider with a
//! single `<provider>/<model>` string (e.g. `anthropic/claude-sonnet-4-6`).
//! The router/judge crates already had per-provider `LlmJudge` impls;
//! this crate covers the conversational-completion side that was
//! previously missing — there was no path for an agent to actually
//! generate a reply with a model.

pub mod completion;
pub mod config;
pub mod config_loader;
pub mod genai_backend;
pub mod router;

pub use completion::{
    ChatMessage, ChatRole, Completion, CompletionError, CompletionRequest,
    CompletionResponse, Usage,
};
pub use config::{LlmConfig, ModelId, ModelIdParseError, ProviderConfig};
pub use config_loader::{
    binary_on_path, config_path, load_default, load_from_path,
    provider_authed, provider_authed_full, provider_authed_with,
    provider_env_var, provider_oauth_binary, save_default, save_to_path,
    ConfigError,
};
pub use genai_backend::GenAiCompletion;
pub use router::CompletionRouter;

#[cfg(test)]
mod tests;
