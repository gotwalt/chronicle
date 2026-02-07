use std::path::Path;

use snafu::ResultExt;

use crate::annotate::gather::AnnotationContext;
use crate::error::agent_error::{GitSnafu, JsonSnafu};
use crate::error::AgentError;
use crate::git::{GitOps, HunkLine};
use crate::provider::ToolDefinition;
use crate::schema::{CrossCuttingConcern, RegionAnnotation};

/// Return the tool definitions the agent has access to.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "get_diff".to_string(),
            description: "Get the full unified diff for this commit.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "get_file_content".to_string(),
            description: "Get the content of a file at this commit.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the file to read"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_ast_outline".to_string(),
            description: "Get a tree-sitter AST outline of semantic units in a file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the file to analyze"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "get_commit_info".to_string(),
            description: "Get commit metadata: SHA, message, author, timestamp.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "emit_annotation".to_string(),
            description: "Emit a region annotation for a changed semantic unit.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "ast_anchor": {
                        "type": "object",
                        "properties": {
                            "unit_type": { "type": "string" },
                            "name": { "type": "string" },
                            "signature": { "type": "string" }
                        },
                        "required": ["unit_type", "name"]
                    },
                    "lines": {
                        "type": "object",
                        "properties": {
                            "start": { "type": "integer" },
                            "end": { "type": "integer" }
                        },
                        "required": ["start", "end"]
                    },
                    "intent": { "type": "string", "description": "What this change does and why" },
                    "reasoning": { "type": "string" },
                    "constraints": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "text": { "type": "string" },
                                "source": { "type": "string", "enum": ["author", "inferred"] }
                            },
                            "required": ["text", "source"]
                        }
                    },
                    "semantic_dependencies": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "anchor": { "type": "string" },
                                "nature": { "type": "string" }
                            },
                            "required": ["file", "anchor", "nature"]
                        }
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "risk_notes": { "type": "string" }
                },
                "required": ["file", "ast_anchor", "lines", "intent"]
            }),
        },
        ToolDefinition {
            name: "emit_cross_cutting".to_string(),
            description: "Emit a cross-cutting concern that spans multiple regions.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string" },
                    "regions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "anchor": { "type": "string" }
                            },
                            "required": ["file", "anchor"]
                        }
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["description", "regions"]
            }),
        },
    ]
}

/// Dispatch a tool call by name, returning the result string.
pub fn dispatch_tool(
    name: &str,
    input: &serde_json::Value,
    git_ops: &dyn GitOps,
    context: &AnnotationContext,
    collected_regions: &mut Vec<RegionAnnotation>,
    collected_cross_cutting: &mut Vec<CrossCuttingConcern>,
) -> Result<String, AgentError> {
    match name {
        "get_diff" => dispatch_get_diff(context),
        "get_file_content" => dispatch_get_file_content(input, git_ops, context),
        "get_ast_outline" => dispatch_get_ast_outline(input, git_ops, context),
        "get_commit_info" => dispatch_get_commit_info(context),
        "emit_annotation" => dispatch_emit_annotation(input, collected_regions),
        "emit_cross_cutting" => dispatch_emit_cross_cutting(input, collected_cross_cutting),
        _ => Ok(format!("Unknown tool: {name}")),
    }
}

fn dispatch_get_diff(context: &AnnotationContext) -> Result<String, AgentError> {
    let mut out = String::new();
    for diff in &context.diffs {
        out.push_str(&format!(
            "--- a/{}\n+++ b/{}\n",
            diff.old_path.as_deref().unwrap_or(&diff.path),
            &diff.path
        ));
        for hunk in &diff.hunks {
            out.push_str(&hunk.header);
            out.push('\n');
            for line in &hunk.lines {
                match line {
                    HunkLine::Context(s) => {
                        out.push(' ');
                        out.push_str(s);
                        out.push('\n');
                    }
                    HunkLine::Added(s) => {
                        out.push('+');
                        out.push_str(s);
                        out.push('\n');
                    }
                    HunkLine::Removed(s) => {
                        out.push('-');
                        out.push_str(s);
                        out.push('\n');
                    }
                }
            }
        }
    }
    Ok(out)
}

fn dispatch_get_file_content(
    input: &serde_json::Value,
    git_ops: &dyn GitOps,
    context: &AnnotationContext,
) -> Result<String, AgentError> {
    let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
        AgentError::InvalidAnnotation {
            message: "get_file_content requires 'path' parameter".to_string(),
            location: snafu::Location::default(),
        }
    })?;
    let content = git_ops
        .file_at_commit(Path::new(path), &context.commit_sha)
        .context(GitSnafu)?;
    Ok(content)
}

fn dispatch_get_ast_outline(
    input: &serde_json::Value,
    git_ops: &dyn GitOps,
    context: &AnnotationContext,
) -> Result<String, AgentError> {
    let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
        AgentError::InvalidAnnotation {
            message: "get_ast_outline requires 'path' parameter".to_string(),
            location: snafu::Location::default(),
        }
    })?;

    let source = git_ops
        .file_at_commit(Path::new(path), &context.commit_sha)
        .context(GitSnafu)?;

    let language = crate::ast::Language::from_path(path);
    match crate::ast::extract_outline(&source, language) {
        Ok(entries) => {
            let mut out = String::new();
            for entry in &entries {
                out.push_str(&format!(
                    "{} {} (lines {}-{})",
                    entry.kind.as_str(),
                    entry.name,
                    entry.lines.start,
                    entry.lines.end,
                ));
                if let Some(sig) = &entry.signature {
                    out.push_str(&format!(" sig: {sig}"));
                }
                out.push('\n');
            }
            Ok(out)
        }
        Err(e) => Ok(format!("AST outline not available: {e}")),
    }
}

fn dispatch_get_commit_info(context: &AnnotationContext) -> Result<String, AgentError> {
    Ok(format!(
        "SHA: {}\nMessage: {}\nAuthor: {} <{}>\nTimestamp: {}",
        context.commit_sha,
        context.commit_message,
        context.author_name,
        context.author_email,
        context.timestamp,
    ))
}

fn dispatch_emit_annotation(
    input: &serde_json::Value,
    collected_regions: &mut Vec<RegionAnnotation>,
) -> Result<String, AgentError> {
    let annotation: RegionAnnotation = serde_json::from_value(input.clone()).context(JsonSnafu)?;
    collected_regions.push(annotation);
    Ok(format!(
        "Annotation emitted. Total annotations: {}",
        collected_regions.len()
    ))
}

fn dispatch_emit_cross_cutting(
    input: &serde_json::Value,
    collected_cross_cutting: &mut Vec<CrossCuttingConcern>,
) -> Result<String, AgentError> {
    let concern: CrossCuttingConcern = serde_json::from_value(input.clone()).context(JsonSnafu)?;
    collected_cross_cutting.push(concern);
    Ok(format!(
        "Cross-cutting concern emitted. Total: {}",
        collected_cross_cutting.len()
    ))
}
