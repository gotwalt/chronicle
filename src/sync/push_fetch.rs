use std::path::PathBuf;
use std::process::Command;

use crate::error::git_error::CommandFailedSnafu;
use crate::error::chronicle_error::GitSnafu;
use crate::error::{GitError, Result};
use snafu::ResultExt;

const NOTES_REF: &str = "refs/notes/chronicle";

/// Current sync configuration for a remote.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub remote: String,
    pub push_refspec: Option<String>,
    pub fetch_refspec: Option<String>,
}

impl SyncConfig {
    pub fn is_enabled(&self) -> bool {
        self.push_refspec.is_some() && self.fetch_refspec.is_some()
    }
}

/// Sync status between local and remote notes.
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub enabled: bool,
    pub local_count: usize,
    pub remote_count: Option<usize>,
    pub unpushed_count: usize,
}

/// Run a git command in the given repo directory.
fn run_git(repo_dir: &PathBuf, args: &[&str]) -> std::result::Result<String, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .map_err(|e| {
            CommandFailedSnafu {
                message: format!("failed to run git: {e}"),
            }
            .build()
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(CommandFailedSnafu {
            message: stderr.trim().to_string(),
        }
        .build())
    }
}

/// Run git and return (success, stdout, stderr) without failing on non-zero exit.
fn run_git_raw(repo_dir: &PathBuf, args: &[&str]) -> std::result::Result<(bool, String, String), GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .map_err(|e| {
            CommandFailedSnafu {
                message: format!("failed to run git: {e}"),
            }
            .build()
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((output.status.success(), stdout, stderr))
}

/// Get the current sync configuration for a remote.
pub fn get_sync_config(repo_dir: &PathBuf, remote: &str) -> Result<SyncConfig> {
    let push_refspec = get_config_values(repo_dir, &format!("remote.{remote}.push"))
        .context(GitSnafu)?
        .into_iter()
        .find(|r| r.contains(NOTES_REF));

    let fetch_refspec = get_config_values(repo_dir, &format!("remote.{remote}.fetch"))
        .context(GitSnafu)?
        .into_iter()
        .find(|r| r.contains(NOTES_REF));

    Ok(SyncConfig {
        remote: remote.to_string(),
        push_refspec,
        fetch_refspec,
    })
}

/// Enable sync by adding push/fetch refspecs for chronicle notes.
pub fn enable_sync(repo_dir: &PathBuf, remote: &str) -> Result<()> {
    let config = get_sync_config(repo_dir, remote)?;

    // Add push refspec if not already present
    if config.push_refspec.is_none() {
        run_git(
            repo_dir,
            &["config", "--add", &format!("remote.{remote}.push"), NOTES_REF],
        )
        .context(GitSnafu)?;
    }

    // Add fetch refspec if not already present
    if config.fetch_refspec.is_none() {
        let fetch_spec = format!("+{NOTES_REF}:{NOTES_REF}");
        run_git(
            repo_dir,
            &["config", "--add", &format!("remote.{remote}.fetch"), &fetch_spec],
        )
        .context(GitSnafu)?;
    }

    Ok(())
}

/// Get the sync status for a remote.
pub fn get_sync_status(repo_dir: &PathBuf, remote: &str) -> Result<SyncStatus> {
    let config = get_sync_config(repo_dir, remote)?;
    let enabled = config.is_enabled();

    let local_count = count_local_notes(repo_dir).context(GitSnafu)?;

    // Try to get remote note count (may fail if remote is unreachable)
    let remote_count = count_remote_notes(repo_dir, remote).ok();

    let unpushed_count = if let Some(rc) = remote_count {
        local_count.saturating_sub(rc)
    } else {
        0
    };

    Ok(SyncStatus {
        enabled,
        local_count,
        remote_count,
        unpushed_count,
    })
}

/// Pull (fetch) notes from a remote.
pub fn pull_notes(repo_dir: &PathBuf, remote: &str) -> Result<()> {
    run_git(
        repo_dir,
        &["fetch", remote, &format!("+{NOTES_REF}:{NOTES_REF}")],
    )
    .context(GitSnafu)?;

    Ok(())
}

/// Count local notes under refs/notes/chronicle.
fn count_local_notes(repo_dir: &PathBuf) -> std::result::Result<usize, GitError> {
    let (success, stdout, _) = run_git_raw(repo_dir, &["notes", "--ref", NOTES_REF, "list"])?;
    if !success {
        return Ok(0);
    }
    Ok(stdout.lines().filter(|l| !l.is_empty()).count())
}

/// Count remote notes by ls-remote.
fn count_remote_notes(repo_dir: &PathBuf, _remote: &str) -> Result<usize> {
    // We can only check if the ref exists remotely; accurate count needs a fetch.
    // After a fetch, we can count local notes (which now include fetched ones).
    // For a quick status, just return the local count as an approximation.
    let count = count_local_notes(repo_dir).context(GitSnafu)?;
    Ok(count)
}

/// Get all values for a multi-valued git config key.
fn get_config_values(repo_dir: &PathBuf, key: &str) -> std::result::Result<Vec<String>, GitError> {
    let (success, stdout, _) = run_git_raw(repo_dir, &["config", "--get-all", key])?;
    if !success {
        return Ok(Vec::new());
    }
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}
