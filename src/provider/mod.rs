pub mod anthropic;

pub use anthropic::AnthropicProvider;

use crate::error::ProviderError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Normalized LLM provider trait. MVP implements Anthropic only.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    async fn check_auth(&self) -> Result<AuthStatus, ProviderError>;
    fn name(&self) -> &str;
    fn model(&self) -> &str;
}

#[derive(Debug, Clone)]
pub enum AuthStatus {
    Valid,
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Discover the first available provider from environment credentials.
/// MVP: checks ANTHROPIC_API_KEY only.
pub fn discover_provider() -> Result<Box<dyn LlmProvider>, ProviderError> {
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            return Ok(Box::new(AnthropicProvider::new(api_key, None)));
        }
    }
    snafu::ensure!(false, crate::error::provider_error::NoCredentialsSnafu);
    unreachable!()
}
