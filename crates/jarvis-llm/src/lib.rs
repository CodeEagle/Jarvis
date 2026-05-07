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
pub mod genai_backend;

pub use completion::{
    ChatMessage, ChatRole, Completion, CompletionError, CompletionRequest,
    CompletionResponse, Usage,
};
pub use config::{LlmConfig, ModelId, ModelIdParseError, ProviderConfig};
pub use genai_backend::GenAiCompletion;

#[cfg(test)]
mod tests;
