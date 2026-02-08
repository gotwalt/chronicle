use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v3};

/// Query parameters for dependency inversion.
#[derive(Debug, Clone)]
pub struct DepsQuery {
    pub file: String,
    pub anchor: Option<String>,
    pub max_results: u32,
    pub scan_limit: u32,
}

/// A single dependent found during the scan.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DependentEntry {
    pub file: String,
    pub anchor: String,
    pub nature: String,
    pub commit: String,
    pub timestamp: String,
    pub context_level: String,
}

/// Statistics about the deps scan.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DepsStats {
    pub commits_scanned: u32,
    pub dependencies_found: u32,
    pub scan_method: String,
}

/// Output of a deps query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DepsOutput {
    pub schema: String,
    pub query: QueryEcho,
    pub dependents: Vec<DependentEntry>,
    pub stats: DepsStats,
}

/// Echo of the query parameters in the output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryEcho {
    pub file: String,
    pub anchor: Option<String>,
}

/// Execute a dependency inversion query via linear scan.
///
/// In v3, dependency information lives in `insight` wisdom entries
/// with the pattern "Depends on {file}:{anchor} — {assumption}".
/// This scans annotated commits for wisdom entries referencing the queried file+anchor.
pub fn find_dependents(git: &dyn GitOps, query: &DepsQuery) -> Result<DepsOutput, GitError> {
    let annotated = git.list_annotated_commits(query.scan_limit)?;
    let commits_scanned = annotated.len() as u32;

    let mut dependents: Vec<DependentEntry> = Vec::new();

    for sha in &annotated {
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

        for w in &annotation.wisdom {
            if w.category != v3::WisdomCategory::Insight {
                continue;
            }

            // Parse "Depends on {target_file}:{target_anchor} — {assumption}"
            if let Some(dep) = parse_dependency_content(&w.content) {
                if dep_matches(dep.0, dep.1, &query.file, query.anchor.as_deref()) {
                    let source_file = w.file.clone().unwrap_or_default();
                    dependents.push(DependentEntry {
                        file: source_file,
                        anchor: String::new(),
                        nature: dep.2.to_string(),
                        commit: sha.clone(),
                        timestamp: annotation.timestamp.clone(),
                        context_level: annotation.provenance.source.to_string(),
                    });
                }
            }
        }
    }

    // Deduplicate: keep most recent entry per (file, anchor) pair
    deduplicate(&mut dependents);

    // Apply max_results cap
    dependents.truncate(query.max_results as usize);

    let dependencies_found = dependents.len() as u32;

    Ok(DepsOutput {
        schema: "chronicle-deps/v1".to_string(),
        query: QueryEcho {
            file: query.file.clone(),
            anchor: query.anchor.clone(),
        },
        dependents,
        stats: DepsStats {
            commits_scanned,
            dependencies_found,
            scan_method: "linear".to_string(),
        },
    })
}

/// Parse dependency content from the migration format:
/// "Depends on {file}:{anchor} — {assumption}"
fn parse_dependency_content(content: &str) -> Option<(&str, &str, &str)> {
    let rest = content.strip_prefix("Depends on ")?;
    let (target, assumption) = rest.split_once(" — ")?;
    let (target_file, target_anchor) = target.split_once(':')?;
    Some((target_file, target_anchor, assumption))
}

/// Check if a dependency's target matches the queried file+anchor.
fn dep_matches(
    target_file: &str,
    target_anchor: &str,
    query_file: &str,
    query_anchor: Option<&str>,
) -> bool {
    if !file_matches(target_file, query_file) {
        return false;
    }
    match query_anchor {
        None => true,
        Some(qa) => anchor_matches(target_anchor, qa),
    }
}

use super::matching::{anchor_matches, file_matches};

/// Deduplicate dependents by (file, anchor), keeping the first occurrence
/// (which is the most recent since we scan newest-first from list_annotated_commits).
fn deduplicate(dependents: &mut Vec<DependentEntry>) {
    let mut seen = std::collections::HashSet::new();
    dependents.retain(|entry| {
        let key = (entry.file.clone(), entry.anchor.clone());
        seen.insert(key)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::AstAnchor;
    use crate::schema::v2;

    struct MockGitOps {
        annotated_commits: Vec<String>,
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
            Ok(vec![])
        }
        fn list_annotated_commits(&self, limit: u32) -> Result<Vec<String>, GitError> {
            Ok(self
                .annotated_commits
                .iter()
                .take(limit as usize)
                .cloned()
                .collect())
        }
    }

    fn make_v2_annotation(commit: &str, timestamp: &str, markers: Vec<v2::CodeMarker>) -> String {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            narrative: v2::Narrative {
                summary: "test".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec![],
                sentiments: vec![],
            },
            decisions: vec![],
            markers,
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: None,
                derived_from: vec![],
                notes: None,
            },
        };
        serde_json::to_string(&ann).unwrap()
    }

    fn make_dep_marker(
        file: &str,
        anchor: &str,
        target_file: &str,
        target_anchor: &str,
        assumption: &str,
    ) -> v2::CodeMarker {
        v2::CodeMarker {
            file: file.to_string(),
            anchor: Some(AstAnchor {
                unit_type: "fn".to_string(),
                name: anchor.to_string(),
                signature: None,
            }),
            lines: None,
            kind: v2::MarkerKind::Dependency {
                target_file: target_file.to_string(),
                target_anchor: target_anchor.to_string(),
                assumption: assumption.to_string(),
            },
        }
    }

    #[test]
    fn test_finds_dependency() {
        let note = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/mqtt/reconnect.rs",
                "ReconnectHandler::attempt",
                "src/tls/session.rs",
                "TlsSessionCache::max_sessions",
                "assumes max_sessions is 4",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/tls/session.rs".to_string(),
            anchor: Some("TlsSessionCache::max_sessions".to_string()),
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 1);
        assert_eq!(result.dependents[0].file, "src/mqtt/reconnect.rs");
        assert_eq!(result.dependents[0].anchor, ""); // v3 reader sets anchor to empty string
        assert_eq!(result.dependents[0].nature, "assumes max_sessions is 4");
    }

    #[test]
    fn test_no_dependencies() {
        let note = make_v2_annotation("commit1", "2025-01-01T00:00:00Z", vec![]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/tls/session.rs".to_string(),
            anchor: Some("max_sessions".to_string()),
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 0);
        assert_eq!(result.stats.dependencies_found, 0);
    }

    #[test]
    fn test_unqualified_anchor_match() {
        let note = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/mqtt/reconnect.rs",
                "ReconnectHandler::attempt",
                "src/tls/session.rs",
                "max_sessions",
                "assumes max_sessions is 4",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/tls/session.rs".to_string(),
            anchor: Some("TlsSessionCache::max_sessions".to_string()),
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 1);
    }

    #[test]
    fn test_multiple_dependents_from_different_commits() {
        let note1 = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/a.rs",
                "fn_a",
                "src/shared.rs",
                "shared_fn",
                "calls shared_fn",
            )],
        );
        let note2 = make_v2_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_dep_marker(
                "src/b.rs",
                "fn_b",
                "src/shared.rs",
                "shared_fn",
                "uses shared_fn return value",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);

        let git = MockGitOps {
            annotated_commits: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/shared.rs".to_string(),
            anchor: Some("shared_fn".to_string()),
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 2);
    }

    #[test]
    fn test_deduplicates_same_file_anchor() {
        let note1 = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/a.rs",
                "fn_a",
                "src/shared.rs",
                "shared_fn",
                "old nature",
            )],
        );
        let note2 = make_v2_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_dep_marker(
                "src/a.rs",
                "fn_a",
                "src/shared.rs",
                "shared_fn",
                "new nature",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);

        let git = MockGitOps {
            // newest first
            annotated_commits: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/shared.rs".to_string(),
            anchor: Some("shared_fn".to_string()),
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 1);
        // Should keep the first (most recent) one
        assert_eq!(result.dependents[0].nature, "new nature");
    }

    #[test]
    fn test_scan_limit_respected() {
        let note = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/a.rs",
                "fn_a",
                "src/shared.rs",
                "shared_fn",
                "test",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        // scan_limit=0 means we scan nothing
        let query = DepsQuery {
            file: "src/shared.rs".to_string(),
            anchor: Some("shared_fn".to_string()),
            max_results: 50,
            scan_limit: 0,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 0);
        assert_eq!(result.stats.commits_scanned, 0);
    }

    #[test]
    fn test_max_results_cap() {
        let note = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                make_dep_marker("src/a.rs", "fn_a", "src/shared.rs", "shared_fn", "dep 1"),
                make_dep_marker("src/b.rs", "fn_b", "src/shared.rs", "shared_fn", "dep 2"),
                make_dep_marker("src/c.rs", "fn_c", "src/shared.rs", "shared_fn", "dep 3"),
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        let query = DepsQuery {
            file: "src/shared.rs".to_string(),
            anchor: Some("shared_fn".to_string()),
            max_results: 2,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 2);
    }

    #[test]
    fn test_file_only_query() {
        let note = make_v2_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_dep_marker(
                "src/mqtt/reconnect.rs",
                "ReconnectHandler::attempt",
                "src/tls/session.rs",
                "TlsSessionCache::max_sessions",
                "assumes max_sessions is 4",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        // No anchor specified — should match any dep referencing the file
        let query = DepsQuery {
            file: "src/tls/session.rs".to_string(),
            anchor: None,
            max_results: 50,
            scan_limit: 500,
        };

        let result = find_dependents(&git, &query).unwrap();
        assert_eq!(result.dependents.len(), 1);
    }
}
