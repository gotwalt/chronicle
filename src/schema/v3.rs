use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::LineRange;

// Re-export Provenance from v2 — shared between v2 and v3.
pub use super::v2::{Provenance, ProvenanceSource};

// ---------------------------------------------------------------------------
// Top-level Annotation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Annotation {
    pub schema: String,
    pub commit: String,
    pub timestamp: String,

    /// What this commit does and WHY this approach. Not a diff restatement.
    pub summary: String,

    /// Accumulated wisdom entries — dead ends, gotchas, insights, threads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wisdom: Vec<WisdomEntry>,

    /// How this annotation was created.
    pub provenance: Provenance,
}

impl Annotation {
    /// Validate the annotation for structural correctness.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "chronicle/v3" {
            return Err(format!("unsupported schema version: {}", self.schema));
        }
        if self.commit.is_empty() {
            return Err("commit SHA is empty".to_string());
        }
        if self.summary.is_empty() {
            return Err("summary is empty".to_string());
        }
        for (i, entry) in self.wisdom.iter().enumerate() {
            if let Err(e) = entry.validate() {
                return Err(format!("wisdom[{}]: {}", i, e));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wisdom Entries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct WisdomEntry {
    /// What kind of wisdom this captures.
    pub category: WisdomCategory,

    /// Free-form prose — what was learned, not what the code does.
    pub content: String,

    /// File this wisdom applies to. None = repo-wide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,

    /// Line range within the file. Only meaningful when `file` is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<LineRange>,
}

impl WisdomEntry {
    pub fn validate(&self) -> Result<(), String> {
        if self.content.is_empty() {
            return Err("content is empty".to_string());
        }
        if let Some(lines) = &self.lines {
            if lines.start > lines.end {
                return Err(format!(
                    "invalid line range: start ({}) > end ({})",
                    lines.start, lines.end
                ));
            }
        }
        if self.lines.is_some() && self.file.is_none() {
            return Err("lines specified without file".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WisdomCategory {
    /// Things tried and failed.
    DeadEnd,
    /// Non-obvious traps invisible in the code.
    Gotcha,
    /// Mental models, key relationships, architecture.
    Insight,
    /// Incomplete work, suspected better approaches.
    UnfinishedThread,
}

impl std::fmt::Display for WisdomCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeadEnd => write!(f, "dead_end"),
            Self::Gotcha => write!(f, "gotcha"),
            Self::Insight => write!(f, "insight"),
            Self::UnfinishedThread => write!(f, "unfinished_thread"),
        }
    }
}
