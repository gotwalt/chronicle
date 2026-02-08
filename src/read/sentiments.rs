use crate::error::GitError;
use crate::git::GitOps;
use crate::schema;

/// Query parameters for sentiments lookup.
#[derive(Debug, Clone)]
pub struct SentimentsQuery {
    pub file: Option<String>,
}

/// A sentiment entry extracted from a v2 `Narrative.sentiments`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SentimentEntry {
    pub feeling: String,
    pub detail: String,
    pub commit: String,
    pub timestamp: String,
    pub summary: String,
}

/// Output of a sentiments query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SentimentsOutput {
    pub schema: String,
    pub sentiments: Vec<SentimentEntry>,
}

/// Collect sentiments from annotations across the repository.
///
/// 1. Determine which commits to examine:
///    - If a file is specified, use `log_for_file` to get commits touching that file
///    - Otherwise, use `list_annotated_commits` to scan all annotated commits
/// 2. For each commit, parse annotation via `parse_annotation` (handles v1 migration)
/// 3. Collect sentiments from `annotation.narrative.sentiments`
/// 4. Return newest-first
pub fn query_sentiments(
    git: &dyn GitOps,
    query: &SentimentsQuery,
) -> Result<SentimentsOutput, GitError> {
    let shas = match &query.file {
        Some(file) => git.log_for_file(file)?,
        None => git.list_annotated_commits(1000)?,
    };

    let mut sentiments = Vec::new();

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

        for sentiment in &annotation.narrative.sentiments {
            sentiments.push(SentimentEntry {
                feeling: sentiment.feeling.clone(),
                detail: sentiment.detail.clone(),
                commit: annotation.commit.clone(),
                timestamp: annotation.timestamp.clone(),
                summary: annotation.narrative.summary.clone(),
            });
        }
    }

    // Already newest-first from git log order, but sort to be sure
    sentiments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(SentimentsOutput {
        schema: "chronicle-sentiments/v1".to_string(),
        sentiments,
    })
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

    fn make_v2_with_sentiments(
        commit: &str,
        timestamp: &str,
        sentiments: Vec<v2::Sentiment>,
    ) -> String {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            narrative: v2::Narrative {
                summary: "Test summary".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec!["src/main.rs".to_string()],
                sentiments,
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
        serde_json::to_string(&ann).unwrap()
    }

    #[test]
    fn test_sentiments_collected_from_annotations() {
        let note = make_v2_with_sentiments(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                v2::Sentiment {
                    feeling: "confidence".to_string(),
                    detail: "This approach is well-tested".to_string(),
                },
                v2::Sentiment {
                    feeling: "worry".to_string(),
                    detail: "Performance might degrade under load".to_string(),
                },
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec![],
            annotated_commits: vec!["commit1".to_string()],
            notes,
        };

        let result = query_sentiments(&git, &SentimentsQuery { file: None }).unwrap();
        assert_eq!(result.schema, "chronicle-sentiments/v1");
        assert_eq!(result.sentiments.len(), 2);
        assert_eq!(result.sentiments[0].feeling, "confidence");
        assert_eq!(result.sentiments[1].feeling, "worry");
        assert_eq!(result.sentiments[0].summary, "Test summary");
    }

    #[test]
    fn test_sentiments_filtered_by_file() {
        let note = make_v2_with_sentiments(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![v2::Sentiment {
                feeling: "curiosity".to_string(),
                detail: "Interesting edge case".to_string(),
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes,
        };

        let result = query_sentiments(
            &git,
            &SentimentsQuery {
                file: Some("src/main.rs".to_string()),
            },
        )
        .unwrap();
        assert_eq!(result.sentiments.len(), 1);
        assert_eq!(result.sentiments[0].feeling, "curiosity");
    }

    #[test]
    fn test_sentiments_empty_when_no_annotations() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let result = query_sentiments(
            &git,
            &SentimentsQuery {
                file: Some("src/main.rs".to_string()),
            },
        )
        .unwrap();
        assert!(result.sentiments.is_empty());
    }

    #[test]
    fn test_sentiments_newest_first() {
        let note1 = make_v2_with_sentiments(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![v2::Sentiment {
                feeling: "doubt".to_string(),
                detail: "Not sure about this".to_string(),
            }],
        );
        let note2 = make_v2_with_sentiments(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![v2::Sentiment {
                feeling: "confidence".to_string(),
                detail: "This works well".to_string(),
            }],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);

        let git = MockGitOps {
            file_log: vec![],
            annotated_commits: vec!["commit1".to_string(), "commit2".to_string()],
            notes,
        };

        let result = query_sentiments(&git, &SentimentsQuery { file: None }).unwrap();
        assert_eq!(result.sentiments.len(), 2);
        assert_eq!(result.sentiments[0].feeling, "confidence");
        assert_eq!(result.sentiments[0].timestamp, "2025-01-02T00:00:00Z");
        assert_eq!(result.sentiments[1].feeling, "doubt");
    }

    #[test]
    fn test_sentiments_output_serializable() {
        let output = SentimentsOutput {
            schema: "chronicle-sentiments/v1".to_string(),
            sentiments: vec![SentimentEntry {
                feeling: "worry".to_string(),
                detail: "Edge case not covered".to_string(),
                commit: "abc123".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                summary: "Added error handling".to_string(),
            }],
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("chronicle-sentiments/v1"));
        assert!(json.contains("worry"));
    }
}
