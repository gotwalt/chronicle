use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::annotation::Annotation;

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
/// Scans annotated commits and finds regions whose `semantic_dependencies`
/// reference the queried file+anchor.
pub fn find_dependents(git: &dyn GitOps, query: &DepsQuery) -> Result<DepsOutput, GitError> {
    let annotated = git.list_annotated_commits(query.scan_limit)?;
    let commits_scanned = annotated.len() as u32;

    let mut dependents: Vec<DependentEntry> = Vec::new();

    for sha in &annotated {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation: Annotation = match serde_json::from_str(&note) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for region in &annotation.regions {
            for dep in &region.semantic_dependencies {
                if dep_matches(dep, &query.file, query.anchor.as_deref()) {
                    dependents.push(DependentEntry {
                        file: region.file.clone(),
                        anchor: region.ast_anchor.name.clone(),
                        nature: dep.nature.clone(),
                        commit: sha.clone(),
                        timestamp: annotation.timestamp.clone(),
                        context_level: format!("{:?}", annotation.context_level).to_lowercase(),
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
        schema: "ultragit-deps/v1".to_string(),
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

/// Check if a semantic dependency matches the queried file+anchor.
/// Supports unqualified matching: "max_sessions" matches "TlsSessionCache::max_sessions".
fn dep_matches(
    dep: &crate::schema::annotation::SemanticDependency,
    query_file: &str,
    query_anchor: Option<&str>,
) -> bool {
    if !file_matches(&dep.file, query_file) {
        return false;
    }
    match query_anchor {
        None => true,
        Some(qa) => anchor_matches(&dep.anchor, qa),
    }
}

fn file_matches(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> &str {
        s.strip_prefix("./").unwrap_or(s)
    }
    norm(a) == norm(b)
}

/// Check if a dependency anchor matches the query anchor.
/// Supports unqualified matching: "max_sessions" matches "TlsSessionCache::max_sessions"
/// and vice versa.
fn anchor_matches(dep_anchor: &str, query_anchor: &str) -> bool {
    if dep_anchor == query_anchor {
        return true;
    }
    // Unqualified match: check if one is a suffix of the other after "::"
    let dep_short = dep_anchor.rsplit("::").next().unwrap_or(dep_anchor);
    let query_short = query_anchor.rsplit("::").next().unwrap_or(query_anchor);
    dep_short == query_anchor || dep_anchor == query_short || dep_short == query_short
}

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
    use crate::schema::annotation::*;

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
        fn file_at_commit(&self, _path: &std::path::Path, _commit: &str) -> Result<String, GitError> {
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
            Ok(self.annotated_commits.iter().take(limit as usize).cloned().collect())
        }
    }

    fn make_annotation(commit: &str, timestamp: &str, regions: Vec<RegionAnnotation>) -> Annotation {
        Annotation {
            schema: "ultragit/v1".to_string(),
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

    fn make_region(file: &str, anchor: &str, deps: Vec<SemanticDependency>) -> RegionAnnotation {
        RegionAnnotation {
            file: file.to_string(),
            ast_anchor: AstAnchor {
                unit_type: "fn".to_string(),
                name: anchor.to_string(),
                signature: None,
            },
            lines: LineRange { start: 1, end: 10 },
            intent: "test".to_string(),
            reasoning: None,
            constraints: vec![],
            semantic_dependencies: deps,
            related_annotations: vec![],
            tags: vec![],
            risk_notes: None,
            corrections: vec![],
        }
    }

    #[test]
    fn test_finds_dependency() {
        let annotation = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/mqtt/reconnect.rs", "ReconnectHandler::attempt", vec![
                SemanticDependency {
                    file: "src/tls/session.rs".to_string(),
                    anchor: "TlsSessionCache::max_sessions".to_string(),
                    nature: "assumes max_sessions is 4".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&annotation).unwrap());

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
        assert_eq!(result.dependents[0].anchor, "ReconnectHandler::attempt");
        assert_eq!(result.dependents[0].nature, "assumes max_sessions is 4");
    }

    #[test]
    fn test_no_dependencies() {
        let annotation = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/mqtt/reconnect.rs", "ReconnectHandler::attempt", vec![]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&annotation).unwrap());

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
        let annotation = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/mqtt/reconnect.rs", "ReconnectHandler::attempt", vec![
                SemanticDependency {
                    file: "src/tls/session.rs".to_string(),
                    anchor: "max_sessions".to_string(),
                    nature: "assumes max_sessions is 4".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&annotation).unwrap());

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
        let ann1 = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/a.rs", "fn_a", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "calls shared_fn".to_string(),
                },
            ]),
        ]);
        let ann2 = make_annotation("commit2", "2025-01-02T00:00:00Z", vec![
            make_region("src/b.rs", "fn_b", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "uses shared_fn return value".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());

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
        // Two commits that both show src/a.rs:fn_a depending on src/shared.rs:shared_fn
        let ann1 = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/a.rs", "fn_a", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "old nature".to_string(),
                },
            ]),
        ]);
        let ann2 = make_annotation("commit2", "2025-01-02T00:00:00Z", vec![
            make_region("src/a.rs", "fn_a", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "new nature".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());

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
        let ann = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/a.rs", "fn_a", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "test".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

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
        let ann = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/a.rs", "fn_a", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "dep 1".to_string(),
                },
            ]),
            make_region("src/b.rs", "fn_b", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "dep 2".to_string(),
                },
            ]),
            make_region("src/c.rs", "fn_c", vec![
                SemanticDependency {
                    file: "src/shared.rs".to_string(),
                    anchor: "shared_fn".to_string(),
                    nature: "dep 3".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

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
        let annotation = make_annotation("commit1", "2025-01-01T00:00:00Z", vec![
            make_region("src/mqtt/reconnect.rs", "ReconnectHandler::attempt", vec![
                SemanticDependency {
                    file: "src/tls/session.rs".to_string(),
                    anchor: "TlsSessionCache::max_sessions".to_string(),
                    nature: "assumes max_sessions is 4".to_string(),
                },
            ]),
        ]);

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&annotation).unwrap());

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

    #[test]
    fn test_anchor_matches_exact() {
        assert!(anchor_matches("max_sessions", "max_sessions"));
    }

    #[test]
    fn test_anchor_matches_unqualified_dep() {
        assert!(anchor_matches("max_sessions", "TlsSessionCache::max_sessions"));
    }

    #[test]
    fn test_anchor_matches_unqualified_query() {
        assert!(anchor_matches("TlsSessionCache::max_sessions", "max_sessions"));
    }

    #[test]
    fn test_anchor_no_match() {
        assert!(!anchor_matches("other_fn", "max_sessions"));
    }
}
