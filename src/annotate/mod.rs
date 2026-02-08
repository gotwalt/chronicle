pub mod filter;
pub mod gather;
pub mod live;
pub mod squash;
pub mod staging;

use crate::error::{chronicle_error, Result};
use crate::git::GitOps;
use crate::provider::LlmProvider;
use crate::schema::{v2, v3};
use snafu::ResultExt;

/// The main annotation entry point. Gathers context, checks filters,
/// runs the agent, and writes the annotation as a git note.
///
/// Produces v3 annotations (wisdom-first). The agent still returns v2-shaped
/// data internally, which is converted to v3 before writing.
pub fn run(
    git_ops: &dyn GitOps,
    provider: &dyn LlmProvider,
    commit: &str,
) -> Result<v3::Annotation> {
    // 1. Gather context
    let context = gather::build_context(git_ops, commit)?;

    // 2. Pre-LLM filter
    let decision = filter::pre_llm_filter(&context);

    let annotation = match decision {
        filter::FilterDecision::Skip(reason) => {
            tracing::info!("Skipping annotation: {}", reason);
            v3::Annotation {
                schema: "chronicle/v3".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                summary: format!("Skipped: {reason}"),
                wisdom: Vec::new(),
                provenance: v3::Provenance {
                    source: v3::ProvenanceSource::Batch,
                    author: None,
                    derived_from: Vec::new(),
                    notes: Some(format!("Skipped: {reason}")),
                },
            }
        }
        filter::FilterDecision::Trivial(reason) => {
            tracing::info!("Trivial commit: {}", reason);
            v3::Annotation {
                schema: "chronicle/v3".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                summary: context.commit_message.clone(),
                wisdom: Vec::new(),
                provenance: v3::Provenance {
                    source: v3::ProvenanceSource::Batch,
                    author: None,
                    derived_from: Vec::new(),
                    notes: Some(format!("Trivial: {reason}")),
                },
            }
        }
        filter::FilterDecision::Annotate => {
            // Call the agent loop for full LLM annotation
            let (collected, _summary) = crate::agent::run_agent_loop(provider, git_ops, &context)
                .context(chronicle_error::AgentSnafu)?;

            // The narrative is required (agent loop guarantees it's Some)
            let narrative = collected.narrative.unwrap();

            // Convert agent output (v2 shapes) to v3 wisdom entries
            let mut wisdom = Vec::new();

            // Convert markers to wisdom entries
            for marker in &collected.markers {
                let (category, content) = match &marker.kind {
                    v2::MarkerKind::Contract { description, .. } => {
                        (v3::WisdomCategory::Gotcha, description.clone())
                    }
                    v2::MarkerKind::Hazard { description } => {
                        (v3::WisdomCategory::Gotcha, description.clone())
                    }
                    v2::MarkerKind::Dependency {
                        target_file,
                        target_anchor,
                        assumption,
                    } => (
                        v3::WisdomCategory::Insight,
                        format!("Depends on {target_file}:{target_anchor} \u{2014} {assumption}"),
                    ),
                    v2::MarkerKind::Unstable { description, .. } => {
                        (v3::WisdomCategory::UnfinishedThread, description.clone())
                    }
                    v2::MarkerKind::Security { description } => {
                        (v3::WisdomCategory::Gotcha, description.clone())
                    }
                    v2::MarkerKind::Performance { description } => {
                        (v3::WisdomCategory::Gotcha, description.clone())
                    }
                    v2::MarkerKind::Deprecated { description, .. } => {
                        (v3::WisdomCategory::UnfinishedThread, description.clone())
                    }
                    v2::MarkerKind::TechDebt { description } => {
                        (v3::WisdomCategory::UnfinishedThread, description.clone())
                    }
                    v2::MarkerKind::TestCoverage { description } => {
                        (v3::WisdomCategory::Insight, description.clone())
                    }
                };
                wisdom.push(v3::WisdomEntry {
                    category,
                    content,
                    file: Some(marker.file.clone()),
                    lines: marker.lines,
                });
            }

            // Convert decisions to wisdom entries
            for decision in &collected.decisions {
                let file = decision
                    .scope
                    .first()
                    .map(|s| s.split(':').next().unwrap_or(s).to_string());
                wisdom.push(v3::WisdomEntry {
                    category: v3::WisdomCategory::Insight,
                    content: format!("{}: {}", decision.what, decision.why),
                    file,
                    lines: None,
                });
            }

            // Convert rejected alternatives to dead_end entries
            for ra in &narrative.rejected_alternatives {
                let content = if ra.reason.is_empty() {
                    ra.approach.clone()
                } else {
                    format!("{}: {}", ra.approach, ra.reason)
                };
                wisdom.push(v3::WisdomEntry {
                    category: v3::WisdomCategory::DeadEnd,
                    content,
                    file: None,
                    lines: None,
                });
            }

            v3::Annotation {
                schema: "chronicle/v3".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                summary: narrative.summary,
                wisdom,
                provenance: v3::Provenance {
                    source: v3::ProvenanceSource::Batch,
                    author: None,
                    derived_from: Vec::new(),
                    notes: None,
                },
            }
        }
    };

    // 3. Serialize and write as git note
    let json = serde_json::to_string_pretty(&annotation).context(chronicle_error::JsonSnafu)?;
    git_ops
        .note_write(commit, &json)
        .context(chronicle_error::GitSnafu)?;

    Ok(annotation)
}
