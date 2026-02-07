pub mod prompt;
pub mod tools;

use snafu::ResultExt;

use crate::annotate::gather::AnnotationContext;
use crate::error::agent_error::{MaxTurnsExceededSnafu, NoAnnotationsSnafu, ProviderSnafu};
use crate::error::AgentError;
use crate::git::GitOps;
use crate::provider::{CompletionRequest, ContentBlock, LlmProvider, Message, Role, StopReason};
use crate::schema::{CrossCuttingConcern, RegionAnnotation};

const MAX_TURNS: u32 = 20;

/// Run the annotation agent loop. Calls the LLM with tools until it finishes
/// or hits the turn limit. Returns collected annotations.
pub fn run_agent_loop(
    provider: &dyn LlmProvider,
    git_ops: &dyn GitOps,
    context: &AnnotationContext,
) -> Result<(Vec<RegionAnnotation>, Vec<CrossCuttingConcern>, String), AgentError> {
    let system_prompt = prompt::build_system_prompt(context);
    let tool_defs = tools::tool_definitions();

    let mut messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "Please annotate this commit.".to_string(),
        }],
    }];

    let mut collected_regions: Vec<RegionAnnotation> = Vec::new();
    let mut collected_cross_cutting: Vec<CrossCuttingConcern> = Vec::new();
    let mut summary = String::new();

    for turn in 0..MAX_TURNS {
        let request = CompletionRequest {
            system: system_prompt.clone(),
            messages: messages.clone(),
            tools: tool_defs.clone(),
            max_tokens: 4096,
        };

        let response = provider.complete(&request).context(ProviderSnafu)?;

        // Collect any text from the response as potential summary
        let mut assistant_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();

        for block in &response.content {
            match block {
                ContentBlock::Text { text } => {
                    assistant_text.push_str(text);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_uses.push((id.clone(), name.clone(), input.clone()));
                }
                _ => {}
            }
        }

        // Add the assistant message to history
        messages.push(Message {
            role: Role::Assistant,
            content: response.content.clone(),
        });

        // If stop reason is EndTurn or MaxTokens with no tool uses, we're done
        if tool_uses.is_empty() {
            summary = assistant_text;
            break;
        }

        // Process tool uses
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        for (id, name, input) in &tool_uses {
            let result = tools::dispatch_tool(
                name,
                input,
                git_ops,
                context,
                &mut collected_regions,
                &mut collected_cross_cutting,
            );
            match result {
                Ok(content) => {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content,
                        is_error: None,
                    });
                }
                Err(e) => {
                    tool_results.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: format!("Error: {e}"),
                        is_error: Some(true),
                    });
                }
            }
        }

        // Add tool results as a user message
        messages.push(Message {
            role: Role::User,
            content: tool_results,
        });

        // Check stop conditions
        if response.stop_reason == StopReason::EndTurn {
            summary = assistant_text;
            break;
        }
        if response.stop_reason == StopReason::MaxTokens {
            summary = assistant_text;
            break;
        }

        if turn + 1 >= MAX_TURNS {
            return MaxTurnsExceededSnafu { turns: MAX_TURNS }.fail();
        }
    }

    if collected_regions.is_empty() {
        return NoAnnotationsSnafu.fail();
    }

    // Use the last text block as summary, fall back to a default
    if summary.is_empty() {
        summary = "Annotation complete.".to_string();
    }

    Ok((collected_regions, collected_cross_cutting, summary))
}
