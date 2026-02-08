use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{AstAnchor, LineRange};

// ---------------------------------------------------------------------------
// Top-level Annotation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Annotation {
    pub schema: String,
    pub commit: String,
    pub timestamp: String,

    /// The narrative (commit-level, always present).
    pub narrative: Narrative,

    /// Design decisions (zero or more).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,

    /// Code-level markers (optional, only where valuable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<CodeMarker>,

    /// Link to broader effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortLink>,

    /// How this annotation was created.
    pub provenance: Provenance,
}

impl Annotation {
    /// Validate the annotation for structural correctness.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "chronicle/v2" {
            return Err(format!("unsupported schema version: {}", self.schema));
        }
        if self.commit.is_empty() {
            return Err("commit SHA is empty".to_string());
        }
        if self.narrative.summary.is_empty() {
            return Err("narrative summary is empty".to_string());
        }
        for (i, marker) in self.markers.iter().enumerate() {
            if let Err(e) = marker.validate() {
                return Err(format!("marker[{}]: {}", i, e));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Narrative
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Narrative {
    /// What this commit does and WHY this approach. Not a diff restatement.
    pub summary: String,

    /// What triggered this change? User request, bug, planned work?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivation: Option<String>,

    /// What alternatives were considered and rejected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_alternatives: Vec<RejectedAlternative>,

    /// Expected follow-up. None = this is complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,

    /// Files touched (auto-populated from diff for indexing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_changed: Vec<String>,

    /// Agent intuitions: worries, hunches, confidence, unease.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sentiments: Vec<Sentiment>,
}

/// An agent sentiment — an intuition about the work that isn't captured
/// by facts alone. Feelings resist rigid categorization, so `feeling` is
/// a free string.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Sentiment {
    /// e.g. "worry", "confidence", "uncertainty", "pride", "unease",
    /// "curiosity", "frustration", "surprise", "doubt"
    pub feeling: String,
    /// What specifically and why.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RejectedAlternative {
    pub approach: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    /// What was decided.
    pub what: String,
    /// Why.
    pub why: String,
    /// How stable is this decision.
    pub stability: Stability,
    /// When to revisit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revisit_when: Option<String>,
    /// Files/modules this applies to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Permanent,
    Provisional,
    Experimental,
}

// ---------------------------------------------------------------------------
// Code Markers (replaces RegionAnnotation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeMarker {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AstAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<LineRange>,
    pub kind: MarkerKind,
}

impl CodeMarker {
    pub fn validate(&self) -> Result<(), String> {
        if self.file.is_empty() {
            return Err("file is empty".to_string());
        }
        if let Some(lines) = &self.lines {
            if lines.start > lines.end {
                return Err(format!(
                    "invalid line range: start ({}) > end ({})",
                    lines.start, lines.end
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MarkerKind {
    /// Behavioral contract: invariant, precondition, assumption.
    Contract {
        description: String,
        source: ContractSource,
    },
    /// Something non-obvious that could cause bugs.
    Hazard { description: String },
    /// This code assumes something about code elsewhere.
    Dependency {
        target_file: String,
        target_anchor: String,
        assumption: String,
    },
    /// This code is provisional/experimental.
    Unstable {
        description: String,
        revisit_when: String,
    },
    /// Security-sensitive code: auth, crypto, input validation, etc.
    Security { description: String },
    /// Performance-sensitive code: hot paths, allocations, latency.
    Performance { description: String },
    /// Deprecated code with optional replacement pointer.
    Deprecated {
        description: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replacement: Option<String>,
    },
    /// Known technical debt to address later.
    TechDebt { description: String },
    /// Test coverage note: what's tested, what's missing.
    TestCoverage { description: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContractSource {
    Author,
    Inferred,
}

// ---------------------------------------------------------------------------
// Effort Linking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EffortLink {
    /// Stable identifier (ticket ID, slug, etc.)
    pub id: String,
    pub description: String,
    pub phase: EffortPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffortPhase {
    Start,
    InProgress,
    Complete,
}

// ---------------------------------------------------------------------------
// Provenance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct Provenance {
    pub source: ProvenanceSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    Live,
    Batch,
    Backfill,
    Squash,
    Amend,
    MigratedV1,
    MigratedV2,
}

impl std::fmt::Display for ProvenanceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Live => write!(f, "live"),
            Self::Batch => write!(f, "batch"),
            Self::Backfill => write!(f, "backfill"),
            Self::Squash => write!(f, "squash"),
            Self::Amend => write!(f, "amend"),
            Self::MigratedV1 => write!(f, "migrated_v1"),
            Self::MigratedV2 => write!(f, "migrated_v2"),
        }
    }
}
