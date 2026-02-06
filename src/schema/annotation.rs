use serde::{Deserialize, Serialize};

use super::correction::Correction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub schema: String,
    pub commit: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub summary: String,
    pub context_level: ContextLevel,
    pub regions: Vec<RegionAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_cutting: Vec<CrossCuttingConcern>,
    pub provenance: Provenance,
}

impl Annotation {
    pub fn new_initial(commit: String, summary: String, context_level: ContextLevel) -> Self {
        Self {
            schema: "chronicle/v1".to_string(),
            commit,
            timestamp: chrono::Utc::now().to_rfc3339(),
            task: None,
            summary,
            context_level,
            regions: Vec::new(),
            cross_cutting: Vec::new(),
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: Vec::new(),
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }
    }

    /// Validate the annotation for structural correctness.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "chronicle/v1" {
            return Err(format!("unsupported schema version: {}", self.schema));
        }
        if self.commit.is_empty() {
            return Err("commit SHA is empty".to_string());
        }
        if self.summary.is_empty() {
            return Err("summary is empty".to_string());
        }
        for (i, region) in self.regions.iter().enumerate() {
            if let Err(e) = region.validate() {
                return Err(format!("region[{}]: {}", i, e));
            }
        }
        Ok(())
    }
}

impl RegionAnnotation {
    /// Validate a region annotation for structural correctness.
    pub fn validate(&self) -> Result<(), String> {
        if self.file.is_empty() {
            return Err("file is empty".to_string());
        }
        if self.intent.is_empty() {
            return Err("intent is empty".to_string());
        }
        if self.lines.start > self.lines.end {
            return Err(format!(
                "invalid line range: start ({}) > end ({})",
                self.lines.start, self.lines.end
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextLevel {
    Enhanced,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAnnotation {
    pub file: String,
    pub ast_anchor: AstAnchor,
    pub lines: LineRange,
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_dependencies: Vec<SemanticDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_annotations: Vec<RelatedAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<Correction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstAnchor {
    pub unit_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub text: String,
    pub source: ConstraintSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSource {
    Author,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDependency {
    pub file: String,
    pub anchor: String,
    pub nature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedAnnotation {
    pub commit: String,
    pub anchor: String,
    pub relationship: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossCuttingConcern {
    pub description: String,
    pub regions: Vec<CrossCuttingRegionRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossCuttingRegionRef {
    pub file: String,
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub operation: ProvenanceOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    pub original_annotations_preserved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesis_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceOperation {
    Initial,
    Squash,
    Amend,
}
