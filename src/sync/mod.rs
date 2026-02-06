pub mod push_fetch;

pub use push_fetch::{SyncConfig, SyncStatus, enable_sync, get_sync_config, get_sync_status, pull_notes};

/// Merge strategy for conflicting notes on the same commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesMergeStrategy {
    /// Keep local version on conflict.
    Ours,
    /// Keep remote version on conflict.
    Theirs,
    /// JSON-level merge of annotation content (default).
    Union,
}

impl std::str::FromStr for NotesMergeStrategy {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ours" => Ok(Self::Ours),
            "theirs" => Ok(Self::Theirs),
            "union" => Ok(Self::Union),
            other => Err(format!("unknown merge strategy: {other}")),
        }
    }
}

impl std::fmt::Display for NotesMergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ours => write!(f, "ours"),
            Self::Theirs => write!(f, "theirs"),
            Self::Union => write!(f, "union"),
        }
    }
}
