use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v3};

use super::{MatchedAnnotation, ReadQuery};

/// Retrieve matching annotations for a file from git notes.
///
/// 1. Find commits that touched the file via `git log --follow`
/// 2. For each commit, try to read the chronicle note
/// 3. Parse the note as a v3 Annotation via `parse_annotation`
/// 4. Filter wisdom entries matching the query (file path, line range)
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
            Err(e) => {
                tracing::debug!("skipping malformed annotation for {sha}: {e}");
                continue;
            }
        };

        let filtered_wisdom: Vec<v3::WisdomEntry> = annotation
            .wisdom
            .iter()
            .filter(|w| w.file.as_ref().is_none_or(|f| file_matches(f, &query.file)))
            .filter(|w| {
                query.lines.as_ref().is_none_or(|line_range| {
                    // Include entries without lines (file-wide or repo-wide)
                    w.lines.as_ref().is_none_or(|wl| {
                        ranges_overlap(wl.start, wl.end, line_range.start, line_range.end)
                    })
                })
            })
            .cloned()
            .collect();

        matched.push(MatchedAnnotation {
            commit: sha.clone(),
            timestamp: annotation.timestamp.clone(),
            summary: annotation.summary.clone(),
            wisdom: filtered_wisdom,
            provenance: annotation.provenance.source.to_string(),
        });
    }

    Ok(matched)
}

use super::matching::file_matches;

/// Check if two line ranges overlap.
fn ranges_overlap(a_start: u32, a_end: u32, b_start: u32, b_end: u32) -> bool {
    a_start <= b_end && b_start <= a_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::v2;

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
        // v2 annotation with markers on two files; parse_annotation() migrates to v3 wisdom entries.
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
                sentiments: vec![],
            },
            decisions: vec![],
            markers: vec![
                v2::CodeMarker {
                    file: "src/main.rs".to_string(),
                    anchor: Some(crate::schema::common::AstAnchor {
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
                    anchor: Some(crate::schema::common::AstAnchor {
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
        // In v3, wisdom entries for src/main.rs should be filtered to match
        assert!(results[0]
            .wisdom
            .iter()
            .all(|w| w.file.as_ref().is_none_or(|f| f == "src/main.rs")));
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
    fn test_retrieve_includes_annotation_without_wisdom() {
        // v2 annotation with no markers — just files_changed. Migrates to v3 with empty wisdom.
        // After migration to v3, this has no wisdom entries but should still be included.
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
                sentiments: vec![],
            },
            decisions: vec![],
            markers: vec![],
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
