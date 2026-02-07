use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v2};

use super::{MatchedAnnotation, ReadQuery};

/// Retrieve matching annotations for a file from git notes.
///
/// 1. Find commits that touched the file via `git log --follow`
/// 2. For each commit, try to read the chronicle note
/// 3. Parse the note as an Annotation (v1 or v2 via parse_annotation)
/// 4. Filter markers matching the query (file path, anchor, line range)
/// 5. Return results sorted newest-first (preserving git log order)
pub fn retrieve_annotations(
    git: &dyn GitOps,
    query: &ReadQuery,
) -> Result<Vec<MatchedAnnotation>, GitError> {
    let shas = git.log_for_file(&query.file)?;
    let mut matched = Vec::new();

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation = match schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(_) => continue, // skip malformed notes
        };

        // Filter markers by file/anchor/lines
        let filtered_markers: Vec<v2::CodeMarker> = annotation
            .markers
            .iter()
            .filter(|m| file_matches(&m.file, &query.file))
            .filter(|m| {
                query.anchor.as_ref().is_none_or(|qa| {
                    m.anchor
                        .as_ref()
                        .is_some_and(|a| a.name == *qa)
                })
            })
            .filter(|m| {
                query.lines.as_ref().is_none_or(|line_range| {
                    m.lines.as_ref().is_some_and(|ml| {
                        ranges_overlap(ml.start, ml.end, line_range.start, line_range.end)
                    })
                })
            })
            .cloned()
            .collect();

        // Filter decisions by scope
        let filtered_decisions: Vec<v2::Decision> = annotation
            .decisions
            .iter()
            .filter(|d| decision_scope_matches(d, &query.file))
            .cloned()
            .collect();

        // Include annotation if it has matching markers, matching decisions,
        // or if the file is in files_changed (relevant context even without markers)
        let file_in_files_changed = annotation
            .narrative
            .files_changed
            .iter()
            .any(|f| file_matches(f, &query.file));

        if filtered_markers.is_empty() && filtered_decisions.is_empty() && !file_in_files_changed {
            continue;
        }

        matched.push(MatchedAnnotation {
            commit: sha.clone(),
            timestamp: annotation.timestamp.clone(),
            summary: annotation.narrative.summary.clone(),
            motivation: annotation.narrative.motivation.clone(),
            markers: filtered_markers,
            decisions: filtered_decisions,
            follow_up: annotation.narrative.follow_up.clone(),
            provenance: format!("{:?}", annotation.provenance.source).to_lowercase(),
        });
    }

    Ok(matched)
}

/// Check if two file paths refer to the same file.
/// Normalizes by stripping leading "./" if present.
fn file_matches(region_file: &str, query_file: &str) -> bool {
    fn norm(s: &str) -> &str {
        s.strip_prefix("./").unwrap_or(s)
    }
    norm(region_file) == norm(query_file)
}

/// Check if two line ranges overlap.
fn ranges_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    a_start <= b_end && b_start <= a_end
}

/// Check if a decision's scope matches the queried file.
fn decision_scope_matches(decision: &v2::Decision, file: &str) -> bool {
    if decision.scope.is_empty() {
        return true;
    }
    let norm_file = file.strip_prefix("./").unwrap_or(file);
    decision.scope.iter().any(|s| {
        let norm_scope = s.strip_prefix("./").unwrap_or(s);
        let scope_file = norm_scope.split(':').next().unwrap_or(norm_scope);
        scope_file == norm_file || norm_file.starts_with(scope_file)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::AstAnchor;

    #[test]
    fn test_file_matches_exact() {
        assert!(file_matches("src/main.rs", "src/main.rs"));
    }

    #[test]
    fn test_file_matches_dot_slash() {
        assert!(file_matches("./src/main.rs", "src/main.rs"));
        assert!(file_matches("src/main.rs", "./src/main.rs"));
    }

    #[test]
    fn test_file_no_match() {
        assert!(!file_matches("src/lib.rs", "src/main.rs"));
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap(1, 10, 5, 15));
        assert!(ranges_overlap(5, 15, 1, 10));
        assert!(ranges_overlap(1, 10, 10, 20));
        assert!(ranges_overlap(1, 10, 1, 10));
    }

    #[test]
    fn test_ranges_no_overlap() {
        assert!(!ranges_overlap(1, 5, 6, 10));
        assert!(!ranges_overlap(6, 10, 1, 5));
    }

    #[test]
    fn test_retrieve_filters_by_file() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "test commit".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            },
            decisions: vec![],
            markers: vec![
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "main".to_string(),
                        signature: None,
                    }),
                    lines: None,
                    kind: v2::MarkerKind::Contract {
                        description: "entry point".to_string(),
                        source: v2::ContractSource::Author,
                    },
                },
                v2::CodeMarker {
                    file: "src/lib.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "mod".to_string(),
                        name: "lib".to_string(),
                        signature: None,
                    }),
                    lines: None,
                    kind: v2::MarkerKind::Contract {
                        description: "module decl".to_string(),
                        source: v2::ContractSource::Author,
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

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&ann).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: None,
        };

        let results = retrieve_annotations(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "test commit");
        // Only the marker for src/main.rs should be included
        assert_eq!(results[0].markers.len(), 1);
        assert_eq!(results[0].markers[0].file, "src/main.rs");
    }

    #[test]
    fn test_retrieve_filters_by_anchor() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "test commit".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string()],
            },
            decisions: vec![],
            markers: vec![
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "main".to_string(),
                        signature: None,
                    }),
                    lines: None,
                    kind: v2::MarkerKind::Contract {
                        description: "entry point".to_string(),
                        source: v2::ContractSource::Author,
                    },
                },
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "helper".to_string(),
                        signature: None,
                    }),
                    lines: None,
                    kind: v2::MarkerKind::Contract {
                        description: "helper fn".to_string(),
                        source: v2::ContractSource::Author,
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

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&ann).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
            lines: None,
        };

        let results = retrieve_annotations(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        // Only the marker for "main" anchor should be included
        assert_eq!(results[0].markers.len(), 1);
        assert_eq!(
            results[0].markers[0]
                .anchor
                .as_ref()
                .unwrap()
                .name,
            "main"
        );
    }

    #[test]
    fn test_retrieve_filters_by_lines() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "test commit".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string()],
            },
            decisions: vec![],
            markers: vec![
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "main".to_string(),
                        signature: None,
                    }),
                    lines: Some(crate::schema::common::LineRange { start: 1, end: 10 }),
                    kind: v2::MarkerKind::Contract {
                        description: "entry point".to_string(),
                        source: v2::ContractSource::Author,
                    },
                },
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "fn".to_string(),
                        name: "helper".to_string(),
                        signature: None,
                    }),
                    lines: Some(crate::schema::common::LineRange {
                        start: 50,
                        end: 60,
                    }),
                    kind: v2::MarkerKind::Contract {
                        description: "helper fn".to_string(),
                        source: v2::ContractSource::Author,
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

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&ann).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: Some(crate::schema::common::LineRange { start: 5, end: 15 }),
        };

        let results = retrieve_annotations(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        // Only the marker overlapping lines 5-15 should be included
        assert_eq!(results[0].markers.len(), 1);
        assert_eq!(
            results[0].markers[0]
                .anchor
                .as_ref()
                .unwrap()
                .name,
            "main"
        );
    }

    #[test]
    fn test_retrieve_skips_commits_without_notes() {
        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: None,
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: None,
        };

        let results = retrieve_annotations(&git, &query).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_retrieve_includes_annotation_with_file_in_files_changed() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "refactored main".to_string(),
                motivation: Some("cleanup".to_string()),
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string()],
            },
            decisions: vec![],
            markers: vec![], // no markers, but file is in files_changed
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: None,
                derived_from: vec![],
                notes: None,
            },
        };

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&ann).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: None,
        };

        let results = retrieve_annotations(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "refactored main");
        assert_eq!(results[0].motivation.as_deref(), Some("cleanup"));
    }

    /// Minimal mock for testing retrieve logic.
    struct MockGitOps {
        shas: Vec<String>,
        note: Option<String>,
    }

    impl crate::git::GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> Result<Vec<crate::git::FileDiff>, crate::error::GitError> {
            Ok(vec![])
        }
        fn note_read(&self, _commit: &str) -> Result<Option<String>, crate::error::GitError> {
            Ok(self.note.clone())
        }
        fn note_write(&self, _commit: &str, _content: &str) -> Result<(), crate::error::GitError> {
            Ok(())
        }
        fn note_exists(&self, _commit: &str) -> Result<bool, crate::error::GitError> {
            Ok(self.note.is_some())
        }
        fn file_at_commit(
            &self,
            _path: &std::path::Path,
            _commit: &str,
        ) -> Result<String, crate::error::GitError> {
            Ok(String::new())
        }
        fn commit_info(
            &self,
            _commit: &str,
        ) -> Result<crate::git::CommitInfo, crate::error::GitError> {
            Ok(crate::git::CommitInfo {
                sha: "abc123".to_string(),
                message: "test".to_string(),
                author_name: "test".to_string(),
                author_email: "test@test.com".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                parent_shas: vec![],
            })
        }
        fn resolve_ref(&self, _refspec: &str) -> Result<String, crate::error::GitError> {
            Ok("abc123".to_string())
        }
        fn config_get(&self, _key: &str) -> Result<Option<String>, crate::error::GitError> {
            Ok(None)
        }
        fn config_set(&self, _key: &str, _value: &str) -> Result<(), crate::error::GitError> {
            Ok(())
        }
        fn log_for_file(&self, _path: &str) -> Result<Vec<String>, crate::error::GitError> {
            Ok(self.shas.clone())
        }
        fn list_annotated_commits(
            &self,
            _limit: u32,
        ) -> Result<Vec<String>, crate::error::GitError> {
            Ok(self.shas.clone())
        }
    }
}
