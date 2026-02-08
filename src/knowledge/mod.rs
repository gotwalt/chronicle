use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::knowledge::{FilteredKnowledge, KnowledgeStore};

/// The empty tree SHA — exists in every git repo. Used as the "commit" object
/// for the knowledge notes ref, since knowledge is repo-global, not per-commit.
const EMPTY_TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf899d15f13160d28";

/// The notes ref used to store the knowledge store.
pub const KNOWLEDGE_REF: &str = "refs/notes/chronicle-knowledge";

/// Read the knowledge store from git notes.
///
/// Returns a default (empty) store if no knowledge has been written yet.
pub fn read_store(git: &dyn GitOps) -> Result<KnowledgeStore, GitError> {
    let note = git.note_read(EMPTY_TREE_SHA)?;
    match note {
        Some(json) => {
            let store: KnowledgeStore = serde_json::from_str(&json).map_err(|e| {
                crate::error::git_error::CommandFailedSnafu {
                    message: format!("failed to parse knowledge store: {e}"),
                }
                .build()
            })?;
            Ok(store)
        }
        None => Ok(KnowledgeStore::new()),
    }
}

/// Write the knowledge store to git notes (atomic overwrite).
pub fn write_store(git: &dyn GitOps, store: &KnowledgeStore) -> Result<(), GitError> {
    let json = serde_json::to_string_pretty(store).map_err(|e| {
        crate::error::git_error::CommandFailedSnafu {
            message: format!("failed to serialize knowledge store: {e}"),
        }
        .build()
    })?;
    git.note_write(EMPTY_TREE_SHA, &json)
}

/// Filter the knowledge store to entries whose scope matches a file path.
///
/// Matching rules:
/// - A scope of "*" matches everything.
/// - A scope ending with "/" is a directory prefix match.
/// - Otherwise, exact match on the file path.
pub fn filter_by_scope(store: &KnowledgeStore, file: &str) -> FilteredKnowledge {
    let file_normalized = file.strip_prefix("./").unwrap_or(file);

    let conventions = store
        .conventions
        .iter()
        .filter(|c| scope_matches(&c.scope, file_normalized))
        .cloned()
        .collect();

    let boundaries = store
        .boundaries
        .iter()
        .filter(|b| scope_matches(&b.module, file_normalized))
        .cloned()
        .collect();

    // Anti-patterns are always global (no scope field), so return all of them.
    let anti_patterns = store.anti_patterns.clone();

    FilteredKnowledge {
        conventions,
        boundaries,
        anti_patterns,
    }
}

fn scope_matches(scope: &str, file: &str) -> bool {
    if scope == "*" {
        return true;
    }
    let scope_normalized = scope.strip_prefix("./").unwrap_or(scope);
    let file_normalized = file.strip_prefix("./").unwrap_or(file);
    if scope_normalized.ends_with('/') {
        file_normalized.starts_with(scope_normalized)
    } else {
        file_normalized == scope_normalized
            || file_normalized.starts_with(&format!("{scope_normalized}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::FileDiff;
    use crate::git::CommitInfo;
    use crate::schema::knowledge::{AntiPattern, Convention, ModuleBoundary};
    use crate::schema::v2::Stability;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockGitOps {
        notes: Mutex<HashMap<String, String>>,
    }

    impl MockGitOps {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        fn with_note(self, commit: &str, content: &str) -> Self {
            self.notes
                .lock()
                .unwrap()
                .insert(commit.to_string(), content.to_string());
            self
        }
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> Result<Vec<FileDiff>, GitError> {
            Ok(vec![])
        }
        fn note_read(&self, commit: &str) -> Result<Option<String>, GitError> {
            Ok(self.notes.lock().unwrap().get(commit).cloned())
        }
        fn note_write(&self, commit: &str, content: &str) -> Result<(), GitError> {
            self.notes
                .lock()
                .unwrap()
                .insert(commit.to_string(), content.to_string());
            Ok(())
        }
        fn note_exists(&self, commit: &str) -> Result<bool, GitError> {
            Ok(self.notes.lock().unwrap().contains_key(commit))
        }
        fn file_at_commit(
            &self,
            _path: &std::path::Path,
            _commit: &str,
        ) -> Result<String, GitError> {
            Ok(String::new())
        }
        fn commit_info(&self, _commit: &str) -> Result<CommitInfo, GitError> {
            Ok(CommitInfo {
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
        fn list_annotated_commits(&self, _limit: u32) -> Result<Vec<String>, GitError> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_read_empty_store() {
        let git = MockGitOps::new();
        let store = read_store(&git).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.schema, "chronicle/knowledge-v1");
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let git = MockGitOps::new();

        let mut store = KnowledgeStore::new();
        store.conventions.push(Convention {
            id: "conv-1".to_string(),
            scope: "src/schema/".to_string(),
            rule: "Use parse_annotation() for all deserialization".to_string(),
            decided_in: None,
            stability: Stability::Permanent,
        });

        write_store(&git, &store).unwrap();
        let loaded = read_store(&git).unwrap();
        assert_eq!(loaded.conventions.len(), 1);
        assert_eq!(loaded.conventions[0].id, "conv-1");
    }

    #[test]
    fn test_read_existing_store() {
        let store = KnowledgeStore {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: vec![Convention {
                id: "conv-1".to_string(),
                scope: "src/".to_string(),
                rule: "Test rule".to_string(),
                decided_in: None,
                stability: Stability::Provisional,
            }],
            boundaries: vec![],
            anti_patterns: vec![],
        };
        let json = serde_json::to_string(&store).unwrap();
        let git = MockGitOps::new().with_note(EMPTY_TREE_SHA, &json);

        let loaded = read_store(&git).unwrap();
        assert_eq!(loaded.conventions.len(), 1);
    }

    #[test]
    fn test_filter_by_scope_directory_prefix() {
        let store = KnowledgeStore {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: vec![
                Convention {
                    id: "conv-1".to_string(),
                    scope: "src/schema/".to_string(),
                    rule: "Schema rule".to_string(),
                    decided_in: None,
                    stability: Stability::Permanent,
                },
                Convention {
                    id: "conv-2".to_string(),
                    scope: "src/git/".to_string(),
                    rule: "Git rule".to_string(),
                    decided_in: None,
                    stability: Stability::Permanent,
                },
            ],
            boundaries: vec![],
            anti_patterns: vec![AntiPattern {
                id: "ap-1".to_string(),
                pattern: "bad".to_string(),
                instead: "good".to_string(),
                learned_from: None,
            }],
        };

        let filtered = filter_by_scope(&store, "src/schema/v2.rs");
        assert_eq!(filtered.conventions.len(), 1);
        assert_eq!(filtered.conventions[0].id, "conv-1");
        // Anti-patterns are always returned
        assert_eq!(filtered.anti_patterns.len(), 1);
    }

    #[test]
    fn test_filter_by_scope_wildcard() {
        let store = KnowledgeStore {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: vec![Convention {
                id: "conv-1".to_string(),
                scope: "*".to_string(),
                rule: "Global rule".to_string(),
                decided_in: None,
                stability: Stability::Permanent,
            }],
            boundaries: vec![],
            anti_patterns: vec![],
        };

        let filtered = filter_by_scope(&store, "any/file.rs");
        assert_eq!(filtered.conventions.len(), 1);
    }

    #[test]
    fn test_filter_by_scope_no_match() {
        let store = KnowledgeStore {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: vec![Convention {
                id: "conv-1".to_string(),
                scope: "src/git/".to_string(),
                rule: "Git rule".to_string(),
                decided_in: None,
                stability: Stability::Permanent,
            }],
            boundaries: vec![],
            anti_patterns: vec![],
        };

        let filtered = filter_by_scope(&store, "src/schema/v2.rs");
        assert!(filtered.conventions.is_empty());
    }

    #[test]
    fn test_scope_matches_normalization() {
        assert!(scope_matches("src/", "./src/foo.rs"));
        assert!(scope_matches("./src/", "src/foo.rs"));
    }

    #[test]
    fn test_filter_boundaries_by_module() {
        let store = KnowledgeStore {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: vec![],
            boundaries: vec![
                ModuleBoundary {
                    id: "b-1".to_string(),
                    module: "src/git/".to_string(),
                    owns: "Git operations".to_string(),
                    boundary: "No provider imports".to_string(),
                    decided_in: None,
                },
                ModuleBoundary {
                    id: "b-2".to_string(),
                    module: "src/provider/".to_string(),
                    owns: "LLM providers".to_string(),
                    boundary: "No git imports".to_string(),
                    decided_in: None,
                },
            ],
            anti_patterns: vec![],
        };

        let filtered = filter_by_scope(&store, "src/git/cli_ops.rs");
        assert_eq!(filtered.boundaries.len(), 1);
        assert_eq!(filtered.boundaries[0].id, "b-1");
    }
}
