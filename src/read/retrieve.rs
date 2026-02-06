use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::annotation::Annotation;

use super::{MatchedRegion, ReadQuery};

/// Retrieve matching region annotations for a file from git notes.
///
/// 1. Find commits that touched the file via `git log --follow`
/// 2. For each commit, try to read the ultragit note
/// 3. Parse the note as an Annotation
/// 4. Filter regions matching the query (file path, anchor, line range)
/// 5. Return results sorted newest-first (preserving git log order)
pub fn retrieve_regions(git: &dyn GitOps, query: &ReadQuery) -> Result<Vec<MatchedRegion>, GitError> {
    let shas = git.log_for_file(&query.file)?;
    let mut matched = Vec::new();

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation: Annotation = match serde_json::from_str(&note) {
            Ok(a) => a,
            Err(_) => continue, // skip malformed notes
        };

        for region in &annotation.regions {
            if !file_matches(&region.file, &query.file) {
                continue;
            }
            if let Some(ref anchor_name) = query.anchor {
                if region.ast_anchor.name != *anchor_name {
                    continue;
                }
            }
            if let Some(ref line_range) = query.lines {
                if !ranges_overlap(region.lines.start, region.lines.end, line_range.start, line_range.end) {
                    continue;
                }
            }
            matched.push(MatchedRegion {
                commit: sha.clone(),
                timestamp: annotation.timestamp.clone(),
                region: region.clone(),
                summary: annotation.summary.clone(),
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::schema::annotation::*;

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&Annotation {
                schema: "ultragit/v1".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                task: None,
                summary: "test commit".to_string(),
                context_level: ContextLevel::Enhanced,
                regions: vec![
                    RegionAnnotation {
                        file: "src/main.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "fn".to_string(), name: "main".to_string(), signature: None },
                        lines: LineRange { start: 1, end: 10 },
                        intent: "entry point".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                    RegionAnnotation {
                        file: "src/lib.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "mod".to_string(), name: "lib".to_string(), signature: None },
                        lines: LineRange { start: 1, end: 5 },
                        intent: "module decl".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                ],
                cross_cutting: vec![],
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            }).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: None,
        };

        let results = retrieve_regions(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].region.file, "src/main.rs");
        assert_eq!(results[0].region.intent, "entry point");
    }

    #[test]
    fn test_retrieve_filters_by_anchor() {
        use crate::schema::annotation::*;

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&Annotation {
                schema: "ultragit/v1".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                task: None,
                summary: "test commit".to_string(),
                context_level: ContextLevel::Enhanced,
                regions: vec![
                    RegionAnnotation {
                        file: "src/main.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "fn".to_string(), name: "main".to_string(), signature: None },
                        lines: LineRange { start: 1, end: 10 },
                        intent: "entry point".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                    RegionAnnotation {
                        file: "src/main.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "fn".to_string(), name: "helper".to_string(), signature: None },
                        lines: LineRange { start: 12, end: 20 },
                        intent: "helper fn".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                ],
                cross_cutting: vec![],
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            }).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
            lines: None,
        };

        let results = retrieve_regions(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].region.ast_anchor.name, "main");
    }

    #[test]
    fn test_retrieve_filters_by_lines() {
        use crate::schema::annotation::*;

        let git = MockGitOps {
            shas: vec!["abc123".to_string()],
            note: Some(serde_json::to_string(&Annotation {
                schema: "ultragit/v1".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                task: None,
                summary: "test commit".to_string(),
                context_level: ContextLevel::Enhanced,
                regions: vec![
                    RegionAnnotation {
                        file: "src/main.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "fn".to_string(), name: "main".to_string(), signature: None },
                        lines: LineRange { start: 1, end: 10 },
                        intent: "entry point".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                    RegionAnnotation {
                        file: "src/main.rs".to_string(),
                        ast_anchor: AstAnchor { unit_type: "fn".to_string(), name: "helper".to_string(), signature: None },
                        lines: LineRange { start: 50, end: 60 },
                        intent: "helper fn".to_string(),
                        reasoning: None,
                        constraints: vec![],
                        semantic_dependencies: vec![],
                        related_annotations: vec![],
                        tags: vec![],
                        risk_notes: None,
                    },
                ],
                cross_cutting: vec![],
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            }).unwrap()),
        };

        let query = ReadQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            lines: Some(LineRange { start: 5, end: 15 }),
        };

        let results = retrieve_regions(&git, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].region.ast_anchor.name, "main");
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

        let results = retrieve_regions(&git, &query).unwrap();
        assert!(results.is_empty());
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
        fn file_at_commit(&self, _path: &std::path::Path, _commit: &str) -> Result<String, crate::error::GitError> {
            Ok(String::new())
        }
        fn commit_info(&self, _commit: &str) -> Result<crate::git::CommitInfo, crate::error::GitError> {
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
        fn list_annotated_commits(&self, _limit: u32) -> Result<Vec<String>, crate::error::GitError> {
            Ok(self.shas.clone())
        }
    }
}
