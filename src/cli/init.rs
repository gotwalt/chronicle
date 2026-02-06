use std::path::PathBuf;

use crate::error::ultragit_error::{GitSnafu, IoSnafu, NotARepositorySnafu};
use crate::error::Result;
use crate::git::CliOps;
use crate::git::GitOps;
use crate::hooks::install_hooks;
use snafu::ResultExt;

pub fn run(sync: bool, no_hooks: bool, provider: Option<String>, model: Option<String>) -> Result<()> {
    // Find the git directory
    let git_dir = find_git_dir()?;

    // Create .git/ultragit/ directory
    let ultragit_dir = git_dir.join("ultragit");
    std::fs::create_dir_all(&ultragit_dir).context(IoSnafu)?;

    // Set up git config
    let repo_dir = git_dir.parent().unwrap_or(&git_dir).to_path_buf();
    let ops = CliOps::new(repo_dir);

    ops.config_set("ultragit.enabled", "true").context(GitSnafu)?;

    if sync {
        ops.config_set("ultragit.sync", "true").context(GitSnafu)?;
    }

    if let Some(ref p) = provider {
        ops.config_set("ultragit.provider", p).context(GitSnafu)?;
    }

    if let Some(ref m) = model {
        ops.config_set("ultragit.model", m).context(GitSnafu)?;
    }

    // Install hooks unless --no-hooks
    if !no_hooks {
        install_hooks(&git_dir)?;
        eprintln!("installed post-commit hook");
    }

    // Check for API key
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("warning: ANTHROPIC_API_KEY is not set. Set it before running annotations.");
    }

    eprintln!("ultragit initialized in {}", ultragit_dir.display());
    Ok(())
}

/// Find the .git directory by running `git rev-parse --git-dir` or walking up.
fn find_git_dir() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .context(IoSnafu)?;

    if output.status.success() {
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = PathBuf::from(&dir);
        // Make absolute if relative
        if path.is_relative() {
            let cwd = std::env::current_dir().context(IoSnafu)?;
            Ok(cwd.join(path))
        } else {
            Ok(path)
        }
    } else {
        let cwd = std::env::current_dir().context(IoSnafu)?;
        Err(NotARepositorySnafu { path: cwd }.build())
    }
}
