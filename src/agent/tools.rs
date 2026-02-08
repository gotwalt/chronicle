use std::path::Path;

use snafu::ResultExt;

use crate::annotate::gather::AnnotationContext;
use crate::error::agent_error::{GitSnafu, JsonSnafu};
use crate::error::AgentError;
use crate::git::{GitOps, HunkLine};
use crate::provider::ToolDefinition;
use crate::schema::v2::{CodeMarker, Decision, Narrative};

/// Collected output from the agent's emit tools.
#[derive(Debug, Default)]
pub struct CollectedOutput {
    pub narrative: Option<Narrative>,
    pub decisions: Vec<Decision>,
    pub markers: Vec<CodeMarker>,
}

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
            name: "get_commit_info".to_string(),
            description: "Get commit metadata: SHA, message, author, timestamp.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "emit_narrative".to_string(),
            description: "Emit the commit-level narrative (REQUIRED, call exactly once). \
                Tell the story of this commit: what it does, why this approach, \
                what was considered and rejected."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "What this commit does and WHY this approach. Not a diff restatement."
                    },
                    "motivation": {
                        "type": "string",
                        "description": "What triggered this change? User request, bug, planned work?"
                    },
                    "rejected_alternatives": {
                        "type": "array",
                        "description": "Approaches that were considered and rejected",
                        "items": {
                            "type": "object",
                            "properties": {
                                "approach": { "type": "string" },
                                "reason": { "type": "string" }
                            },
                            "required": ["approach", "reason"]
                        }
                    },
                    "follow_up": {
                        "type": "string",
                        "description": "Expected follow-up work, if any. Omit if this is complete."
                    }
                },
                "required": ["summary"]
            }),
        },
        ToolDefinition {
            name: "emit_decision".to_string(),
            description: "Emit a design or architectural decision made in this commit.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "what": { "type": "string", "description": "What was decided" },
                    "why": { "type": "string", "description": "Why this decision was made" },
                    "stability": {
                        "type": "string",
                        "enum": ["permanent", "provisional", "experimental"],
                        "description": "How stable is this decision?"
                    },
                    "revisit_when": {
                        "type": "string",
                        "description": "When should this decision be reconsidered?"
                    },
                    "scope": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Files/modules this decision applies to"
                    }
                },
                "required": ["what", "why", "stability"]
            }),
        },
        ToolDefinition {
            name: "emit_marker".to_string(),
            description: "Emit a code marker for genuinely non-obvious behavior. \
                Only use for contracts, hazards, dependencies, or unstable code. \
                Do NOT emit a marker for every function."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "anchor": {
                        "type": "object",
                        "description": "Optional AST anchor. Omit for file-level markers.",
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
                    "kind": {
                        "type": "string",
                        "enum": ["contract", "hazard", "dependency", "unstable"],
                        "description": "Type of marker"
                    },
                    "description": {
                        "type": "string",
                        "description": "For contract/hazard/unstable: what the behavior or concern is"
                    },
                    "source": {
                        "type": "string",
                        "enum": ["author", "inferred"],
                        "description": "For contracts: whether the author stated this or it was inferred"
                    },
                    "target_file": { "type": "string", "description": "For dependency: the file depended on" },
                    "target_anchor": { "type": "string", "description": "For dependency: the anchor depended on" },
                    "assumption": { "type": "string", "description": "For dependency: what is assumed" },
                    "revisit_when": { "type": "string", "description": "For unstable: when to revisit" }
                },
                "required": ["file", "kind"]
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
    collected: &mut CollectedOutput,
) -> Result<String, AgentError> {
    match name {
        "get_diff" => dispatch_get_diff(context),
        "get_file_content" => dispatch_get_file_content(input, git_ops, context),
        "get_commit_info" => dispatch_get_commit_info(context),
        "emit_narrative" => dispatch_emit_narrative(input, collected),
        "emit_decision" => dispatch_emit_decision(input, collected),
        "emit_marker" => dispatch_emit_marker(input, collected),
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

fn dispatch_emit_narrative(
    input: &serde_json::Value,
    collected: &mut CollectedOutput,
) -> Result<String, AgentError> {
    let narrative: Narrative = serde_json::from_value(input.clone()).context(JsonSnafu)?;
    collected.narrative = Some(narrative);
    Ok("Narrative emitted.".to_string())
}

fn dispatch_emit_decision(
    input: &serde_json::Value,
    collected: &mut CollectedOutput,
) -> Result<String, AgentError> {
    let decision: Decision = serde_json::from_value(input.clone()).context(JsonSnafu)?;
    collected.decisions.push(decision);
    Ok(format!(
        "Decision emitted. Total decisions: {}",
        collected.decisions.len()
    ))
}

fn dispatch_emit_marker(
    input: &serde_json::Value,
    collected: &mut CollectedOutput,
) -> Result<String, AgentError> {
    // The agent emits a flat JSON object with `kind` as a string discriminator
    // and kind-specific fields at the top level. We manually construct the
    // CodeMarker since the serde format for MarkerKind uses `tag = "type"`.
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v2::{ContractSource, MarkerKind};

    let file = input
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind_str = input
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("hazard");

    let anchor = input.get("anchor").and_then(|v| {
        let unit_type = v.get("unit_type")?.as_str()?.to_string();
        let name = v.get("name")?.as_str()?.to_string();
        let signature = v
            .get("signature")
            .and_then(|s| s.as_str())
            .map(String::from);
        Some(AstAnchor {
            unit_type,
            name,
            signature,
        })
    });

    let lines = input.get("lines").and_then(|v| {
        let start = v.get("start")?.as_u64()? as u32;
        let end = v.get("end")?.as_u64()? as u32;
        Some(LineRange { start, end })
    });

    let marker_kind = match kind_str {
        "contract" => {
            let description = input
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let source = match input.get("source").and_then(|v| v.as_str()) {
                Some("author") => ContractSource::Author,
                _ => ContractSource::Inferred,
            };
            MarkerKind::Contract {
                description,
                source,
            }
        }
        "hazard" => {
            let description = input
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            MarkerKind::Hazard { description }
        }
        "dependency" => {
            let target_file = input
                .get("target_file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let target_anchor = input
                .get("target_anchor")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let assumption = input
                .get("assumption")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            MarkerKind::Dependency {
                target_file,
                target_anchor,
                assumption,
            }
        }
        "unstable" => {
            let description = input
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let revisit_when = input
                .get("revisit_when")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            MarkerKind::Unstable {
                description,
                revisit_when,
            }
        }
        _ => {
            return Err(AgentError::InvalidAnnotation {
                message: format!("Unknown marker kind: {kind_str}"),
                location: snafu::Location::default(),
            });
        }
    };

    let marker = CodeMarker {
        file,
        anchor,
        lines,
        kind: marker_kind,
    };
    collected.markers.push(marker);
    Ok(format!(
        "Marker emitted. Total markers: {}",
        collected.markers.len()
    ))
}
