use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v2};

/// Query parameters: "What was decided and what was tried?"
#[derive(Debug, Clone)]
pub struct DecisionsQuery {
    pub file: Option<String>,
}

/// A decision entry extracted from a v2 `Decision`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionEntry {
    pub what: String,
    pub why: String,
    pub stability: String,
    pub revisit_when: Option<String>,
    pub scope: Vec<String>,
    pub commit: String,
    pub timestamp: String,
}

/// A rejected alternative extracted from a v2 `Narrative.rejected_alternatives`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RejectedAlternativeEntry {
    pub approach: String,
    pub reason: String,
    pub commit: String,
    pub timestamp: String,
}

/// Output of a decisions query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionsOutput {
    pub schema: String,
    pub decisions: Vec<DecisionEntry>,
    pub rejected_alternatives: Vec<RejectedAlternativeEntry>,
}

/// Collect decisions and rejected alternatives from annotations.
///
/// 1. Determine which commits to examine:
///    - If a file is specified, use `log_for_file` to get commits touching that file
///    - Otherwise, use `list_annotated_commits` to scan all annotated commits
/// 2. For each commit, parse annotation via `parse_annotation` (handles v1 migration)
/// 3. Collect decisions and rejected alternatives
/// 4. When a file is given, filter decisions to those whose scope includes the file
/// 5. Deduplicate decisions by `what` field, keeping the most recent
pub fn query_decisions(
    git: &dyn GitOps,
    query: &DecisionsQuery,
) -> Result<DecisionsOutput, GitError> {
    let shas = match &query.file {
        Some(file) => git.log_for_file(file)?,
        None => git.list_annotated_commits(1000)?,
    };

    // Key: decision.what -> DecisionEntry (first match wins, newest first)
    let mut best_decisions: std::collections::HashMap<String, DecisionEntry> =
        std::collections::HashMap::new();
    // Key: (approach, reason) -> RejectedAlternativeEntry
    let mut best_rejected: std::collections::HashMap<String, RejectedAlternativeEntry> =
        std::collections::HashMap::new();

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation: v2::Annotation = match schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(e) => {
                tracing::debug!("skipping malformed annotation for {sha}: {e}");
                continue;
            }
        };

        // Collect decisions, optionally filtered by scope
        for decision in &annotation.decisions {
            if let Some(ref file) = query.file {
                if !decision_scope_matches(decision, file) {
                    continue;
                }
            }

            let stability_str = stability_to_string(&decision.stability);

            let key = decision.what.clone();
            best_decisions.entry(key).or_insert_with(|| DecisionEntry {
                what: decision.what.clone(),
                why: decision.why.clone(),
                stability: stability_str,
                revisit_when: decision.revisit_when.clone(),
                scope: decision.scope.clone(),
                commit: annotation.commit.clone(),
                timestamp: annotation.timestamp.clone(),
            });
        }

        // Collect rejected alternatives from narrative
        for rejected in &annotation.narrative.rejected_alternatives {
            // When filtering by file, only include rejected alternatives from
            // commits that touched the file (which is already the case since
            // we used log_for_file). For the no-file case, include all.
            let key = format!("{}:{}", rejected.approach, rejected.reason);
            best_rejected
                .entry(key)
                .or_insert_with(|| RejectedAlternativeEntry {
                    approach: rejected.approach.clone(),
                    reason: rejected.reason.clone(),
                    commit: annotation.commit.clone(),
                    timestamp: annotation.timestamp.clone(),
                });
        }
    }

    let mut decisions: Vec<DecisionEntry> = best_decisions.into_values().collect();
    decisions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let mut rejected_alternatives: Vec<RejectedAlternativeEntry> =
        best_rejected.into_values().collect();
    rejected_alternatives.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(DecisionsOutput {
        schema: "chronicle-decisions/v1".to_string(),
        decisions,
        rejected_alternatives,
    })
}

/// Check if a decision's scope matches the queried file.
///
/// A decision matches if:
/// - Its scope is empty (applies globally)
/// - Any scope entry starts with the file path or contains the file name
fn decision_scope_matches(decision: &v2::Decision, file: &str) -> bool {
    if decision.scope.is_empty() {
        return true;
    }
    let norm_file = file.strip_prefix("./").unwrap_or(file);
    decision.scope.iter().any(|s| {
        let norm_scope = s.strip_prefix("./").unwrap_or(s);
        // Scope entry could be "src/foo.rs:bar_fn" (file:anchor) or just "src/foo.rs"
        let scope_file = norm_scope.split(':').next().unwrap_or(norm_scope);
        scope_file == norm_file || norm_file.starts_with(scope_file)
    })
}

fn stability_to_string(stability: &v2::Stability) -> String {
    match stability {
        v2::Stability::Permanent => "permanent".to_string(),
        v2::Stability::Provisional => "provisional".to_string(),
        v2::Stability::Experimental => "experimental".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v1::{
        ContextLevel, CrossCuttingConcern, CrossCuttingRegionRef, Provenance, ProvenanceOperation,
        RegionAnnotation,
    };

    struct MockGitOps {
        file_log: Vec<String>,
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
            Ok(self.file_log.clone())
        }
        fn list_annotated_commits(&self, _limit: u32) -> Result<Vec<String>, GitError> {
            Ok(self.annotated_commits.clone())
        }
    }

    /// Build a v1 annotation JSON string. parse_annotation() will migrate
    /// it to v2, so cross_cutting concerns become v2 decisions.
    fn make_v1_annotation_with_cross_cutting(
        commit: &str,
        timestamp: &str,
        regions: Vec<RegionAnnotation>,
        cross_cutting: Vec<CrossCuttingConcern>,
    ) -> String {
        let ann = crate::schema::v1::Annotation {
            schema: "chronicle/v1".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions,
            cross_cutting,
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        };
        serde_json::to_string(&ann).unwrap()
    }

    fn make_region(file: &str, anchor: &str) -> RegionAnnotation {
        RegionAnnotation {
            file: file.to_string(),
            ast_anchor: AstAnchor {
                unit_type: "function".to_string(),
                name: anchor.to_string(),
                signature: None,
            },
            lines: LineRange { start: 1, end: 10 },
            intent: "test intent".to_string(),
            reasoning: None,
            constraints: vec![],
            semantic_dependencies: vec![],
            related_annotations: vec![],
            tags: vec![],
            risk_notes: None,
            corrections: vec![],
        }
    }

    #[test]
    fn test_decisions_from_v1_cross_cutting() {
        // v1 cross-cutting concerns migrate to v2 decisions
        let note = make_v1_annotation_with_cross_cutting(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main")],
            vec![CrossCuttingConcern {
                description: "All paths validate input".to_string(),
                regions: vec![CrossCuttingRegionRef {
                    file: "src/main.rs".to_string(),
                    anchor: "main".to_string(),
                }],
                tags: vec![],
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let query = DecisionsQuery {
            file: Some("src/main.rs".to_string()),
        };

        let result = query_decisions(&git, &query).unwrap();
        assert_eq!(result.schema, "chronicle-decisions/v1");
        assert_eq!(result.decisions.len(), 1);
        assert_eq!(result.decisions[0].what, "All paths validate input");
        assert_eq!(result.decisions[0].stability, "permanent");
        assert_eq!(result.decisions[0].commit, "commit1");
    }

    #[test]
    fn test_decisions_dedup_keeps_newest() {
        let note1 = make_v1_annotation_with_cross_cutting(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main")],
            vec![CrossCuttingConcern {
                description: "All paths validate input".to_string(),
                regions: vec![CrossCuttingRegionRef {
                    file: "src/main.rs".to_string(),
                    anchor: "main".to_string(),
                }],
                tags: vec![],
            }],
        );
        let note2 = make_v1_annotation_with_cross_cutting(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region("src/main.rs", "main")],
            vec![CrossCuttingConcern {
                description: "All paths validate input".to_string(),
                regions: vec![CrossCuttingRegionRef {
                    file: "src/main.rs".to_string(),
                    anchor: "main".to_string(),
                }],
                tags: vec![],
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);

        let git = MockGitOps {
            // newest first
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let query = DecisionsQuery {
            file: Some("src/main.rs".to_string()),
        };

        let result = query_decisions(&git, &query).unwrap();
        assert_eq!(result.decisions.len(), 1);
        assert_eq!(result.decisions[0].commit, "commit2");
        assert_eq!(result.decisions[0].timestamp, "2025-01-02T00:00:00Z");
    }

    #[test]
    fn test_decisions_scope_filter() {
        // Decision scoped to src/config.rs should not appear when querying src/main.rs
        let note = make_v1_annotation_with_cross_cutting(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main")],
            vec![CrossCuttingConcern {
                description: "Config must be reloaded".to_string(),
                regions: vec![CrossCuttingRegionRef {
                    file: "src/config.rs".to_string(),
                    anchor: "reload".to_string(),
                }],
                tags: vec![],
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let query = DecisionsQuery {
            file: Some("src/main.rs".to_string()),
        };

        let result = query_decisions(&git, &query).unwrap();
        // The migrated decision's scope is "src/config.rs:reload", which
        // doesn't match "src/main.rs", so it should be filtered out.
        assert_eq!(result.decisions.len(), 0);
    }

    #[test]
    fn test_decisions_no_file_returns_all() {
        let note = make_v1_annotation_with_cross_cutting(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main")],
            vec![CrossCuttingConcern {
                description: "All paths validate input".to_string(),
                regions: vec![CrossCuttingRegionRef {
                    file: "src/main.rs".to_string(),
                    anchor: "main".to_string(),
                }],
                tags: vec![],
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec![],
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        // No file filter: uses list_annotated_commits
        let query = DecisionsQuery { file: None };

        let result = query_decisions(&git, &query).unwrap();
        assert_eq!(result.decisions.len(), 1);
        assert_eq!(result.decisions[0].what, "All paths validate input");
    }

    #[test]
    fn test_decisions_empty_when_no_annotations() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let query = DecisionsQuery {
            file: Some("src/main.rs".to_string()),
        };

        let result = query_decisions(&git, &query).unwrap();
        assert!(result.decisions.is_empty());
        assert!(result.rejected_alternatives.is_empty());
    }

    #[test]
    fn test_decisions_with_native_v2_rejected_alternatives() {
        // Build a native v2 annotation with rejected_alternatives in the narrative
        let v2_ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "commit1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "Chose HashMap over BTreeMap".to_string(),
                motivation: None,
                rejected_alternatives: vec![v2::RejectedAlternative {
                    approach: "BTreeMap for ordered iteration".to_string(),
                    reason: "Lookup performance is more important than ordering".to_string(),
                }],
                follow_up: None,
                files_changed: vec!["src/store.rs".to_string()],
                sentiments: vec![],
            },
            decisions: vec![v2::Decision {
                what: "Use HashMap for the cache".to_string(),
                why: "O(1) lookups are critical for the hot path".to_string(),
                stability: v2::Stability::Provisional,
                revisit_when: Some("If we need sorted keys".to_string()),
                scope: vec!["src/store.rs".to_string()],
            }],
            markers: vec![],
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
            annotated_commits: vec![],
            notes,
        };

        let query = DecisionsQuery {
            file: Some("src/store.rs".to_string()),
        };

        let result = query_decisions(&git, &query).unwrap();

        assert_eq!(result.decisions.len(), 1);
        assert_eq!(result.decisions[0].what, "Use HashMap for the cache");
        assert_eq!(result.decisions[0].stability, "provisional");
        assert_eq!(
            result.decisions[0].revisit_when.as_deref(),
            Some("If we need sorted keys")
        );

        assert_eq!(result.rejected_alternatives.len(), 1);
        assert_eq!(
            result.rejected_alternatives[0].approach,
            "BTreeMap for ordered iteration"
        );
        assert_eq!(
            result.rejected_alternatives[0].reason,
            "Lookup performance is more important than ordering"
        );
    }

    #[test]
    fn test_decisions_output_serializable() {
        let output = DecisionsOutput {
            schema: "chronicle-decisions/v1".to_string(),
            decisions: vec![DecisionEntry {
                what: "Use HashMap".to_string(),
                why: "Performance".to_string(),
                stability: "provisional".to_string(),
                revisit_when: Some("If ordering needed".to_string()),
                scope: vec!["src/store.rs".to_string()],
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
            rejected_alternatives: vec![RejectedAlternativeEntry {
                approach: "BTreeMap".to_string(),
                reason: "Slower lookups".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("chronicle-decisions/v1"));
        assert!(json.contains("Use HashMap"));
        assert!(json.contains("BTreeMap"));
    }

    #[test]
    fn test_decision_scope_matches_helper() {
        let decision = v2::Decision {
            what: "test".to_string(),
            why: "test".to_string(),
            stability: v2::Stability::Permanent,
            revisit_when: None,
            scope: vec!["src/main.rs:main".to_string()],
        };

        assert!(decision_scope_matches(&decision, "src/main.rs"));
        assert!(!decision_scope_matches(&decision, "src/other.rs"));
    }

    #[test]
    fn test_decision_empty_scope_matches_any_file() {
        let decision = v2::Decision {
            what: "test".to_string(),
            why: "test".to_string(),
            stability: v2::Stability::Permanent,
            revisit_when: None,
            scope: vec![],
        };

        assert!(decision_scope_matches(&decision, "src/main.rs"));
        assert!(decision_scope_matches(&decision, "src/anything.rs"));
    }
}
