pub mod filter;
pub mod gather;
pub mod live;
pub mod squash;
pub mod staging;

use crate::error::{chronicle_error, Result};
use crate::git::GitOps;
use crate::provider::LlmProvider;
use crate::schema::v2;
use snafu::ResultExt;

/// The main annotation entry point. Gathers context, checks filters,
/// runs the agent, and writes the annotation as a git note.
///
/// Produces v2 annotations (narrative-first).
pub fn run(
    git_ops: &dyn GitOps,
    provider: &dyn LlmProvider,
    commit: &str,
) -> Result<v2::Annotation> {
    // 1. Gather context
    let context = gather::build_context(git_ops, commit)?;

    // 2. Pre-LLM filter
    let decision = filter::pre_llm_filter(&context);

    // Collect files_changed from diffs
    let files_changed: Vec<String> = context.diffs.iter().map(|d| d.path.clone()).collect();

    let annotation = match decision {
        filter::FilterDecision::Skip(reason) => {
            tracing::info!("Skipping annotation: {}", reason);
            v2::Annotation {
                schema: "chronicle/v2".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                narrative: v2::Narrative {
                    summary: format!("Skipped: {reason}"),
                    motivation: None,
                    rejected_alternatives: Vec::new(),
                    follow_up: None,
                    files_changed,
                    sentiments: Vec::new(),
                },
                decisions: Vec::new(),
                markers: Vec::new(),
                effort: None,
                provenance: v2::Provenance {
                    source: v2::ProvenanceSource::Batch,
                    author: None,
                    derived_from: Vec::new(),
                    notes: Some(format!("Skipped: {reason}")),
                },
            }
        }
        filter::FilterDecision::Trivial(reason) => {
            tracing::info!("Trivial commit: {}", reason);
            let effort = context.author_context.as_ref().and_then(|ac| {
                ac.task.as_ref().map(|task| v2::EffortLink {
                    id: task.clone(),
                    description: task.clone(),
                    phase: v2::EffortPhase::InProgress,
                })
            });
            v2::Annotation {
                schema: "chronicle/v2".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                narrative: v2::Narrative {
                    summary: context.commit_message.clone(),
                    motivation: None,
                    rejected_alternatives: Vec::new(),
                    follow_up: None,
                    files_changed,
                    sentiments: Vec::new(),
                },
                decisions: Vec::new(),
                markers: Vec::new(),
                effort,
                provenance: v2::Provenance {
                    source: v2::ProvenanceSource::Batch,
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
            let mut narrative = collected.narrative.unwrap();

            // Auto-populate files_changed from diffs
            narrative.files_changed = files_changed;

            let effort = context.author_context.as_ref().and_then(|ac| {
                ac.task.as_ref().map(|task| v2::EffortLink {
                    id: task.clone(),
                    description: task.clone(),
                    phase: v2::EffortPhase::InProgress,
                })
            });

            v2::Annotation {
                schema: "chronicle/v2".to_string(),
                commit: context.commit_sha.clone(),
                timestamp: context.timestamp.clone(),
                narrative,
                decisions: collected.decisions,
                markers: collected.markers,
                effort,
                provenance: v2::Provenance {
                    source: v2::ProvenanceSource::Batch,
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
