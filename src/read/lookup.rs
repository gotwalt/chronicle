use crate::error::GitError;
use crate::git::GitOps;
use crate::knowledge;
use crate::read::{contracts, decisions, history, staleness};
use crate::schema;
use crate::schema::knowledge::FilteredKnowledge;

/// Output of the composite lookup query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LookupOutput {
    pub schema: String,
    pub file: String,
    pub contracts: Vec<contracts::ContractEntry>,
    pub dependencies: Vec<contracts::DependencyEntry>,
    pub decisions: Vec<decisions::DecisionEntry>,
    pub recent_history: Vec<history::TimelineEntry>,
    pub open_follow_ups: Vec<FollowUpEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staleness: Vec<staleness::StalenessInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<FilteredKnowledge>,
}

/// A follow-up entry from a recent annotation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FollowUpEntry {
    pub commit: String,
    pub follow_up: String,
}

/// Build a composite context view for a file (contracts + decisions + history + follow-ups).
pub fn build_lookup(
    git: &dyn GitOps,
    file: &str,
    anchor: Option<&str>,
) -> Result<LookupOutput, GitError> {
    // 1. Contracts
    let contracts_out = contracts::query_contracts(
        git,
        &contracts::ContractsQuery {
            file: file.to_string(),
            anchor: anchor.map(|s| s.to_string()),
        },
    )?;

    // 2. Decisions
    let decisions_out = decisions::query_decisions(
        git,
        &decisions::DecisionsQuery {
            file: Some(file.to_string()),
        },
    )?;

    // 3. Recent history (limit 3)
    let history_out = history::build_timeline(
        git,
        &history::HistoryQuery {
            file: file.to_string(),
            anchor: anchor.map(|s| s.to_string()),
            limit: 3,
            follow_related: false,
        },
    )?;

    // 4. Follow-ups from recent annotations
    let follow_ups = collect_follow_ups(git, file)?;

    // 5. Staleness: for recent annotated commits, compute how stale each is
    let mut staleness_infos = Vec::new();
    for entry in &history_out.timeline {
        if let Some(info) = staleness::compute_staleness(git, file, &entry.commit)? {
            staleness_infos.push(info);
        }
    }

    // 6. Knowledge: filter store by file scope (best-effort, don't fail lookup)
    let knowledge_filtered = knowledge::read_store(git)
        .ok()
        .map(|store| knowledge::filter_by_scope(&store, file))
        .filter(|k| !k.is_empty());

    Ok(LookupOutput {
        schema: "chronicle-lookup/v1".to_string(),
        file: file.to_string(),
        contracts: contracts_out.contracts,
        dependencies: contracts_out.dependencies,
        decisions: decisions_out.decisions,
        recent_history: history_out.timeline,
        open_follow_ups: follow_ups,
        staleness: staleness_infos,
        knowledge: knowledge_filtered,
    })
}

fn collect_follow_ups(git: &dyn GitOps, file: &str) -> Result<Vec<FollowUpEntry>, GitError> {
    let shas = git.log_for_file(file)?;
    let mut follow_ups = Vec::new();

    for sha in shas.iter().take(10) {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };
        let annotation = match schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(_) => continue,
        };
        if let Some(fu) = &annotation.narrative.follow_up {
            follow_ups.push(FollowUpEntry {
                commit: sha.clone(),
                follow_up: fu.clone(),
            });
        }
    }

    Ok(follow_ups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::v2;

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
        fn commit_info(&self, commit: &str) -> Result<crate::git::CommitInfo, GitError> {
            Ok(crate::git::CommitInfo {
                sha: commit.to_string(),
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

    #[test]
    fn test_lookup_empty() {
        let git = MockGitOps {
            file_log: vec![],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let result = build_lookup(&git, "src/main.rs", None).unwrap();
        assert_eq!(result.schema, "chronicle-lookup/v1");
        assert_eq!(result.file, "src/main.rs");
        assert!(result.contracts.is_empty());
        assert!(result.dependencies.is_empty());
        assert!(result.decisions.is_empty());
        assert!(result.recent_history.is_empty());
        assert!(result.open_follow_ups.is_empty());
    }

    #[test]
    fn test_lookup_collects_follow_ups() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "commit1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "test change".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: Some("Need to add error handling".to_string()),
                files_changed: vec!["src/main.rs".to_string()],
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

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let result = build_lookup(&git, "src/main.rs", None).unwrap();
        assert_eq!(result.open_follow_ups.len(), 1);
        assert_eq!(
            result.open_follow_ups[0].follow_up,
            "Need to add error handling"
        );
        assert_eq!(result.open_follow_ups[0].commit, "commit1");
    }

    #[test]
    fn test_lookup_combines_contracts_and_history() {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "commit1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "add validation".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string()],
            },
            decisions: vec![],
            markers: vec![v2::CodeMarker {
                file: "src/main.rs".to_string(),
                anchor: Some(crate::schema::common::AstAnchor {
                    unit_type: "fn".to_string(),
                    name: "validate".to_string(),
                    signature: None,
                }),
                lines: None,
                kind: v2::MarkerKind::Contract {
                    description: "must not panic".to_string(),
                    source: v2::ContractSource::Author,
                },
            }],
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: None,
                derived_from: vec![],
                notes: None,
            },
        };

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let result = build_lookup(&git, "src/main.rs", None).unwrap();
        assert_eq!(result.contracts.len(), 1);
        assert_eq!(result.contracts[0].description, "must not panic");
        assert_eq!(result.recent_history.len(), 1);
        assert_eq!(result.recent_history[0].intent, "add validation");
    }
}
