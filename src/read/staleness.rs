use crate::error::GitError;
use crate::git::GitOps;

/// Default threshold: an annotation is considered stale if more than 5
/// commits have touched the file since the annotation was written.
const DEFAULT_STALENESS_THRESHOLD: usize = 5;

/// Staleness information for a single annotation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StalenessInfo {
    pub annotation_commit: String,
    pub latest_file_commit: String,
    pub commits_since: usize,
    pub stale: bool,
}

/// Compute staleness for an annotation on a given file.
///
/// Returns `None` if the annotation commit isn't in the file's history
/// (e.g., the file was renamed).
pub fn compute_staleness(
    git: &dyn GitOps,
    file: &str,
    annotation_commit: &str,
) -> Result<Option<StalenessInfo>, GitError> {
    compute_staleness_with_threshold(git, file, annotation_commit, DEFAULT_STALENESS_THRESHOLD)
}

/// Compute staleness with a custom threshold.
pub fn compute_staleness_with_threshold(
    git: &dyn GitOps,
    file: &str,
    annotation_commit: &str,
    threshold: usize,
) -> Result<Option<StalenessInfo>, GitError> {
    let shas = git.log_for_file(file)?;
    if shas.is_empty() {
        return Ok(None);
    }

    let latest = shas[0].clone();

    // Find the position of the annotation commit in the file's history.
    // shas are ordered newest-first, so position 0 = HEAD of the file.
    let position = shas.iter().position(|sha| sha == annotation_commit);

    match position {
        Some(pos) => Ok(Some(StalenessInfo {
            annotation_commit: annotation_commit.to_string(),
            latest_file_commit: latest,
            commits_since: pos,
            stale: pos > threshold,
        })),
        None => {
            // Annotation commit not found in file history — could be
            // a renamed file or the commit didn't touch this file directly.
            // Treat as stale (the annotation is about a different version).
            Ok(Some(StalenessInfo {
                annotation_commit: annotation_commit.to_string(),
                latest_file_commit: latest,
                commits_since: shas.len(),
                stale: true,
            }))
        }
    }
}

/// Scan annotated commits and report staleness across the repo.
pub fn scan_staleness(
    git: &dyn GitOps,
    limit: u32,
) -> Result<StalenessReport, GitError> {
    let annotated = git.list_annotated_commits(limit)?;
    let mut total_annotations = 0usize;
    let mut stale_count = 0usize;
    let mut stale_files: Vec<StaleFileEntry> = Vec::new();

    for sha in &annotated {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation = match crate::schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(e) => {
                tracing::debug!("skipping malformed annotation for {sha}: {e}");
                continue;
            }
        };

        total_annotations += 1;

        for file in &annotation.narrative.files_changed {
            if let Some(info) = compute_staleness(git, file, &annotation.commit)? {
                if info.stale {
                    stale_count += 1;
                    stale_files.push(StaleFileEntry {
                        file: file.clone(),
                        annotation_commit: annotation.commit.clone(),
                        commits_since: info.commits_since,
                    });
                }
            }
        }
    }

    Ok(StalenessReport {
        total_annotations,
        stale_count,
        stale_files,
    })
}

/// Summary report of staleness across the repo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StalenessReport {
    pub total_annotations: usize,
    pub stale_count: usize,
    pub stale_files: Vec<StaleFileEntry>,
}

/// A single stale file entry in the report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StaleFileEntry {
    pub file: String,
    pub annotation_commit: String,
    pub commits_since: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::CommitInfo;
    use crate::git::diff::FileDiff;

    struct MockGitOps {
        file_log: Vec<String>,
        annotated_commits: Vec<String>,
        notes: std::collections::HashMap<String, String>,
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> Result<Vec<FileDiff>, GitError> {
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
        fn commit_info(&self, commit: &str) -> Result<CommitInfo, GitError> {
            Ok(CommitInfo {
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
    fn test_staleness_fresh_annotation() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness(&git, "src/main.rs", "commit1")
            .unwrap()
            .unwrap();
        assert_eq!(info.commits_since, 0);
        assert!(!info.stale);
    }

    #[test]
    fn test_staleness_annotation_is_stale() {
        // 7 commits newer than the annotation commit
        let git = MockGitOps {
            file_log: vec![
                "c7".to_string(),
                "c6".to_string(),
                "c5".to_string(),
                "c4".to_string(),
                "c3".to_string(),
                "c2".to_string(),
                "c1".to_string(),
                "c0".to_string(), // annotation commit at position 7
            ],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness(&git, "src/main.rs", "c0")
            .unwrap()
            .unwrap();
        assert_eq!(info.commits_since, 7);
        assert!(info.stale);
        assert_eq!(info.latest_file_commit, "c7");
    }

    #[test]
    fn test_staleness_just_under_threshold() {
        // 5 commits newer = exactly at threshold, not stale
        let git = MockGitOps {
            file_log: vec![
                "c5".to_string(),
                "c4".to_string(),
                "c3".to_string(),
                "c2".to_string(),
                "c1".to_string(),
                "c0".to_string(),
            ],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness(&git, "src/main.rs", "c0")
            .unwrap()
            .unwrap();
        assert_eq!(info.commits_since, 5);
        assert!(!info.stale); // exactly at threshold, not over
    }

    #[test]
    fn test_staleness_empty_file_log() {
        let git = MockGitOps {
            file_log: vec![],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness(&git, "src/main.rs", "commit1").unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_staleness_commit_not_in_history() {
        let git = MockGitOps {
            file_log: vec!["other_commit".to_string()],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness(&git, "src/main.rs", "missing_commit")
            .unwrap()
            .unwrap();
        assert!(info.stale);
        assert_eq!(info.commits_since, 1);
    }

    #[test]
    fn test_custom_threshold() {
        let git = MockGitOps {
            file_log: vec![
                "c2".to_string(),
                "c1".to_string(),
                "c0".to_string(),
            ],
            annotated_commits: vec![],
            notes: std::collections::HashMap::new(),
        };

        let info = compute_staleness_with_threshold(&git, "src/main.rs", "c0", 1)
            .unwrap()
            .unwrap();
        assert_eq!(info.commits_since, 2);
        assert!(info.stale); // 2 > 1
    }
}
