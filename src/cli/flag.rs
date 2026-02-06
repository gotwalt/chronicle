use crate::error::Result;
use crate::git::{CliOps, GitOps};
use crate::schema::annotation::Annotation;
use crate::schema::correction::{Correction, CorrectionType, resolve_author};

/// Run the `git chronicle flag` command.
///
/// Flags the most recent annotation for a code region as potentially inaccurate.
/// 1. Find commits that touched the file via git log --follow.
/// 2. For each commit (newest first), look for an annotation with a matching region.
/// 3. Append a Flag correction to that region.
/// 4. Write the updated annotation back.
pub fn run(path: String, anchor: Option<String>, reason: String) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    // Find commits that touched this file
    let shas = git_ops
        .log_for_file(&path)
        .map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    // Search for the first commit with a matching annotation/region
    for sha in &shas {
        let note_content = match git_ops
            .note_read(sha)
            .map_err(|e| crate::error::ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            })? {
            Some(n) => n,
            None => continue,
        };

        let mut annotation: Annotation = serde_json::from_str(&note_content).map_err(|e| {
            crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            }
        })?;

        // Find the matching region
        let region_idx = find_matching_region(&annotation, &path, anchor.as_deref());
        if region_idx.is_none() {
            continue;
        }
        let region_idx = region_idx.unwrap();

        let author = resolve_author(&git_ops);
        let timestamp = chrono::Utc::now().to_rfc3339();

        let anchor_display = annotation.regions[region_idx].ast_anchor.name.clone();

        let correction = Correction {
            field: "region".to_string(),
            correction_type: CorrectionType::Flag,
            reason: reason.clone(),
            target_value: None,
            replacement: None,
            timestamp,
            author,
        };

        annotation.regions[region_idx].corrections.push(correction);

        let updated_json =
            serde_json::to_string_pretty(&annotation).map_err(|e| {
                crate::error::ChronicleError::Json {
                    source: e,
                    location: snafu::Location::default(),
                }
            })?;

        git_ops
            .note_write(sha, &updated_json)
            .map_err(|e| crate::error::ChronicleError::Git {
                source: e,
                location: snafu::Location::default(),
            })?;

        let short_sha = &sha[..7.min(sha.len())];
        eprintln!("Flagged annotation on commit {short_sha} for {anchor_display}");
        eprintln!("  Reason: {reason}");
        eprintln!("  Correction stored in refs/notes/chronicle");
        return Ok(());
    }

    // No matching annotation found
    let target = match &anchor {
        Some(a) => format!("{path}:{a}"),
        None => path.clone(),
    };
    Err(crate::error::ChronicleError::Config {
        message: format!(
            "No annotation found for '{target}'. No commits with matching annotations were found."
        ),
        location: snafu::Location::default(),
    })
}

/// Find the index of a region in the annotation matching the file path and optional anchor.
fn find_matching_region(
    annotation: &Annotation,
    path: &str,
    anchor: Option<&str>,
) -> Option<usize> {
    fn norm(s: &str) -> &str {
        s.strip_prefix("./").unwrap_or(s)
    }

    for (i, region) in annotation.regions.iter().enumerate() {
        if norm(&region.file) != norm(path) {
            continue;
        }
        match anchor {
            Some(anchor_name) => {
                if region.ast_anchor.name == anchor_name {
                    return Some(i);
                }
            }
            None => {
                // No anchor specified: match any region for this file
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::annotation::*;

    #[test]
    fn test_find_matching_region_by_anchor() {
        let annotation = Annotation {
            schema: "chronicle/v1".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions: vec![
                RegionAnnotation {
                    file: "src/main.rs".to_string(),
                    ast_anchor: AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "main".to_string(),
                        signature: None,
                    },
                    lines: LineRange { start: 1, end: 10 },
                    intent: "entry point".to_string(),
                    reasoning: None,
                    constraints: vec![],
                    semantic_dependencies: vec![],
                    related_annotations: vec![],
                    tags: vec![],
                    risk_notes: None,
                    corrections: vec![],
                },
                RegionAnnotation {
                    file: "src/main.rs".to_string(),
                    ast_anchor: AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "helper".to_string(),
                        signature: None,
                    },
                    lines: LineRange {
                        start: 12,
                        end: 20,
                    },
                    intent: "helper fn".to_string(),
                    reasoning: None,
                    constraints: vec![],
                    semantic_dependencies: vec![],
                    related_annotations: vec![],
                    tags: vec![],
                    risk_notes: None,
                    corrections: vec![],
                },
            ],
            cross_cutting: vec![],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        };

        assert_eq!(
            find_matching_region(&annotation, "src/main.rs", Some("helper")),
            Some(1)
        );
        assert_eq!(
            find_matching_region(&annotation, "src/main.rs", Some("main")),
            Some(0)
        );
        assert_eq!(
            find_matching_region(&annotation, "src/main.rs", Some("nonexistent")),
            None
        );
    }

    #[test]
    fn test_find_matching_region_no_anchor() {
        let annotation = Annotation {
            schema: "chronicle/v1".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions: vec![RegionAnnotation {
                file: "src/lib.rs".to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "mod".to_string(),
                    name: "lib".to_string(),
                    signature: None,
                },
                lines: LineRange { start: 1, end: 5 },
                intent: "module".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                related_annotations: vec![],
                tags: vec![],
                risk_notes: None,
                corrections: vec![],
            }],
            cross_cutting: vec![],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        };

        // No anchor, matches first region for the file
        assert_eq!(
            find_matching_region(&annotation, "src/lib.rs", None),
            Some(0)
        );
        // Wrong file
        assert_eq!(
            find_matching_region(&annotation, "src/main.rs", None),
            None
        );
    }

    #[test]
    fn test_find_matching_region_dot_slash_normalization() {
        let annotation = Annotation {
            schema: "chronicle/v1".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions: vec![RegionAnnotation {
                file: "./src/main.rs".to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "fn".to_string(),
                    name: "main".to_string(),
                    signature: None,
                },
                lines: LineRange { start: 1, end: 10 },
                intent: "entry".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                related_annotations: vec![],
                tags: vec![],
                risk_notes: None,
                corrections: vec![],
            }],
            cross_cutting: vec![],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        };

        assert_eq!(
            find_matching_region(&annotation, "src/main.rs", Some("main")),
            Some(0)
        );
    }
}
