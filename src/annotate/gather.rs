use crate::error::{Result, ultragit_error};
use crate::git::{FileDiff, GitOps};
use snafu::ResultExt;
use std::path::PathBuf;

/// Author-provided context captured at commit time.
#[derive(Debug, Clone, Default)]
pub struct AuthorContext {
    pub task: Option<String>,
    pub reasoning: Option<String>,
    pub dependencies: Option<String>,
    pub tags: Vec<String>,
}

/// All the context needed for annotation, assembled before calling the agent.
#[derive(Debug, Clone)]
pub struct AnnotationContext {
    pub commit_sha: String,
    pub commit_message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: String,
    pub diffs: Vec<FileDiff>,
    pub author_context: Option<AuthorContext>,
}

/// Build the annotation context for a commit.
pub fn build_context(
    git_ops: &dyn GitOps,
    commit: &str,
) -> Result<AnnotationContext> {
    // Get commit metadata
    let info = git_ops.commit_info(commit).context(ultragit_error::GitSnafu)?;

    // Get file diffs
    let diffs = git_ops.diff(commit).context(ultragit_error::GitSnafu)?;

    // Gather author context from pending-context file and env vars
    let author_context = gather_author_context();

    Ok(AnnotationContext {
        commit_sha: info.sha,
        commit_message: info.message,
        author_name: info.author_name,
        author_email: info.author_email,
        timestamp: info.timestamp,
        diffs,
        author_context,
    })
}

/// Gather author context from pending-context.json and environment variables.
fn gather_author_context() -> Option<AuthorContext> {
    // Try reading pending context from .git/ultragit/pending-context.json
    let pending = read_pending_context_from_git_dir();

    // Also check environment variables
    let env_task = std::env::var("ULTRAGIT_TASK").ok().filter(|s| !s.is_empty());
    let env_reasoning = std::env::var("ULTRAGIT_REASONING").ok().filter(|s| !s.is_empty());
    let env_dependencies = std::env::var("ULTRAGIT_DEPENDENCIES").ok().filter(|s| !s.is_empty());
    let env_tags: Vec<String> = std::env::var("ULTRAGIT_TAGS")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    // Merge: pending context provides base, env vars override
    let mut ctx = pending
        .map(|p| p.to_author_context())
        .unwrap_or_default();

    if env_task.is_some() {
        ctx.task = env_task;
    }
    if env_reasoning.is_some() {
        ctx.reasoning = env_reasoning;
    }
    if env_dependencies.is_some() {
        ctx.dependencies = env_dependencies;
    }
    if !env_tags.is_empty() {
        ctx.tags = env_tags;
    }

    // Return None if everything is empty
    if ctx.task.is_none()
        && ctx.reasoning.is_none()
        && ctx.dependencies.is_none()
        && ctx.tags.is_empty()
    {
        None
    } else {
        Some(ctx)
    }
}

/// Try to read pending context from the .git directory.
fn read_pending_context_from_git_dir() -> Option<crate::hooks::PendingContext> {
    // Try to find .git directory by looking at current dir
    let git_dir = PathBuf::from(".git");
    if !git_dir.exists() {
        return None;
    }
    crate::hooks::read_pending_context(&git_dir).ok().flatten()
}
