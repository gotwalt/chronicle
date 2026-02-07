use crate::error::provider_error::ApiSnafu;
use crate::error::ProviderError;
use crate::provider::{
    AuthStatus, CompletionRequest, CompletionResponse, ContentBlock, LlmProvider, StopReason,
    TokenUsage,
};

/// Provider that wraps the `claude` CLI (Claude Code) as a subprocess.
/// Single-turn: sends full prompt via `claude --print -p`, gets text response.
pub struct ClaudeCodeProvider {
    model: Option<String>,
}

impl ClaudeCodeProvider {
    pub fn new(model: Option<String>) -> Self {
        Self { model }
    }
}

impl LlmProvider for ClaudeCodeProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // Build a single text prompt from the request
        let mut prompt = String::new();

        if !request.system.is_empty() {
            prompt.push_str("System: ");
            prompt.push_str(&request.system);
            prompt.push_str("\n\n");
        }

        // Include tool definitions as formatted text
        if !request.tools.is_empty() {
            prompt.push_str("Available tools:\n");
            for tool in &request.tools {
                prompt.push_str(&format!("- {}: {}\n", tool.name, tool.description));
                prompt.push_str(&format!(
                    "  Input schema: {}\n",
                    serde_json::to_string(&tool.input_schema).unwrap_or_default()
                ));
            }
            prompt.push_str("\nTo use a tool, output a JSON block with {\"tool\": \"name\", \"input\": {...}}\n\n");
        }

        // Include messages
        for msg in &request.messages {
            let role = match msg.role {
                crate::provider::Role::User => "User",
                crate::provider::Role::Assistant => "Assistant",
            };
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } => {
                        prompt.push_str(&format!("{role}: {text}\n\n"));
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        prompt.push_str(&format!(
                            "{role}: [tool_use: {} {}]\n\n",
                            name,
                            serde_json::to_string(input).unwrap_or_default()
                        ));
                    }
                    ContentBlock::ToolResult {
                        content, is_error, ..
                    } => {
                        let prefix = if *is_error == Some(true) {
                            "Error"
                        } else {
                            "Result"
                        };
                        prompt.push_str(&format!("{role}: [tool_result: {prefix}] {content}\n\n"));
                    }
                }
            }
        }

        // Spawn claude CLI
        let mut cmd = std::process::Command::new("claude");
        cmd.arg("--print");
        cmd.arg("-p");
        cmd.arg(&prompt);

        if let Some(ref model) = self.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProviderError::Api {
                    message: "Claude CLI not found. Install Claude Code or run 'git chronicle reconfigure' to select a different provider.".to_string(),
                    location: snafu::Location::default(),
                }
            } else {
                ProviderError::Api {
                    message: format!("Failed to spawn claude CLI: {e}"),
                    location: snafu::Location::default(),
                }
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return ApiSnafu {
                message: format!("claude CLI failed: {stderr}"),
            }
            .fail();
        }

        let response_text = String::from_utf8_lossy(&output.stdout).to_string();

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text: response_text,
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage::default(),
        })
    }

    fn check_auth(&self) -> Result<AuthStatus, ProviderError> {
        match std::process::Command::new("claude")
            .arg("--version")
            .output()
        {
            Ok(output) if output.status.success() => Ok(AuthStatus::Valid),
            Ok(_) => Ok(AuthStatus::Invalid(
                "claude CLI returned non-zero exit code".to_string(),
            )),
            Err(e) => Ok(AuthStatus::Invalid(format!("claude CLI not found: {e}"))),
        }
    }

    fn name(&self) -> &str {
        "claude-code"
    }

    fn model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or("claude-sonnet-4-5-20250929")
    }
}
