use std::path::PathBuf;

use crate::error::chronicle_error::{GitSnafu, IoSnafu};
use crate::error::Result;
use crate::git::CliOps;
use crate::git::GitOps;
use crate::hooks::install_hooks;
use crate::sync::enable_sync;
use snafu::ResultExt;

use super::util::find_git_dir;

pub fn run(
    no_sync: bool,
    no_hooks: bool,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    // Find the git directory
    let git_dir = find_git_dir()?;

    // Create .git/chronicle/ directory
    let chronicle_dir = git_dir.join("chronicle");
    std::fs::create_dir_all(&chronicle_dir).context(IoSnafu)?;

    // Set up git config
    let repo_dir = git_dir.parent().unwrap_or(&git_dir).to_path_buf();
    let ops = CliOps::new(repo_dir.clone());

    ops.config_set("chronicle.enabled", "true")
        .context(GitSnafu)?;

    if let Some(ref p) = provider {
        ops.config_set("chronicle.provider", p).context(GitSnafu)?;
    }

    if let Some(ref m) = model {
        ops.config_set("chronicle.model", m).context(GitSnafu)?;
    }

    // Install hooks unless --no-hooks
    if !no_hooks {
        install_hooks(&git_dir)?;
        eprintln!("installed post-commit hook");
    }

    // Enable notes sync by default (push/fetch refspecs on origin)
    if !no_sync {
        ops.config_set("chronicle.sync", "true").context(GitSnafu)?;
        let remote = "origin";
        match enable_sync(&repo_dir, remote) {
            Ok(()) => eprintln!("notes sync enabled for remote '{remote}'"),
            Err(e) => eprintln!("warning: could not enable notes sync: {e}"),
        }
    }

    eprintln!("chronicle initialized in {}", chronicle_dir.display());

    // Check if global skills are installed
    if let Ok(home) = std::env::var("HOME") {
        let skills_dir = PathBuf::from(&home)
            .join(".claude")
            .join("skills")
            .join("chronicle");
        if !skills_dir.exists() {
            eprintln!();
            eprintln!("TIP: Run `git chronicle setup` to install Claude Code skills globally.");
        }
    }

    Ok(())
}
