use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v3};

use super::matching::file_matches;

/// Query parameters: "What was decided and what was tried?"
#[derive(Debug, Clone)]
pub struct DecisionsQuery {
    pub file: Option<String>,
}

/// A decision entry extracted from insight wisdom entries.
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

/// A rejected alternative extracted from dead_end wisdom entries.
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
/// In v3, decisions come from `insight` wisdom entries whose content
/// matches the "{what}: {why}" pattern (from v2 decision migration),
/// and rejected alternatives come from `dead_end` wisdom entries whose
/// content matches the "{approach}: {reason}" pattern.
///
/// 1. Determine which commits to examine
/// 2. For each commit, parse annotation via `parse_annotation`
/// 3. Collect insight entries as decisions, dead_end entries as rejected alternatives
/// 4. When a file is given, filter by wisdom entry file scope
/// 5. Deduplicate by content, keeping the most recent
pub fn query_decisions(
    git: &dyn GitOps,
    query: &DecisionsQuery,
) -> Result<DecisionsOutput, GitError> {
    let shas = match &query.file {
        Some(file) => git.log_for_file(file)?,
        None => git.list_annotated_commits(1000)?,
    };

    // Key: content -> DecisionEntry (first match wins, newest first)
    let mut best_decisions: std::collections::HashMap<String, DecisionEntry> =
        std::collections::HashMap::new();
    // Key: content -> RejectedAlternativeEntry
    let mut best_rejected: std::collections::HashMap<String, RejectedAlternativeEntry> =
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

        for w in &annotation.wisdom {
            // If filtering by file, check scope
            if let Some(ref file) = query.file {
                if let Some(ref wf) = w.file {
                    if !file_matches(wf, file) && !file.starts_with(wf.as_str()) {
                        continue;
                    }
                }
                // Wisdom with no file scope is repo-wide, include it
            }

            match w.category {
                v3::WisdomCategory::Insight => {
                    // Parse "{what}: {why}" format from v2 decision migration
                    let (what, why) = if let Some((w_str, y_str)) = w.content.split_once(": ") {
                        (w_str.to_string(), y_str.to_string())
                    } else {
                        (w.content.clone(), String::new())
                    };

                    let scope = w.file.as_ref().map(|f| vec![f.clone()]).unwrap_or_default();

                    let key = w.content.clone();
                    best_decisions.entry(key).or_insert_with(|| DecisionEntry {
                        what,
                        why,
                        stability: "permanent".to_string(),
                        revisit_when: None,
                        scope,
                        commit: annotation.commit.clone(),
                        timestamp: annotation.timestamp.clone(),
                    });
                }
                v3::WisdomCategory::DeadEnd => {
                    // Parse "{approach}: {reason}" format from v2 rejected_alternatives migration
                    let (approach, reason) =
                        if let Some((a_str, r_str)) = w.content.split_once(": ") {
                            (a_str.to_string(), r_str.to_string())
                        } else {
                            (w.content.clone(), String::new())
                        };

                    let key = w.content.clone();
                    best_rejected
                        .entry(key)
                        .or_insert_with(|| RejectedAlternativeEntry {
                            approach,
                            reason,
                            commit: annotation.commit.clone(),
                            timestamp: annotation.timestamp.clone(),
                        });
                }
                _ => {}
            }
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
    /// it to v3, so cross_cutting concerns become insight wisdom entries.
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
        // v1 cross-cutting concerns migrate to insight wisdom entries
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
        // Build a native v2 annotation. parse_annotation() migrates it to v3.
        // parse_annotation() will migrate it to v3, where:
        //   - decisions become insight wisdom: "Use HashMap for the cache: O(1) lookups..."
        //   - rejected_alternatives become dead_end wisdom: "BTreeMap for ordered iteration: Lookup..."
        use crate::schema::v2;

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

        // v2 decision migrates to insight wisdom with "{what}: {why}" format
        assert_eq!(result.decisions.len(), 1);
        assert_eq!(result.decisions[0].what, "Use HashMap for the cache");
        assert_eq!(
            result.decisions[0].why,
            "O(1) lookups are critical for the hot path"
        );

        // v2 rejected_alternative migrates to dead_end wisdom with "{approach}: {reason}" format
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
}
