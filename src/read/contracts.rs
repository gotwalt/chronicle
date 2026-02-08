use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v3};

/// Query parameters: "What must I not break?" for a file/anchor.
#[derive(Debug, Clone)]
pub struct ContractsQuery {
    pub file: String,
    pub anchor: Option<String>,
}

/// Echo of the query in the output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContractsQueryEcho {
    pub file: String,
    pub anchor: Option<String>,
}

/// A contract entry extracted from gotcha wisdom entries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContractEntry {
    pub file: String,
    pub anchor: Option<String>,
    pub description: String,
    pub source: String,
    pub commit: String,
    pub timestamp: String,
}

/// A dependency entry extracted from insight wisdom entries with dependency content.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyEntry {
    pub file: String,
    pub anchor: Option<String>,
    pub target_file: String,
    pub target_anchor: String,
    pub assumption: String,
    pub commit: String,
    pub timestamp: String,
}

/// Output of a contracts query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContractsOutput {
    pub schema: String,
    pub query: ContractsQueryEcho,
    pub contracts: Vec<ContractEntry>,
    pub dependencies: Vec<DependencyEntry>,
}

/// Build a contracts-and-dependencies view for a file (or file+anchor).
///
/// In v3, contracts come from `gotcha` wisdom entries and dependencies
/// from `insight` wisdom entries whose content matches the dependency pattern
/// ("Depends on <file>:<anchor> — <assumption>").
///
/// 1. Get commits that touched the file via `log_for_file`
/// 2. For each commit, parse annotation via `parse_annotation`
/// 3. Filter wisdom entries matching the query file
/// 4. Deduplicate by keeping the most recent entry per unique key
pub fn query_contracts(
    git: &dyn GitOps,
    query: &ContractsQuery,
) -> Result<ContractsOutput, GitError> {
    let shas = git.log_for_file(&query.file)?;

    // Key: (file, description) -> ContractEntry
    let mut best_contracts: std::collections::HashMap<String, ContractEntry> =
        std::collections::HashMap::new();
    // Key: (file, target_file, target_anchor) -> DependencyEntry
    let mut best_deps: std::collections::HashMap<String, DependencyEntry> =
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
            // Only consider wisdom entries that match the queried file
            let entry_file = match &w.file {
                Some(f) => f,
                None => continue,
            };
            if !file_matches(entry_file, &query.file) {
                continue;
            }

            match w.category {
                v3::WisdomCategory::Gotcha => {
                    let key = format!("{}:{}", entry_file, w.content);
                    best_contracts.entry(key).or_insert_with(|| ContractEntry {
                        file: entry_file.clone(),
                        anchor: None,
                        description: w.content.clone(),
                        source: "author".to_string(),
                        commit: annotation.commit.clone(),
                        timestamp: annotation.timestamp.clone(),
                    });
                }
                v3::WisdomCategory::Insight => {
                    // Check if this insight is a dependency entry
                    // Migration produces: "Depends on {target_file}:{target_anchor} — {assumption}"
                    if let Some(dep) = parse_dependency_content(&w.content) {
                        let key = format!("{}:{}:{}", entry_file, dep.0, dep.1);
                        best_deps.entry(key).or_insert_with(|| DependencyEntry {
                            file: entry_file.clone(),
                            anchor: None,
                            target_file: dep.0.to_string(),
                            target_anchor: dep.1.to_string(),
                            assumption: dep.2.to_string(),
                            commit: annotation.commit.clone(),
                            timestamp: annotation.timestamp.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let mut contracts: Vec<ContractEntry> = best_contracts.into_values().collect();
    contracts.sort_by(|a, b| a.file.cmp(&b.file).then(a.description.cmp(&b.description)));

    let mut dependencies: Vec<DependencyEntry> = best_deps.into_values().collect();
    dependencies.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.target_file.cmp(&b.target_file))
            .then(a.target_anchor.cmp(&b.target_anchor))
    });

    Ok(ContractsOutput {
        schema: "chronicle-contracts/v1".to_string(),
        query: ContractsQueryEcho {
            file: query.file.clone(),
            anchor: query.anchor.clone(),
        },
        contracts,
        dependencies,
    })
}

use super::matching::file_matches;

/// Parse dependency content from the migration format:
/// "Depends on {file}:{anchor} — {assumption}"
/// Returns (target_file, target_anchor, assumption) if matched.
fn parse_dependency_content(content: &str) -> Option<(&str, &str, &str)> {
    let rest = content.strip_prefix("Depends on ")?;
    let (target, assumption) = rest.split_once(" — ")?;
    let (target_file, target_anchor) = target.split_once(':')?;
    Some((target_file, target_anchor, assumption))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v1::{
        Constraint, ConstraintSource, ContextLevel, Provenance, ProvenanceOperation,
        RegionAnnotation, SemanticDependency,
    };

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

    /// Build a v1 annotation (serialized as JSON). parse_annotation() will
    /// migrate it to v3, exercising the migration path in the test.
    fn make_v1_annotation(commit: &str, timestamp: &str, regions: Vec<RegionAnnotation>) -> String {
        let ann = crate::schema::v1::Annotation {
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
        };
        serde_json::to_string(&ann).unwrap()
    }

    fn make_region_with_contract(
        file: &str,
        anchor: &str,
        constraint_text: &str,
    ) -> RegionAnnotation {
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
            constraints: vec![Constraint {
                text: constraint_text.to_string(),
                source: ConstraintSource::Author,
            }],
            semantic_dependencies: vec![],
            related_annotations: vec![],
            tags: vec![],
            risk_notes: None,
            corrections: vec![],
        }
    }

    fn make_region_with_dependency(
        file: &str,
        anchor: &str,
        target_file: &str,
        target_anchor: &str,
        nature: &str,
    ) -> RegionAnnotation {
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
            semantic_dependencies: vec![SemanticDependency {
                file: target_file.to_string(),
                anchor: target_anchor.to_string(),
                nature: nature.to_string(),
            }],
            related_annotations: vec![],
            tags: vec![],
            risk_notes: None,
            corrections: vec![],
        }
    }

    #[test]
    fn test_contracts_from_v1_migration() {
        let note = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region_with_contract(
                "src/main.rs",
                "main",
                "must not panic",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert_eq!(result.schema, "chronicle-contracts/v1");
        assert_eq!(result.contracts.len(), 1);
        assert_eq!(result.contracts[0].description, "must not panic");
        assert_eq!(result.contracts[0].source, "author");
        assert_eq!(result.contracts[0].file, "src/main.rs");
        assert_eq!(result.contracts[0].anchor, None); // v3 wisdom entries lose named anchors after migration
        assert_eq!(result.contracts[0].commit, "commit1");
    }

    #[test]
    fn test_dependencies_from_v1_migration() {
        let note = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region_with_dependency(
                "src/main.rs",
                "main",
                "src/config.rs",
                "Config::load",
                "assumes Config::load returns defaults on missing file",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].target_file, "src/config.rs");
        assert_eq!(result.dependencies[0].target_anchor, "Config::load");
        assert_eq!(
            result.dependencies[0].assumption,
            "assumes Config::load returns defaults on missing file"
        );
    }

    #[test]
    fn test_contracts_with_anchor_filter() {
        let note = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                make_region_with_contract("src/main.rs", "main", "must not panic"),
                make_region_with_contract("src/main.rs", "helper", "must be pure"),
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
        };

        let result = query_contracts(&git, &query).unwrap();
        // v3 has no anchor-level filtering; both contracts for the file are returned
        assert_eq!(result.contracts.len(), 2);
    }

    #[test]
    fn test_contracts_dedup_keeps_newest() {
        // Two commits annotating the same function with the same constraint.
        // Newest first in git log, so commit2 should win.
        let note1 = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region_with_contract(
                "src/main.rs",
                "main",
                "must not panic",
            )],
        );
        let note2 = make_v1_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region_with_contract(
                "src/main.rs",
                "main",
                "must not panic",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);

        let git = MockGitOps {
            // newest first
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert_eq!(result.contracts.len(), 1);
        assert_eq!(result.contracts[0].commit, "commit2");
        assert_eq!(result.contracts[0].timestamp, "2025-01-02T00:00:00Z");
    }

    #[test]
    fn test_contracts_empty_when_no_annotations() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes: std::collections::HashMap::new(),
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert!(result.contracts.is_empty());
        assert!(result.dependencies.is_empty());
    }

    #[test]
    fn test_contracts_mixed_contracts_and_deps() {
        let region_contract = make_region_with_contract("src/main.rs", "main", "must not allocate");
        let region_dep = make_region_with_dependency(
            "src/main.rs",
            "main",
            "src/alloc.rs",
            "Allocator::new",
            "assumes Allocator::new never fails",
        );

        let note = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![region_contract, region_dep],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert_eq!(result.contracts.len(), 1);
        assert_eq!(result.contracts[0].description, "must not allocate");
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].target_file, "src/alloc.rs");
    }

    #[test]
    fn test_contracts_file_path_normalization() {
        let note = make_v1_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region_with_contract(
                "./src/main.rs",
                "main",
                "must not panic",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        // Query without "./" prefix should still match
        let query = ContractsQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = query_contracts(&git, &query).unwrap();
        assert_eq!(result.contracts.len(), 1);
    }

    #[test]
    fn test_contracts_output_serializable() {
        let output = ContractsOutput {
            schema: "chronicle-contracts/v1".to_string(),
            query: ContractsQueryEcho {
                file: "src/main.rs".to_string(),
                anchor: None,
            },
            contracts: vec![ContractEntry {
                file: "src/main.rs".to_string(),
                anchor: Some("main".to_string()),
                description: "must not panic".to_string(),
                source: "author".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
            }],
            dependencies: vec![],
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("chronicle-contracts/v1"));
        assert!(json.contains("must not panic"));
    }
}
