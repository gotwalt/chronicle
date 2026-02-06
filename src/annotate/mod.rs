pub mod gather;
pub mod filter;
pub mod squash;

use crate::error::{Result, chronicle_error};
use crate::git::GitOps;
use crate::provider::LlmProvider;
use crate::schema::{Annotation, ContextLevel};
use snafu::ResultExt;

/// The main annotation entry point. Gathers context, checks filters,
/// runs the agent, and writes the annotation as a git note.
pub fn run(
    git_ops: &dyn GitOps,
    provider: &dyn LlmProvider,
    commit: &str,
    _sync: bool,
) -> Result<Annotation> {
    // 1. Gather context
    let context = gather::build_context(git_ops, commit)?;

    // 2. Pre-LLM filter
    let decision = filter::pre_llm_filter(&context);

    let annotation = match decision {
        filter::FilterDecision::Skip(reason) => {
            tracing::info!("Skipping annotation: {}", reason);
            // Return a minimal annotation with just the summary
            Annotation::new_initial(
                context.commit_sha.clone(),
                format!("Skipped: {}", reason),
                ContextLevel::Inferred,
            )
        }
        filter::FilterDecision::Trivial(reason) => {
            tracing::info!("Trivial commit: {}", reason);
            // Build a minimal annotation without calling LLM
            let mut ann = Annotation::new_initial(
                context.commit_sha.clone(),
                context.commit_message.clone(),
                ContextLevel::Inferred,
            );
            // Carry over task from author context if present
            if let Some(ref author_ctx) = context.author_context {
                ann.task = author_ctx.task.clone();
            }
            ann
        }
        filter::FilterDecision::Annotate => {
            // Call the agent loop for full LLM annotation
            let (regions, cross_cutting, summary) =
                crate::agent::run_agent_loop(provider, git_ops, &context)
                    .context(chronicle_error::AgentSnafu)?;

            let context_level = if context.author_context.is_some() {
                ContextLevel::Enhanced
            } else {
                ContextLevel::Inferred
            };

            let mut ann = Annotation::new_initial(
                context.commit_sha.clone(),
                summary,
                context_level,
            );
            ann.regions = regions;
            ann.cross_cutting = cross_cutting;
            // Carry over task from author context if present
            if let Some(ref author_ctx) = context.author_context {
                ann.task = author_ctx.task.clone();
            }
            ann
        }
    };

    // 3. Serialize and write as git note
    let json = serde_json::to_string_pretty(&annotation)
        .context(chronicle_error::JsonSnafu)?;
    git_ops
        .note_write(commit, &json)
        .context(chronicle_error::GitSnafu)?;

    Ok(annotation)
}
