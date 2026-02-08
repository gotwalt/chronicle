pub mod contracts;
pub mod decisions;
pub mod deps;
pub mod history;
pub mod lookup;
pub(crate) mod matching;
pub mod retrieve;
pub mod sentiments;
pub mod staleness;
pub mod summary;

use crate::error::{ChronicleError, Result};
use crate::git::GitOps;
use crate::schema::common::LineRange;
use crate::schema::v2;

/// Query parameters for reading annotations.
#[derive(Debug, Clone)]
pub struct ReadQuery {
    pub file: String,
    pub anchor: Option<String>,
    pub lines: Option<LineRange>,
}

/// Result of a read query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadResult {
    pub file: String,
    pub annotations: Vec<MatchedAnnotation>,
}

/// A v2-native annotation matched to a specific commit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchedAnnotation {
    pub commit: String,
    pub timestamp: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<v2::CodeMarker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<v2::Decision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    pub provenance: String,
}

/// Execute a read query against the repository.
pub fn execute(git: &dyn GitOps, query: &ReadQuery) -> Result<ReadResult> {
    let annotations =
        retrieve::retrieve_annotations(git, query).map_err(|e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    Ok(ReadResult {
        file: query.file.clone(),
        annotations,
    })
}
