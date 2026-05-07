//! `Completion` backed by the [`genai`] crate.

use async_trait::async_trait;
use genai::chat::{ChatMessage as GChatMessage, ChatRequest, ChatResponse};
use genai::Client;

use crate::completion::{
    ChatRole, Completion, CompletionError, CompletionRequest, CompletionResponse, Usage,
};
use crate::config::ModelId;

pub struct GenAiCompletion {
    client: Client,
}

impl GenAiCompletion {
    pub fn new() -> Self {
        Self {
            client: Client::default(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

impl Default for GenAiCompletion {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Completion for GenAiCompletion {
    async fn chat(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, CompletionError> {
        let model_id = ModelId::parse(&req.model)
            .map_err(|e| CompletionError::InvalidModelId(e.to_string()))?;
        let model_for_genai = model_id.to_genai();

        let mut chat_req = ChatRequest::new(Vec::<GChatMessage>::new());
        for m in &req.messages {
            let g_msg = match m.role {
                ChatRole::System => GChatMessage::system(m.content.clone()),
                ChatRole::User => GChatMessage::user(m.content.clone()),
                ChatRole::Assistant => GChatMessage::assistant(m.content.clone()),
            };
            chat_req = chat_req.append_message(g_msg);
        }

        let resp: ChatResponse = self
            .client
            .exec_chat(&model_for_genai, chat_req, None)
            .await
            .map_err(|e| CompletionError::Backend(format!("{e}")))?;

        let usage = Some(Usage {
            input_tokens: resp.usage.prompt_tokens.unwrap_or(0).max(0) as u32,
            output_tokens: resp.usage.completion_tokens.unwrap_or(0).max(0) as u32,
        });
        let text = resp.into_first_text().unwrap_or_default();
        Ok(CompletionResponse {
            model: req.model,
            text,
            usage,
        })
    }
}
