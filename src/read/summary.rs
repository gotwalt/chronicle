use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::common::LineRange;
use crate::schema::{self, v3};

/// Query parameters for a condensed summary.
#[derive(Debug, Clone)]
pub struct SummaryQuery {
    pub file: String,
    pub anchor: Option<String>,
}

/// A summary unit for one AST element.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryUnit {
    pub anchor: SummaryAnchor,
    pub lines: LineRange,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    pub last_modified: String,
}

/// Anchor information in a summary unit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryAnchor {
    #[serde(rename = "type")]
    pub unit_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Statistics about the summary query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryStats {
    pub regions_found: u32,
    pub commits_examined: u32,
}

/// Output of a summary query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryOutput {
    pub schema: String,
    pub query: QueryEcho,
    pub units: Vec<SummaryUnit>,
    pub stats: SummaryStats,
}

/// Echo of the query parameters in the output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryEcho {
    pub file: String,
    pub anchor: Option<String>,
}

/// Accumulated state for a single anchor across markers.
struct AnchorAccumulator {
    anchor: SummaryAnchor,
    lines: LineRange,
    intent: String,
    constraints: Vec<String>,
    risk_notes: Option<String>,
    timestamp: String,
}

/// Build a condensed summary for a file (or file+anchor).
///
/// In v3, all metadata is in wisdom entries. This function groups wisdom
/// by file and produces summary units with gotcha entries as constraints.
pub fn build_summary(git: &dyn GitOps, query: &SummaryQuery) -> Result<SummaryOutput, GitError> {
    let shas = git.log_for_file(&query.file)?;
    let commits_examined = shas.len() as u32;

    // Key: file -> AnchorAccumulator
    // In v3 there are no named anchors in wisdom, so we group by file.
    let mut best: std::collections::HashMap<String, AnchorAccumulator> =
        std::collections::HashMap::new();

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation = match schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(e) => {
                tracing::debug!("skipping malformed annotation for {sha}: {e}");
                continue;
            }
        };

        // Group wisdom entries from this commit by file
        let mut commit_groups: std::collections::HashMap<String, AnchorAccumulator> =
            std::collections::HashMap::new();

        for w in &annotation.wisdom {
            let entry_file = match &w.file {
                Some(f) => f,
                None => continue,
            };
            if !file_matches(entry_file, &query.file) {
                continue;
            }

            let key = entry_file.clone();

            // Skip if we already have a newer entry for this file key
            if best.contains_key(&key) {
                continue;
            }

            let lines = w.lines.unwrap_or(LineRange { start: 0, end: 0 });

            let acc = commit_groups
                .entry(key)
                .or_insert_with(|| AnchorAccumulator {
                    anchor: SummaryAnchor {
                        unit_type: "file".to_string(),
                        name: entry_file.clone(),
                        signature: None,
                    },
                    lines,
                    intent: annotation.summary.clone(),
                    constraints: vec![],
                    risk_notes: None,
                    timestamp: annotation.timestamp.clone(),
                });

            match w.category {
                v3::WisdomCategory::Gotcha => {
                    if !acc.constraints.contains(&w.content) {
                        acc.constraints.push(w.content.clone());
                    }
                }
                _ => {
                    // Other wisdom categories contribute to risk_notes
                    let note = w.content.clone();
                    acc.risk_notes = Some(match acc.risk_notes.take() {
                        Some(existing) => format!("{existing}; {note}"),
                        None => note,
                    });
                }
            }
        }

        for (key, acc) in commit_groups {
            best.entry(key).or_insert(acc);
        }
    }

    let mut units: Vec<SummaryUnit> = best
        .into_values()
        .map(|acc| SummaryUnit {
            anchor: acc.anchor,
            lines: acc.lines,
            intent: acc.intent,
            constraints: acc.constraints,
            risk_notes: acc.risk_notes,
            last_modified: acc.timestamp,
        })
        .collect();
    // Sort by line start for deterministic output
    units.sort_by_key(|u| u.lines.start);

    let regions_found = units.len() as u32;

    Ok(SummaryOutput {
        schema: "chronicle-summary/v1".to_string(),
        query: QueryEcho {
            file: query.file.clone(),
            anchor: query.anchor.clone(),
        },
        units,
        stats: SummaryStats {
            regions_found,
            commits_examined,
        },
    })
}

use super::matching::file_matches;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v1::{
        self, Constraint, ConstraintSource, ContextLevel, Provenance, ProvenanceOperation,
        RegionAnnotation,
    };
    use crate::schema::v2;
    type Annotation = v1::Annotation;

    struct MockGitOps {
        file_log: Vec<String>,
        notes: std::collections::HashMap<String, String>,
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> Result<Vec<crate::git::FileDiff>, GitError> {
            Ok(vec![])
        }
        fn note_read(&self, commit: &str) -> Result<Option<String>, GitError> {
            Ok(self.notes.get(commit).cloned())
        }
        fn note_write(&self, _commit: &str, _content: &str) -> Result<(), GitError> {
            Ok(())
        }
        fn note_exists(&self, commit: &str) -> Result<bool, GitError> {
            Ok(self.notes.contains_key(commit))
        }
        fn file_at_commit(
            &self,
            _path: &std::path::Path,
            _commit: &str,
        ) -> Result<String, GitError> {
            Ok(String::new())
        }
        fn commit_info(&self, _commit: &str) -> Result<crate::git::CommitInfo, GitError> {
            Ok(crate::git::CommitInfo {
                sha: "abc123".to_string(),
                message: "test".to_string(),
                author_name: "test".to_string(),
                author_email: "test@test.com".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                parent_shas: vec![],
            })
        }
        fn resolve_ref(&self, _refspec: &str) -> Result<String, GitError> {
            Ok("abc123".to_string())
        }
        fn config_get(&self, _key: &str) -> Result<Option<String>, GitError> {
            Ok(None)
        }
        fn config_set(&self, _key: &str, _value: &str) -> Result<(), GitError> {
            Ok(())
        }
        fn log_for_file(&self, _path: &str) -> Result<Vec<String>, GitError> {
            Ok(self.file_log.clone())
        }
        fn list_annotated_commits(&self, _limit: u32) -> Result<Vec<String>, GitError> {
            Ok(vec![])
        }
    }

    fn make_annotation(
        commit: &str,
        timestamp: &str,
        regions: Vec<RegionAnnotation>,
    ) -> Annotation {
        Annotation {
            schema: "chronicle/v1".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions,
            cross_cutting: vec![],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }
    }

    fn make_region(
        file: &str,
        anchor: &str,
        unit_type: &str,
        lines: LineRange,
        _intent: &str,
        constraints: Vec<Constraint>,
        risk_notes: Option<&str>,
    ) -> RegionAnnotation {
        RegionAnnotation {
            file: file.to_string(),
            ast_anchor: AstAnchor {
                unit_type: unit_type.to_string(),
                name: anchor.to_string(),
                signature: None,
            },
            lines,
            intent: "test intent".to_string(),
            reasoning: Some("detailed reasoning".to_string()),
            constraints,
            semantic_dependencies: vec![],
            related_annotations: vec![],
            tags: vec!["tag1".to_string()],
            risk_notes: risk_notes.map(|s| s.to_string()),
            corrections: vec![],
        }
    }

    #[test]
    fn test_summary_with_constraints_and_risk() {
        // v1 regions with constraints and risk_notes migrate to markers,
        // which produce summary units.
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "entry point",
                vec![Constraint {
                    text: "must not panic".to_string(),
                    source: ConstraintSource::Author,
                }],
                Some("error handling is fragile"),
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        // The "main" anchor should have both contract and hazard markers aggregated
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].anchor.name, "src/main.rs"); // v3 groups by file, not anchor
        // Both v1 constraints and risk_notes become Gotcha wisdom entries (constraints)
        assert_eq!(
            result.units[0].constraints,
            vec!["must not panic", "error handling is fragile"]
        );
        // No separate risk_notes since both are gotcha category in v3
        assert_eq!(result.units[0].risk_notes, None);
    }

    #[test]
    fn test_summary_keeps_most_recent_marker() {
        // Two commits with same anchor constraint. Newest first in git log.
        let ann1 = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "",
                vec![Constraint {
                    text: "old constraint".to_string(),
                    source: ConstraintSource::Author,
                }],
                None,
            )],
        );
        let ann2 = make_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "",
                vec![Constraint {
                    text: "new constraint".to_string(),
                    source: ConstraintSource::Author,
                }],
                None,
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());

        let git = MockGitOps {
            // newest first (as git log returns)
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].constraints, vec!["new constraint"]);
        assert_eq!(result.units[0].last_modified, "2025-01-02T00:00:00Z");
    }

    #[test]
    fn test_summary_only_intent_constraints_risk() {
        // Verify that reasoning and tags don't appear in the output
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "entry point",
                vec![Constraint {
                    text: "must be fast".to_string(),
                    source: ConstraintSource::Inferred,
                }],
                None,
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        let json = serde_json::to_string(&result).unwrap();
        // Should not contain "reasoning" or "tags" fields
        assert!(!json.contains("\"reasoning\""));
        assert!(!json.contains("\"tags\""));
    }

    #[test]
    fn test_summary_empty_when_no_annotations() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes: std::collections::HashMap::new(),
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert!(result.units.is_empty());
        assert_eq!(result.stats.regions_found, 0);
    }

    #[test]
    fn test_summary_with_anchor_filter() {
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                make_region(
                    "src/main.rs",
                    "main",
                    "fn",
                    LineRange { start: 1, end: 10 },
                    "",
                    vec![Constraint {
                        text: "must not panic".to_string(),
                        source: ConstraintSource::Author,
                    }],
                    None,
                ),
                make_region(
                    "src/main.rs",
                    "helper",
                    "fn",
                    LineRange { start: 12, end: 20 },
                    "",
                    vec![Constraint {
                        text: "must be pure".to_string(),
                        source: ConstraintSource::Inferred,
                    }],
                    None,
                ),
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
        };

        let result = build_summary(&git, &query).unwrap();
        // v3 file-level grouping collapses both anchors into one unit for src/main.rs
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].anchor.name, "src/main.rs");
        // Both anchors' constraints merged into the file-level unit
        assert_eq!(result.units[0].constraints, vec!["must not panic", "must be pure"]);
    }

    #[test]
    fn test_summary_native_v2_annotation() {
        // Test with a native v2 annotation (migrated to v3 at parse time)
        let v2_ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "commit1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "Add caching layer".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/cache.rs".to_string()],
                sentiments: vec![],
            },
            decisions: vec![],
            markers: vec![
                v2::CodeMarker {
                    file: "src/cache.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "function".to_string(),
                        name: "Cache::get".to_string(),
                        signature: None,
                    }),
                    lines: Some(LineRange { start: 10, end: 20 }),
                    kind: v2::MarkerKind::Contract {
                        description: "Must return None for expired entries".to_string(),
                        source: v2::ContractSource::Author,
                    },
                },
                v2::CodeMarker {
                    file: "src/cache.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "function".to_string(),
                        name: "Cache::get".to_string(),
                        signature: None,
                    }),
                    lines: Some(LineRange { start: 10, end: 20 }),
                    kind: v2::MarkerKind::Hazard {
                        description: "Not thread-safe without external locking".to_string(),
                    },
                },
            ],
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: None,
                derived_from: vec![],
                notes: None,
            },
        };
        let note = serde_json::to_string(&v2_ann).unwrap();

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/cache.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].anchor.name, "src/cache.rs"); // v3 groups by file, not anchor
        assert_eq!(result.units[0].intent, "Add caching layer");
        // Both Contract and Hazard become Gotcha wisdom entries (constraints) in v3
        assert_eq!(
            result.units[0].constraints,
            vec![
                "Must return None for expired entries",
                "Not thread-safe without external locking"
            ]
        );
        assert_eq!(result.units[0].risk_notes, None);
    }

    #[test]
    fn test_summary_no_markers_no_units() {
        // v1 regions with no constraints/risk/deps produce no markers,
        // so they correctly produce no summary units (v2 summary is marker-based)
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "entry point",
                vec![],
                None,
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        // No constraints/risk/deps = no markers = no units (this is expected in v2)
        assert!(result.units.is_empty());
    }
}
