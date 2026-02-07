pub mod cli_ops;
pub mod diff;

pub use cli_ops::CliOps;
pub use diff::{DiffStatus, FileDiff, Hunk, HunkLine};

use crate::error::GitError;
use std::path::Path;

/// Metadata about a commit.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: String,
    pub parent_shas: Vec<String>,
}

/// Abstraction over git operations. MVP implements CliOps (shelling out to git).
/// GixOps (pure Rust via gitoxide) will be added later.
pub trait GitOps: Send + Sync {
    /// Get the diff for a single commit.
    fn diff(&self, commit: &str) -> Result<Vec<FileDiff>, GitError>;

    /// Read a git note from the chronicle notes ref.
    fn note_read(&self, commit: &str) -> Result<Option<String>, GitError>;

    /// Write a git note to the chronicle notes ref (overwrites existing).
    fn note_write(&self, commit: &str, content: &str) -> Result<(), GitError>;

    /// Check if a note exists for a commit.
    fn note_exists(&self, commit: &str) -> Result<bool, GitError>;

    /// Read a file at a specific commit.
    fn file_at_commit(&self, path: &Path, commit: &str) -> Result<String, GitError>;

    /// Get commit metadata.
    fn commit_info(&self, commit: &str) -> Result<CommitInfo, GitError>;

    /// Resolve a ref (branch name, HEAD, etc.) to a SHA.
    fn resolve_ref(&self, refspec: &str) -> Result<String, GitError>;

    /// Read a git config value.
    fn config_get(&self, key: &str) -> Result<Option<String>, GitError>;

    /// Set a git config value.
    fn config_set(&self, key: &str, value: &str) -> Result<(), GitError>;

    /// List commit SHAs that touched a file (newest first), following renames.
    fn log_for_file(&self, path: &str) -> Result<Vec<String>, GitError>;

    /// List commit SHAs that have chronicle notes (newest first), up to `limit`.
    fn list_annotated_commits(&self, limit: u32) -> Result<Vec<String>, GitError>;
}
