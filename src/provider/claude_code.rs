use std::io::Write;
use std::process::Stdio;

use crate::error::provider_error::ApiSnafu;
use crate::error::ProviderError;
use crate::provider::{
    AuthStatus, CompletionRequest, CompletionResponse, ContentBlock, LlmProvider, StopReason,
    TokenUsage,
};

/// Provider that wraps the `claude` CLI (Claude Code) as a subprocess.
/// Sends full prompt via stdin to `claude --print`, parses text-based tool calls
/// from the response, and converts them to proper `ContentBlock::ToolUse` blocks
/// so the agent loop can dispatch tools normally.
pub struct ClaudeCodeProvider {
    model: Option<String>,
}

impl ClaudeCodeProvider {
    pub fn new(model: Option<String>) -> Self {
        Self { model }
    }
}

/// A tool call extracted from the model's text output.
#[derive(Debug, Clone)]
struct ExtractedToolCall {
    name: String,
    input: serde_json::Value,
    /// Byte offset in the original text where this JSON object starts.
    start: usize,
    /// Byte offset in the original text where this JSON object ends.
    end: usize,
}

/// Scan response text for `{"tool": "...", "input": {...}}` JSON objects.
///
/// Uses `serde_json::Deserializer` at each `{` to handle arbitrarily nested JSON
/// (e.g., `emit_narrative` with `rejected_alternatives` arrays).
fn extract_tool_calls(text: &str) -> Vec<ExtractedToolCall> {
    let mut results = Vec::new();
    let bytes = text.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] != b'{' {
            pos += 1;
            continue;
        }

        let slice = &text[pos..];
        let mut de = serde_json::Deserializer::from_str(slice).into_iter::<serde_json::Value>();

        if let Some(Ok(value)) = de.next() {
            let consumed = de.byte_offset();
            if let Some(obj) = value.as_object() {
                if obj.contains_key("tool") && obj.contains_key("input") {
                    if let (Some(name), Some(input)) =
                        (obj.get("tool").and_then(|v| v.as_str()), obj.get("input"))
                    {
                        results.push(ExtractedToolCall {
                            name: name.to_string(),
                            input: input.clone(),
                            start: pos,
                            end: pos + consumed,
                        });
                        // Skip past this JSON object
                        pos += consumed;
                        continue;
                    }
                }
            }
            // Valid JSON but not a tool call — skip past the opening brace
            pos += 1;
        } else {
            pos += 1;
        }
    }

    results
}

/// Return only the first batch of consecutive tool calls.
///
/// The model simulates its entire multi-turn conversation in one response. We only
/// want the first batch of tool calls (before the model starts writing prose that
/// simulates receiving tool results). If the text gap between two consecutive calls
/// has > 40 non-whitespace characters, the batch ends at the earlier call.
fn first_batch(calls: &[ExtractedToolCall], text: &str) -> Vec<ExtractedToolCall> {
    if calls.is_empty() {
        return Vec::new();
    }

    let mut batch = vec![calls[0].clone()];

    for window in calls.windows(2) {
        let prev = &window[0];
        let next = &window[1];

        let gap = &text[prev.end..next.start];
        let non_ws = gap.chars().filter(|c| !c.is_whitespace()).count();

        if non_ws > 40 {
            break;
        }
        batch.push(next.clone());
    }

    batch
}

/// Convert the first batch of tool calls into `ContentBlock` items with
/// an appropriate `StopReason`.
fn build_content_blocks(
    text: &str,
    batch: &[ExtractedToolCall],
    counter: &mut u32,
) -> (Vec<ContentBlock>, StopReason) {
    if batch.is_empty() {
        return (
            vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            StopReason::EndTurn,
        );
    }

    let mut blocks = Vec::new();

    // Leading text before the first tool call
    let leading = text[..batch[0].start].trim();
    if !leading.is_empty() {
        blocks.push(ContentBlock::Text {
            text: leading.to_string(),
        });
    }

    for call in batch {
        *counter += 1;
        blocks.push(ContentBlock::ToolUse {
            id: format!("toolu_cc_{counter}"),
            name: call.name.clone(),
            input: call.input.clone(),
        });
    }

    (blocks, StopReason::ToolUse)
}

/// Build the text prompt from a `CompletionRequest`.
fn build_prompt(request: &CompletionRequest) -> String {
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
        prompt.push_str(
            "\nTo use a tool, output a JSON block with {\"tool\": \"name\", \"input\": {...}}\n\n",
        );
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

    prompt
}

impl LlmProvider for ClaudeCodeProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let prompt = build_prompt(request);

        // Spawn claude CLI, piping prompt via stdin to avoid OS arg length limits
        let mut cmd = std::process::Command::new("claude");
        cmd.arg("--print");

        if let Some(ref model) = self.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
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

        // Write prompt to stdin, then drop to close the pipe
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|e| ProviderError::Api {
                    message: format!("Failed to write to claude CLI stdin: {e}"),
                    location: snafu::Location::default(),
                })?;
        }

        let output = child.wait_with_output().map_err(|e| ProviderError::Api {
            message: format!("Failed to wait for claude CLI: {e}"),
            location: snafu::Location::default(),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return ApiSnafu {
                message: format!("claude CLI failed: {stderr}"),
            }
            .fail();
        }

        let response_text = String::from_utf8_lossy(&output.stdout).to_string();

        // Extract tool calls and convert to proper ContentBlocks
        let all_calls = extract_tool_calls(&response_text);
        let batch = first_batch(&all_calls, &response_text);
        let mut counter = 0u32;
        let (content, stop_reason) = build_content_blocks(&response_text, &batch, &mut counter);

        Ok(CompletionResponse {
            content,
            stop_reason,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tool_calls_basic() {
        let text = r#"I'll get the diff now.
{"tool": "get_diff", "input": {}}"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_diff");
        assert_eq!(calls[0].input, serde_json::json!({}));
    }

    #[test]
    fn test_extract_tool_calls_nested() {
        let text = r#"{"tool": "emit_narrative", "input": {"summary": "Refactored auth", "rejected_alternatives": [{"approach": "JWT", "reason": "overkill"}]}}"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "emit_narrative");
        let input = &calls[0].input;
        assert_eq!(input["summary"], "Refactored auth");
        assert_eq!(input["rejected_alternatives"][0]["approach"], "JWT");
    }

    #[test]
    fn test_extract_tool_calls_multiple() {
        let text = r#"Let me gather info.
{"tool": "get_diff", "input": {}}
{"tool": "get_commit_info", "input": {}}"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "get_diff");
        assert_eq!(calls[1].name, "get_commit_info");
    }

    #[test]
    fn test_first_batch_stops_at_prose() {
        // First two calls are close together, then there's substantial prose,
        // then another tool call — only the first two should be in the batch.
        let text = r#"{"tool": "get_diff", "input": {}}
{"tool": "get_commit_info", "input": {}}

Okay, now I can see the diff shows a refactored authentication module with several important changes to the token validation flow.

{"tool": "emit_narrative", "input": {"summary": "test"}}"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 3);

        let batch = first_batch(&calls, text);
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].name, "get_diff");
        assert_eq!(batch[1].name, "get_commit_info");
    }

    #[test]
    fn test_no_tool_calls() {
        let text = "This is just a plain text response with no tool calls at all.";
        let calls = extract_tool_calls(text);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_ignores_non_tool_json() {
        let text = r#"Here is some JSON: {"name": "foo", "value": 42} and more text.
{"tool": "get_diff", "input": {}}"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_diff");
    }

    #[test]
    fn test_realistic_output() {
        // Simulates what Claude Code actually returns: a multi-turn simulation
        // where the model "imagines" tool results and continues.
        let text = r#"I'll analyze this commit. Let me start by getting the diff and commit info.

{"tool": "get_diff", "input": {}}
{"tool": "get_commit_info", "input": {}}

Here's the diff output showing the changes:
```
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,10 @@
+use serde::Serialize;
```

And the commit info:
SHA: abc123
Message: Add serialization support

Now I'll emit the narrative and a decision.

{"tool": "emit_narrative", "input": {"summary": "Added serde serialization to core types", "motivation": "Needed for JSON export feature"}}
{"tool": "emit_decision", "input": {"what": "Use serde for serialization", "why": "Industry standard", "stability": "permanent"}}"#;

        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 4);

        let batch = first_batch(&calls, text);
        // Only the first two calls should be in the batch — the prose gap
        // between get_commit_info and emit_narrative is substantial
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].name, "get_diff");
        assert_eq!(batch[1].name, "get_commit_info");
    }

    #[test]
    fn test_build_content_blocks_no_calls() {
        let text = "Just a plain response.";
        let batch = Vec::new();
        let mut counter = 0;
        let (blocks, reason) = build_content_blocks(text, &batch, &mut counter);
        assert_eq!(blocks.len(), 1);
        assert!(
            matches!(&blocks[0], ContentBlock::Text { text } if text == "Just a plain response.")
        );
        assert_eq!(reason, StopReason::EndTurn);
    }

    #[test]
    fn test_build_content_blocks_with_calls() {
        let text = r#"Let me check.
{"tool": "get_diff", "input": {}}
{"tool": "get_commit_info", "input": {}}"#;
        let calls = extract_tool_calls(text);
        let batch = first_batch(&calls, text);
        let mut counter = 0;
        let (blocks, reason) = build_content_blocks(text, &batch, &mut counter);

        // Leading text + 2 tool uses
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], ContentBlock::Text { text } if text == "Let me check."));
        assert!(
            matches!(&blocks[1], ContentBlock::ToolUse { id, name, .. } if id == "toolu_cc_1" && name == "get_diff")
        );
        assert!(
            matches!(&blocks[2], ContentBlock::ToolUse { id, name, .. } if id == "toolu_cc_2" && name == "get_commit_info")
        );
        assert_eq!(reason, StopReason::ToolUse);
    }

    #[test]
    fn test_build_prompt_includes_system_and_tools() {
        use crate::provider::{Message, Role, ToolDefinition};

        let request = CompletionRequest {
            system: "You are a test assistant.".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Hello".to_string(),
                }],
            }],
            tools: vec![ToolDefinition {
                name: "get_diff".to_string(),
                description: "Get the diff.".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
            max_tokens: 4096,
        };

        let prompt = build_prompt(&request);
        assert!(prompt.contains("System: You are a test assistant."));
        assert!(prompt.contains("get_diff"));
        assert!(prompt.contains("User: Hello"));
    }
}
