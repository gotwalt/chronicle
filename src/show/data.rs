use crate::error::{ChronicleError, Result};
use crate::git::GitOps;
use crate::read::{self, MatchedAnnotation, ReadQuery};
use crate::schema::common::{AstAnchor, LineRange};
use crate::schema::v1::{
    Constraint, ConstraintSource, ContextLevel, Provenance, ProvenanceOperation, RegionAnnotation,
    SemanticDependency,
};
use crate::schema::v2;

/// A region annotation with its commit-level metadata.
#[derive(Debug, Clone)]
pub struct RegionRef {
    pub region: RegionAnnotation,
    pub commit: String,
    pub timestamp: String,
    pub summary: String,
    pub context_level: ContextLevel,
    pub provenance: Provenance,
}

/// Maps each source line to the annotation regions covering it.
#[derive(Debug)]
pub struct LineAnnotationMap {
    /// For each line (index 0 = line 1), indices into ShowData.regions.
    coverage: Vec<Vec<usize>>,
}

impl LineAnnotationMap {
    /// Build the map from regions and the total number of source lines.
    pub fn build_from_regions(regions: &[RegionRef], total_lines: usize) -> Self {
        Self::build(regions, total_lines)
    }

    fn build(regions: &[RegionRef], total_lines: usize) -> Self {
        let mut coverage = vec![Vec::new(); total_lines];
        for (idx, r) in regions.iter().enumerate() {
            let start = r.region.lines.start.saturating_sub(1) as usize;
            let end = (r.region.lines.end as usize).min(total_lines);
            for slot in &mut coverage[start..end] {
                slot.push(idx);
            }
        }
        Self { coverage }
    }

    /// Get region indices covering a given line (1-indexed).
    pub fn regions_at_line(&self, line: u32) -> &[usize] {
        let idx = line.saturating_sub(1) as usize;
        self.coverage.get(idx).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Find the next line >= `from` (1-indexed) that has annotation coverage.
    /// Returns None if no annotated lines from that point.
    pub fn next_annotated_line(&self, from: u32) -> Option<u32> {
        let start = from.saturating_sub(1) as usize;
        for (i, regions) in self.coverage[start..].iter().enumerate() {
            if !regions.is_empty() {
                return Some((start + i) as u32 + 1);
            }
        }
        None
    }

    /// Find the previous line <= `from` (1-indexed) that has annotation coverage.
    pub fn prev_annotated_line(&self, from: u32) -> Option<u32> {
        let end = (from as usize).min(self.coverage.len());
        for i in (0..end).rev() {
            if !self.coverage[i].is_empty() {
                return Some(i as u32 + 1);
            }
        }
        None
    }
}

/// All data needed to render the show view.
#[derive(Debug)]
pub struct ShowData {
    pub file_path: String,
    pub commit: String,
    pub source_lines: Vec<String>,
    pub regions: Vec<RegionRef>,
    pub annotation_map: LineAnnotationMap,
}

/// Build ShowData for a file: read content, parse AST, fetch annotations, map lines.
pub fn build_show_data(
    git_ops: &dyn GitOps,
    file_path: &str,
    commit: &str,
    anchor: Option<&str>,
) -> Result<ShowData> {
    // Read file content at the given commit
    let source = git_ops
        .file_at_commit(std::path::Path::new(file_path), commit)
        .map_err(|e| ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    let source_lines: Vec<String> = source.lines().map(String::from).collect();
    let total_lines = source_lines.len();

    // Fetch annotations via the read pipeline
    let query = ReadQuery {
        file: file_path.to_string(),
        anchor: anchor.map(String::from),
        lines: None,
    };
    let read_result = read::execute(git_ops, &query)?;

    // Convert v2 MatchedAnnotations to v1-style RegionRefs for the show TUI
    let regions = convert_to_region_refs(read_result.annotations, file_path);

    let annotation_map = LineAnnotationMap::build(&regions, total_lines);

    Ok(ShowData {
        file_path: file_path.to_string(),
        commit: commit.to_string(),
        source_lines,
        regions,
        annotation_map,
    })
}

/// Convert v2 MatchedAnnotations into v1-style RegionRefs for the show TUI.
///
/// Each v2 marker with matching file becomes a RegionRef. Annotations without
/// markers but with the file in files_changed get a synthetic region.
fn convert_to_region_refs(annotations: Vec<MatchedAnnotation>, file_path: &str) -> Vec<RegionRef> {
    use std::collections::HashMap;

    let mut best: HashMap<String, RegionRef> = HashMap::new();

    for ann in annotations {
        if ann.markers.is_empty() {
            // Annotation has no markers for this file but file is in files_changed.
            // Create a synthetic region covering line 1 with the summary as intent.
            let key = format!("{}:{}", file_path, "__commit_level__");
            let region_ref = RegionRef {
                region: RegionAnnotation {
                    file: file_path.to_string(),
                    ast_anchor: AstAnchor {
                        unit_type: "commit".to_string(),
                        name: "(commit-level)".to_string(),
                        signature: None,
                    },
                    lines: LineRange { start: 1, end: 1 },
                    intent: ann.summary.clone(),
                    reasoning: ann.motivation.clone(),
                    constraints: vec![],
                    semantic_dependencies: vec![],
                    related_annotations: vec![],
                    tags: vec![],
                    risk_notes: None,
                    corrections: vec![],
                },
                commit: ann.commit.clone(),
                timestamp: ann.timestamp.clone(),
                summary: ann.summary.clone(),
                context_level: ContextLevel::Inferred,
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            };
            let existing = best.get(&key);
            if existing.is_none() || region_ref.timestamp > existing.unwrap().timestamp {
                best.insert(key, region_ref);
            }
            continue;
        }

        // Group markers by anchor name
        let mut markers_by_anchor: HashMap<String, Vec<&v2::CodeMarker>> = HashMap::new();
        for marker in &ann.markers {
            let anchor_name = marker
                .anchor
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default();
            markers_by_anchor
                .entry(anchor_name)
                .or_default()
                .push(marker);
        }

        for (anchor_name, markers) in markers_by_anchor {
            let key = format!("{}:{}", file_path, anchor_name);

            // Determine line range from markers
            let mut line_start = u32::MAX;
            let mut line_end = 0u32;
            for m in &markers {
                if let Some(ref lines) = m.lines {
                    line_start = line_start.min(lines.start);
                    line_end = line_end.max(lines.end);
                }
            }
            if line_start == u32::MAX {
                line_start = 1;
                line_end = 1;
            }

            // Extract constraints, dependencies, risk notes from markers
            let mut constraints = Vec::new();
            let mut deps = Vec::new();
            let mut risk_notes = Vec::new();

            for m in &markers {
                match &m.kind {
                    v2::MarkerKind::Contract {
                        description,
                        source,
                    } => {
                        let cs = match source {
                            v2::ContractSource::Author => ConstraintSource::Author,
                            v2::ContractSource::Inferred => ConstraintSource::Inferred,
                        };
                        constraints.push(Constraint {
                            text: description.clone(),
                            source: cs,
                        });
                    }
                    v2::MarkerKind::Hazard { description } => {
                        risk_notes.push(description.clone());
                    }
                    v2::MarkerKind::Dependency {
                        target_file,
                        target_anchor,
                        assumption,
                    } => {
                        deps.push(SemanticDependency {
                            file: target_file.clone(),
                            anchor: target_anchor.clone(),
                            nature: assumption.clone(),
                        });
                    }
                    v2::MarkerKind::Unstable { description, .. } => {
                        risk_notes.push(format!("[unstable] {}", description));
                    }
                    v2::MarkerKind::Security { description } => {
                        risk_notes.push(format!("[security] {}", description));
                    }
                    v2::MarkerKind::Performance { description } => {
                        risk_notes.push(format!("[performance] {}", description));
                    }
                    v2::MarkerKind::Deprecated { description, .. } => {
                        risk_notes.push(format!("[deprecated] {}", description));
                    }
                    v2::MarkerKind::TechDebt { description } => {
                        risk_notes.push(format!("[tech_debt] {}", description));
                    }
                    v2::MarkerKind::TestCoverage { description } => {
                        risk_notes.push(format!("[test_coverage] {}", description));
                    }
                }
            }

            let ast_anchor = markers
                .first()
                .and_then(|m| m.anchor.clone())
                .unwrap_or(AstAnchor {
                    unit_type: "unknown".to_string(),
                    name: anchor_name.clone(),
                    signature: None,
                });

            let region_ref = RegionRef {
                region: RegionAnnotation {
                    file: file_path.to_string(),
                    ast_anchor,
                    lines: LineRange {
                        start: line_start,
                        end: line_end,
                    },
                    intent: ann.summary.clone(),
                    reasoning: ann.motivation.clone(),
                    constraints,
                    semantic_dependencies: deps,
                    related_annotations: vec![],
                    tags: vec![],
                    risk_notes: if risk_notes.is_empty() {
                        None
                    } else {
                        Some(risk_notes.join("; "))
                    },
                    corrections: vec![],
                },
                commit: ann.commit.clone(),
                timestamp: ann.timestamp.clone(),
                summary: ann.summary.clone(),
                context_level: ContextLevel::Inferred,
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            };

            let existing = best.get(&key);
            if existing.is_none() || region_ref.timestamp > existing.unwrap().timestamp {
                best.insert(key, region_ref);
            }
        }
    }

    let mut regions: Vec<RegionRef> = best.into_values().collect();
    regions.sort_by_key(|r| r.region.lines.start);
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_annotation_map_empty() {
        let map = LineAnnotationMap::build(&[], 10);
        assert!(map.regions_at_line(1).is_empty());
        assert!(map.regions_at_line(5).is_empty());
        assert!(map.next_annotated_line(1).is_none());
        assert!(map.prev_annotated_line(10).is_none());
    }

    #[test]
    fn test_line_annotation_map_coverage() {
        use crate::schema::common::*;
        use crate::schema::v1::*;

        let regions = vec![RegionRef {
            region: RegionAnnotation {
                file: "test.rs".to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "function".to_string(),
                    name: "foo".to_string(),
                    signature: None,
                },
                lines: LineRange { start: 3, end: 5 },
                intent: "test".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                related_annotations: vec![],
                tags: vec![],
                risk_notes: None,
                corrections: vec![],
            },
            commit: "abc".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            summary: "test".to_string(),
            context_level: ContextLevel::Inferred,
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }];

        let map = LineAnnotationMap::build(&regions, 10);
        assert!(map.regions_at_line(1).is_empty());
        assert!(map.regions_at_line(2).is_empty());
        assert_eq!(map.regions_at_line(3), &[0]);
        assert_eq!(map.regions_at_line(4), &[0]);
        assert_eq!(map.regions_at_line(5), &[0]);
        assert!(map.regions_at_line(6).is_empty());
    }

    #[test]
    fn test_next_prev_annotated_line() {
        use crate::schema::common::*;
        use crate::schema::v1::*;

        let regions = vec![RegionRef {
            region: RegionAnnotation {
                file: "test.rs".to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "function".to_string(),
                    name: "foo".to_string(),
                    signature: None,
                },
                lines: LineRange { start: 5, end: 8 },
                intent: "test".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                related_annotations: vec![],
                tags: vec![],
                risk_notes: None,
                corrections: vec![],
            },
            commit: "abc".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            summary: "test".to_string(),
            context_level: ContextLevel::Inferred,
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }];

        let map = LineAnnotationMap::build(&regions, 15);
        assert_eq!(map.next_annotated_line(1), Some(5));
        assert_eq!(map.next_annotated_line(5), Some(5));
        assert_eq!(map.next_annotated_line(9), None);
        assert_eq!(map.prev_annotated_line(10), Some(8));
        assert_eq!(map.prev_annotated_line(4), None);
    }
}
