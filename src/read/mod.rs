pub mod retrieve;
pub mod deps;
pub mod history;
pub mod summary;

use crate::error::{Result, UltragitError};
use crate::git::GitOps;
use crate::schema::annotation::{LineRange, RegionAnnotation};

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
    pub regions: Vec<MatchedRegion>,
}

/// A region annotation matched to a specific commit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchedRegion {
    pub commit: String,
    pub timestamp: String,
    pub region: RegionAnnotation,
    pub summary: String,
}

/// Execute a read query against the repository.
pub fn execute(git: &dyn GitOps, query: &ReadQuery) -> Result<ReadResult> {
    let regions = retrieve::retrieve_regions(git, query).map_err(|e| UltragitError::Git {
        source: e,
        location: snafu::Location::default(),
    })?;

    Ok(ReadResult {
        file: query.file.clone(),
        regions,
    })
}
