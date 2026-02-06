pub mod anthropic;
pub mod claude_code;

pub use anthropic::AnthropicProvider;
pub use claude_code::ClaudeCodeProvider;

use crate::error::ProviderError;
use serde::{Deserialize, Serialize};

/// Normalized LLM provider trait. MVP implements Anthropic only.
pub trait LlmProvider: Send + Sync {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
    fn check_auth(&self) -> Result<AuthStatus, ProviderError>;
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

/// Discover the best available provider.
///
/// Priority:
/// 1. User-level config (~/.git-chronicle.toml)
/// 2. Environment variable detection (ANTHROPIC_API_KEY)
/// 3. Error: no provider configured
pub fn discover_provider() -> Result<Box<dyn LlmProvider>, ProviderError> {
    use crate::config::user_config::{ProviderType, UserConfig};

    // 1. Check user-level config
    if let Ok(Some(config)) = UserConfig::load() {
        match config.provider.provider_type {
            ProviderType::ClaudeCode => {
                return Ok(Box::new(ClaudeCodeProvider::new(config.provider.model)));
            }
            ProviderType::Anthropic => {
                let key_env = config
                    .provider
                    .api_key_env
                    .unwrap_or_else(|| "ANTHROPIC_API_KEY".to_string());
                if let Ok(api_key) = std::env::var(&key_env) {
                    if !api_key.is_empty() {
                        return Ok(Box::new(AnthropicProvider::new(
                            api_key,
                            config.provider.model,
                        )));
                    }
                }
                // Config says anthropic but key not found — fall through to env check
            }
            ProviderType::None => {
                // Explicitly configured as none — fall through to env check
            }
        }
    }

    // 2. Fall back to env var detection
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        if !api_key.is_empty() {
            return Ok(Box::new(AnthropicProvider::new(api_key, None)));
        }
    }

    snafu::ensure!(false, crate::error::provider_error::NoCredentialsSnafu);
    unreachable!()
}
