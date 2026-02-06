use std::path::PathBuf;

use crate::error::chronicle_error::{GitSnafu, IoSnafu, NotARepositorySnafu};
use crate::error::Result;
use crate::git::CliOps;
use crate::git::GitOps;
use crate::hooks::install_hooks;
use crate::sync::enable_sync;
use snafu::ResultExt;

pub fn run(no_sync: bool, no_hooks: bool, provider: Option<String>, model: Option<String>, backfill: bool) -> Result<()> {
    // Find the git directory
    let git_dir = find_git_dir()?;

    // Create .git/chronicle/ directory
    let chronicle_dir = git_dir.join("chronicle");
    std::fs::create_dir_all(&chronicle_dir).context(IoSnafu)?;

    // Set up git config
    let repo_dir = git_dir.parent().unwrap_or(&git_dir).to_path_buf();
    let ops = CliOps::new(repo_dir.clone());

    ops.config_set("chronicle.enabled", "true").context(GitSnafu)?;

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

    // --- Enhanced post-init checks ---

    // Count unannotated commits
    let unannotated = count_unannotated(&ops);
    if unannotated > 0 {
        eprintln!();
        eprintln!(
            "Found {} unannotated commits (of last 100). Run `git chronicle backfill --limit 20` to annotate recent history.",
            unannotated
        );
    }

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

    // Run backfill if requested
    if backfill {
        eprintln!();
        eprintln!("Running backfill (limit 20)...");
        if let Err(e) = crate::cli::backfill::run(20, false) {
            eprintln!("warning: backfill failed: {e}");
        }
    }

    Ok(())
}

/// Count unannotated commits in the last 100.
fn count_unannotated(ops: &dyn GitOps) -> usize {
    let output = match std::process::Command::new("git")
        .args(["log", "--format=%H", "-100"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };

    let shas: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut unannotated = 0;
    for sha in &shas {
        if let Ok(false) = ops.note_exists(sha) {
            unannotated += 1;
        }
    }
    unannotated
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
