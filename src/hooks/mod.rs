pub mod post_rewrite;
pub mod prepare_commit_msg;

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::annotate::gather::AuthorContext;
use crate::error::chronicle_error::{IoSnafu, JsonSnafu};
use crate::error::Result;
use snafu::ResultExt;

const HOOK_BEGIN_MARKER: &str = "# --- chronicle hook begin ---";
const HOOK_END_MARKER: &str = "# --- chronicle hook end ---";

const POST_COMMIT_SCRIPT: &str = r#"# --- chronicle hook begin ---
# Installed by chronicle. Do not edit between these markers.
if command -v git-chronicle >/dev/null 2>&1; then
    git-chronicle annotate --commit HEAD --sync &
fi
# --- chronicle hook end ---"#;

const PREPARE_COMMIT_MSG_SCRIPT: &str = r#"# --- chronicle hook begin ---
# Installed by chronicle. Do not edit between these markers.
if command -v git-chronicle >/dev/null 2>&1; then
    git-chronicle hook prepare-commit-msg "$@"
fi
# --- chronicle hook end ---"#;

const POST_REWRITE_SCRIPT: &str = r#"# --- chronicle hook begin ---
# Installed by chronicle. Do not edit between these markers.
if command -v git-chronicle >/dev/null 2>&1; then
    git-chronicle hook post-rewrite "$@"
fi
# --- chronicle hook end ---"#;

/// Pending context stored in .git/chronicle/pending-context.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl PendingContext {
    pub fn to_author_context(&self) -> AuthorContext {
        AuthorContext {
            task: self.task.clone(),
            reasoning: self.reasoning.clone(),
            dependencies: self.dependencies.clone(),
            tags: self.tags.clone(),
        }
    }
}

fn pending_context_path(git_dir: &Path) -> std::path::PathBuf {
    git_dir.join("chronicle").join("pending-context.json")
}

/// Read pending context from .git/chronicle/pending-context.json.
pub fn read_pending_context(git_dir: &Path) -> Result<Option<PendingContext>> {
    let path = pending_context_path(git_dir);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).context(IoSnafu)?;
    let ctx: PendingContext = serde_json::from_str(&contents).context(JsonSnafu)?;
    Ok(Some(ctx))
}

/// Write pending context to .git/chronicle/pending-context.json.
pub fn write_pending_context(git_dir: &Path, ctx: &PendingContext) -> Result<()> {
    let path = pending_context_path(git_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context(IoSnafu)?;
    }
    let json = serde_json::to_string_pretty(ctx).context(JsonSnafu)?;
    std::fs::write(&path, json).context(IoSnafu)?;
    Ok(())
}

/// Delete the pending context file.
pub fn delete_pending_context(git_dir: &Path) -> Result<()> {
    let path = pending_context_path(git_dir);
    if path.exists() {
        std::fs::remove_file(&path).context(IoSnafu)?;
    }
    Ok(())
}

/// Install a single hook script into the hooks directory.
fn install_single_hook(hooks_dir: &Path, hook_name: &str, script: &str) -> Result<()> {
    let hook_path = hooks_dir.join(hook_name);

    let existing = if hook_path.exists() {
        std::fs::read_to_string(&hook_path).context(IoSnafu)?
    } else {
        String::new()
    };

    let new_content = if existing.contains(HOOK_BEGIN_MARKER) {
        // Replace existing chronicle section
        let mut result = String::new();
        let mut in_section = false;
        for line in existing.lines() {
            if line.contains(HOOK_BEGIN_MARKER) {
                in_section = true;
                result.push_str(script);
                result.push('\n');
                continue;
            }
            if line.contains(HOOK_END_MARKER) {
                in_section = false;
                continue;
            }
            if !in_section {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    } else if existing.is_empty() {
        format!("#!/bin/sh\n{script}\n")
    } else {
        let mut content = existing.clone();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(script);
        content.push('\n');
        content
    };

    std::fs::write(&hook_path, &new_content).context(IoSnafu)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).context(IoSnafu)?;
    }

    Ok(())
}

/// Install all chronicle hooks: post-commit, prepare-commit-msg, and post-rewrite.
pub fn install_hooks(git_dir: &Path) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir).context(IoSnafu)?;

    install_single_hook(&hooks_dir, "post-commit", POST_COMMIT_SCRIPT)?;
    install_single_hook(&hooks_dir, "prepare-commit-msg", PREPARE_COMMIT_MSG_SCRIPT)?;
    install_single_hook(&hooks_dir, "post-rewrite", POST_REWRITE_SCRIPT)?;

    Ok(())
}
