use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::annotate::gather::AuthorContext;
use crate::error::ultragit_error::{IoSnafu, JsonSnafu};
use crate::error::Result;
use snafu::ResultExt;

const HOOK_BEGIN_MARKER: &str = "# --- ultragit hook begin ---";
const HOOK_END_MARKER: &str = "# --- ultragit hook end ---";

const HOOK_SCRIPT: &str = r#"# --- ultragit hook begin ---
# Installed by ultragit. Do not edit between these markers.
if command -v ultragit >/dev/null 2>&1; then
    ultragit annotate --commit HEAD --sync &
fi
# --- ultragit hook end ---"#;

/// Pending context stored in .git/ultragit/pending-context.json.
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
    git_dir.join("ultragit").join("pending-context.json")
}

/// Read pending context from .git/ultragit/pending-context.json.
pub fn read_pending_context(git_dir: &Path) -> Result<Option<PendingContext>> {
    let path = pending_context_path(git_dir);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).context(IoSnafu)?;
    let ctx: PendingContext = serde_json::from_str(&contents).context(JsonSnafu)?;
    Ok(Some(ctx))
}

/// Write pending context to .git/ultragit/pending-context.json.
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

/// Install the post-commit hook with ultragit markers.
pub fn install_hooks(git_dir: &Path) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir).context(IoSnafu)?;

    let hook_path = hooks_dir.join("post-commit");

    let existing = if hook_path.exists() {
        std::fs::read_to_string(&hook_path).context(IoSnafu)?
    } else {
        String::new()
    };

    // Check if ultragit section already exists; if so, replace it
    let new_content = if existing.contains(HOOK_BEGIN_MARKER) {
        // Replace existing ultragit section
        let mut result = String::new();
        let mut in_section = false;
        for line in existing.lines() {
            if line.contains(HOOK_BEGIN_MARKER) {
                in_section = true;
                result.push_str(HOOK_SCRIPT);
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
        // New hook file
        format!("#!/bin/sh\n{HOOK_SCRIPT}\n")
    } else {
        // Append to existing hook, chaining
        let mut content = existing.clone();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(HOOK_SCRIPT);
        content.push('\n');
        content
    };

    std::fs::write(&hook_path, &new_content).context(IoSnafu)?;

    // chmod +x
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).context(IoSnafu)?;
    }

    Ok(())
}
