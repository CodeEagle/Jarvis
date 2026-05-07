//! Per-provider `Completion` dispatch.
//!
//! The default `GenAiCompletion` covers everything genai supports
//! (Anthropic / OpenAI / Gemini / … via API key). OAuth-only providers
//! (e.g. `claude-cli`) plug in here as named overrides — incoming
//! `<provider>/<model>` requests are routed by provider segment.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::completion::{Completion, CompletionError, CompletionRequest, CompletionResponse};
use crate::config::ModelId;

pub struct CompletionRouter {
    providers: HashMap<String, Box<dyn Completion>>,
    default: Box<dyn Completion>,
}

impl CompletionRouter {
    pub fn new(default: Box<dyn Completion>) -> Self {
        Self {
            providers: HashMap::new(),
            default,
        }
    }

    pub fn with_provider(
        mut self,
        name: impl Into<String>,
        backend: Box<dyn Completion>,
    ) -> Self {
        self.providers.insert(name.into(), backend);
        self
    }

    pub fn registered_providers(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(String::as_str)
    }
}

#[async_trait]
impl Completion for CompletionRouter {
    async fn chat(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, CompletionError> {
        let parsed = ModelId::parse(&req.model)
            .map_err(|e| CompletionError::InvalidModelId(e.to_string()))?;
        let backend = self
            .providers
            .get(&parsed.provider)
            .map(Box::as_ref)
            .unwrap_or(self.default.as_ref());
        backend.chat(req).await
    }
}
