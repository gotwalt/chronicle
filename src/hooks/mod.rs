pub mod post_rewrite;
pub mod prepare_commit_msg;

use crate::error::chronicle_error::IoSnafu;
use crate::error::Result;
use snafu::ResultExt;
use std::path::Path;

const HOOK_BEGIN_MARKER: &str = "# --- chronicle hook begin ---";
const HOOK_END_MARKER: &str = "# --- chronicle hook end ---";

const POST_COMMIT_SCRIPT: &str = r#"# --- chronicle hook begin ---
# Installed by chronicle. Do not edit between these markers.
if command -v git-chronicle >/dev/null 2>&1; then
    git-chronicle annotate --auto --commit HEAD &
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
