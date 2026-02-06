use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::error::provider_error::{ApiSnafu, HttpSnafu, RetriesExhaustedSnafu};
use crate::error::ProviderError;
use crate::provider::{
    AuthStatus, CompletionRequest, CompletionResponse, ContentBlock, LlmProvider, Message,
    StopReason, TokenUsage, ToolDefinition,
};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_RETRIES: u32 = 3;

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    agent: ureq::Agent,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| "claude-sonnet-4-5-20250929".to_string()),
            agent: ureq::agent(),
        }
    }
}

// -- Anthropic API request/response types --

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiToolDef>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: Vec<ApiContentBlock>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiContentBlock {
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

#[derive(Serialize)]
struct ApiToolDef {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ApiContentBlock>,
    stop_reason: String,
    usage: ApiUsage,
}

#[derive(Deserialize)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

// -- Conversions --

fn to_api_messages(messages: &[Message]) -> Vec<ApiMessage> {
    messages
        .iter()
        .map(|m| ApiMessage {
            role: match m.role {
                crate::provider::Role::User => "user".to_string(),
                crate::provider::Role::Assistant => "assistant".to_string(),
            },
            content: m
                .content
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => ApiContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => ApiContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => ApiContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    },
                })
                .collect(),
        })
        .collect()
}

fn to_api_tools(tools: &[ToolDefinition]) -> Vec<ApiToolDef> {
    tools
        .iter()
        .map(|t| ApiToolDef {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect()
}

fn from_api_content(blocks: Vec<ApiContentBlock>) -> Vec<ContentBlock> {
    blocks
        .into_iter()
        .map(|b| match b {
            ApiContentBlock::Text { text } => ContentBlock::Text { text },
            ApiContentBlock::ToolUse { id, name, input } => {
                ContentBlock::ToolUse { id, name, input }
            }
            ApiContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            },
        })
        .collect()
}

fn from_api_stop_reason(reason: &str) -> StopReason {
    match reason {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    }
}

impl LlmProvider for AnthropicProvider {
    fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_request = ApiRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            system: request.system.clone(),
            messages: to_api_messages(&request.messages),
            tools: to_api_tools(&request.tools),
        };

        let body = serde_json::to_value(&api_request).map_err(|e| ProviderError::ParseResponse {
            message: e.to_string(),
            location: snafu::Location::default(),
        })?;

        for attempt in 0..MAX_RETRIES {
            match self
                .agent
                .post(API_URL)
                .set("x-api-key", &self.api_key)
                .set("anthropic-version", ANTHROPIC_VERSION)
                .set("content-type", "application/json")
                .send_json(&body)
            {
                Ok(resp) => {
                    let api_resp: ApiResponse =
                        resp.into_json().map_err(|e| ProviderError::ParseResponse {
                            message: e.to_string(),
                            location: snafu::Location::default(),
                        })?;
                    return Ok(CompletionResponse {
                        content: from_api_content(api_resp.content),
                        stop_reason: from_api_stop_reason(&api_resp.stop_reason),
                        usage: TokenUsage {
                            input_tokens: api_resp.usage.input_tokens,
                            output_tokens: api_resp.usage.output_tokens,
                        },
                    });
                }
                Err(ureq::Error::Status(code, resp)) => {
                    // Retryable: 429 and 5xx
                    if code == 429 || code >= 500 {
                        let retry_after = resp
                            .header("retry-after")
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or_else(|| 2u64.pow(attempt));

                        let error_body = resp.into_string().unwrap_or_default();
                        tracing::warn!(
                            attempt = attempt + 1,
                            status = code,
                            retry_after,
                            "retryable API error: {error_body}"
                        );
                        std::thread::sleep(std::time::Duration::from_secs(retry_after));
                        continue;
                    }

                    if code == 401 {
                        return Err(ProviderError::AuthFailed {
                            message: "invalid API key".to_string(),
                            location: snafu::Location::default(),
                        });
                    }

                    // Non-retryable status errors
                    let error_body = resp.into_string().unwrap_or_default();
                    let message = serde_json::from_str::<ApiErrorResponse>(&error_body)
                        .map(|e| e.error.message)
                        .unwrap_or_else(|_| format!("status {code}: {error_body}"));

                    return ApiSnafu { message }.fail();
                }
                Err(ureq::Error::Transport(t)) => {
                    return Err(Box::new(t)).context(HttpSnafu);
                }
            }
        }

        RetriesExhaustedSnafu {
            attempts: MAX_RETRIES,
        }
        .fail()
    }

    fn check_auth(&self) -> Result<AuthStatus, ProviderError> {
        // Send a minimal message to verify the API key works
        let request = CompletionRequest {
            system: String::new(),
            messages: vec![Message {
                role: crate::provider::Role::User,
                content: vec![ContentBlock::Text {
                    text: "hi".to_string(),
                }],
            }],
            tools: vec![],
            max_tokens: 1,
        };

        match self.complete(&request) {
            Ok(_) => Ok(AuthStatus::Valid),
            Err(ProviderError::AuthFailed { message, .. }) => Ok(AuthStatus::Invalid(message)),
            Err(e) => Err(e),
        }
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
