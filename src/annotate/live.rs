use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::error::{chronicle_error, Result};
use crate::git::GitOps;
use crate::schema::common::LineRange;
use crate::schema::v3;

// ---------------------------------------------------------------------------
// Input types (v3 live path)
// ---------------------------------------------------------------------------

/// Input for the v3 live annotation path.
///
/// Designed for minimal friction. Most commits need only two fields:
/// ```json
/// { "commit": "HEAD", "summary": "What and why." }
/// ```
///
/// Rich annotation when warranted:
/// ```json
/// {
///   "commit": "HEAD",
///   "summary": "...",
///   "wisdom": [
///     {"category": "gotcha", "content": "...", "file": "src/foo.rs"},
///     {"category": "dead_end", "content": "Tried X but it failed because Y"}
///   ]
/// }
/// ```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LiveInput {
    pub commit: String,

    /// What this commit does and why.
    pub summary: String,

    /// Accumulated wisdom entries — dead ends, gotchas, insights, threads.
    #[serde(default)]
    pub wisdom: Vec<WisdomEntryInput>,

    /// Pre-loaded staged notes text (appended to provenance.notes).
    /// Not part of the user-facing JSON schema; populated by the CLI layer.
    #[serde(skip)]
    pub staged_notes: Option<String>,
}

/// A single wisdom entry from the caller.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WisdomEntryInput {
    pub category: v3::WisdomCategory,
    pub content: String,
    pub file: Option<String>,
    pub lines: Option<LineRange>,
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result returned after writing a v3 annotation.
#[derive(Debug, Clone, Serialize)]
pub struct LiveResult {
    pub success: bool,
    pub commit: String,
    pub wisdom_written: usize,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Quality checks (non-blocking warnings)
// ---------------------------------------------------------------------------

fn check_quality(input: &LiveInput, files_changed: &[String], commit_message: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    if input.summary.len() < 20 {
        warnings.push("Summary is very short — consider adding more detail".to_string());
    }

    if files_changed.len() > 3 && input.wisdom.is_empty() {
        warnings.push(
            "Multi-file change without wisdom — consider adding gotchas or insights".to_string(),
        );
    }

    if input.summary.trim() == commit_message.trim() {
        warnings.push(
            "Summary matches commit message verbatim — consider adding why this approach was chosen"
                .to_string(),
        );
    }

    warnings
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Core v3 handler: validates input, builds and writes a v3 annotation.
pub fn handle_annotate_v3(git_ops: &dyn GitOps, input: LiveInput) -> Result<LiveResult> {
    let full_sha = git_ops
        .resolve_ref(&input.commit)
        .context(chronicle_error::GitSnafu)?;

    let mut warnings = Vec::new();
    if git_ops
        .note_exists(&full_sha)
        .context(chronicle_error::GitSnafu)?
    {
        warnings.push(format!(
            "Overwriting existing annotation for {}",
            &full_sha[..full_sha.len().min(8)]
        ));
    }

    let files_changed = {
        let diffs = git_ops.diff(&full_sha).context(chronicle_error::GitSnafu)?;
        diffs.into_iter().map(|d| d.path).collect::<Vec<_>>()
    };

    let commit_message = git_ops
        .commit_info(&full_sha)
        .context(chronicle_error::GitSnafu)?
        .message;
    warnings.extend(check_quality(&input, &files_changed, &commit_message));

    let wisdom: Vec<v3::WisdomEntry> = input
        .wisdom
        .iter()
        .map(|w| v3::WisdomEntry {
            category: w.category.clone(),
            content: w.content.clone(),
            file: w.file.clone(),
            lines: w.lines,
        })
        .collect();

    let wisdom_count = wisdom.len();

    let annotation = v3::Annotation {
        schema: "chronicle/v3".to_string(),
        commit: full_sha.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        summary: input.summary.clone(),
        wisdom,
        provenance: v3::Provenance {
            source: v3::ProvenanceSource::Live,
            author: git_ops
                .config_get("chronicle.author")
                .ok()
                .flatten()
                .or_else(|| git_ops.config_get("user.name").ok().flatten()),
            derived_from: Vec::new(),
            notes: input.staged_notes.clone(),
        },
    };

    annotation
        .validate()
        .map_err(|msg| crate::error::ChronicleError::Validation {
            message: msg,
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    let json = serde_json::to_string_pretty(&annotation).context(chronicle_error::JsonSnafu)?;
    git_ops
        .note_write(&full_sha, &json)
        .context(chronicle_error::GitSnafu)?;

    Ok(LiveResult {
        success: true,
        commit: full_sha,
        wisdom_written: wisdom_count,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GitError;
    use crate::git::diff::{DiffStatus, FileDiff};
    use crate::git::CommitInfo;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    fn test_diff(path: &str) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            old_path: None,
            status: DiffStatus::Modified,
            hunks: vec![],
        }
    }

    struct MockGitOps {
        resolved_sha: String,
        files: HashMap<String, String>,
        diffs: Vec<FileDiff>,
        written_notes: Mutex<Vec<(String, String)>>,
        note_exists_result: bool,
        commit_message: String,
    }

    impl MockGitOps {
        fn new(sha: &str) -> Self {
            Self {
                resolved_sha: sha.to_string(),
                files: HashMap::new(),
                diffs: Vec::new(),
                written_notes: Mutex::new(Vec::new()),
                note_exists_result: false,
                commit_message: "test commit".to_string(),
            }
        }

        fn with_diffs(mut self, diffs: Vec<FileDiff>) -> Self {
            self.diffs = diffs;
            self
        }

        fn with_note_exists(mut self, exists: bool) -> Self {
            self.note_exists_result = exists;
            self
        }

        fn with_commit_message(mut self, msg: &str) -> Self {
            self.commit_message = msg.to_string();
            self
        }

        fn written_notes(&self) -> Vec<(String, String)> {
            self.written_notes.lock().unwrap().clone()
        }
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> std::result::Result<Vec<FileDiff>, GitError> {
            Ok(self.diffs.clone())
        }
        fn note_read(&self, _commit: &str) -> std::result::Result<Option<String>, GitError> {
            Ok(None)
        }
        fn note_write(&self, commit: &str, content: &str) -> std::result::Result<(), GitError> {
            self.written_notes
                .lock()
                .unwrap()
                .push((commit.to_string(), content.to_string()));
            Ok(())
        }
        fn note_exists(&self, _commit: &str) -> std::result::Result<bool, GitError> {
            Ok(self.note_exists_result)
        }
        fn file_at_commit(
            &self,
            path: &Path,
            _commit: &str,
        ) -> std::result::Result<String, GitError> {
            self.files
                .get(path.to_str().unwrap_or(""))
                .cloned()
                .ok_or(GitError::FileNotFound {
                    path: path.display().to_string(),
                    commit: "test".to_string(),
                    location: snafu::Location::new(file!(), line!(), 0),
                })
        }
        fn commit_info(&self, _commit: &str) -> std::result::Result<CommitInfo, GitError> {
            Ok(CommitInfo {
                sha: self.resolved_sha.clone(),
                message: self.commit_message.clone(),
                author_name: "Test".to_string(),
                author_email: "test@test.com".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                parent_shas: Vec::new(),
            })
        }
        fn resolve_ref(&self, _refspec: &str) -> std::result::Result<String, GitError> {
            Ok(self.resolved_sha.clone())
        }
        fn config_get(&self, _key: &str) -> std::result::Result<Option<String>, GitError> {
            Ok(None)
        }
        fn config_set(&self, _key: &str, _value: &str) -> std::result::Result<(), GitError> {
            Ok(())
        }
        fn log_for_file(&self, _path: &str) -> std::result::Result<Vec<String>, GitError> {
            Ok(vec![])
        }
        fn list_annotated_commits(
            &self,
            _limit: u32,
        ) -> std::result::Result<Vec<String>, GitError> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_minimal_input() {
        let json =
            r#"{"commit": "HEAD", "summary": "Switch to exponential backoff for MQTT reconnect"}"#;
        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.commit, "HEAD");
        assert!(input.wisdom.is_empty());
    }

    #[test]
    fn test_rich_input() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Redesign annotation schema",
            "wisdom": [
                {"category": "dead_end", "content": "Tried migrating all notes in bulk"},
                {"category": "gotcha", "content": "Must not exceed 60s backoff", "file": "src/reconnect.rs"},
                {"category": "insight", "content": "HashMap is O(1) for cache lookups", "file": "src/cache.rs", "lines": {"start": 10, "end": 20}},
                {"category": "unfinished_thread", "content": "Need to add jitter to the backoff"}
            ]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.wisdom.len(), 4);
        assert_eq!(input.wisdom[0].category, v3::WisdomCategory::DeadEnd);
        assert_eq!(input.wisdom[1].category, v3::WisdomCategory::Gotcha);
        assert_eq!(input.wisdom[2].category, v3::WisdomCategory::Insight);
        assert_eq!(input.wisdom[3].category, v3::WisdomCategory::UnfinishedThread);
        assert_eq!(input.wisdom[1].file.as_deref(), Some("src/reconnect.rs"));
        assert_eq!(input.wisdom[2].lines, Some(LineRange { start: 10, end: 20 }));
    }

    #[test]
    fn test_handle_annotate_v3_minimal() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.commit, "abc123def456");
        assert_eq!(result.wisdom_written, 0);

        let notes = mock.written_notes();
        assert_eq!(notes.len(), 1);
        let annotation: v3::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.schema, "chronicle/v3");
        assert_eq!(
            annotation.summary,
            "Add hello_world function and Config struct"
        );
        assert_eq!(annotation.provenance.source, v3::ProvenanceSource::Live);
    }

    #[test]
    fn test_handle_annotate_v3_with_wisdom() {
        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            wisdom: vec![WisdomEntryInput {
                category: v3::WisdomCategory::Gotcha,
                content: "Must print to stdout".to_string(),
                file: Some("src/lib.rs".to_string()),
                lines: Some(LineRange { start: 2, end: 4 }),
            }],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.wisdom_written, 1);

        let notes = mock.written_notes();
        let annotation: v3::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.wisdom.len(), 1);
        assert_eq!(annotation.wisdom[0].category, v3::WisdomCategory::Gotcha);
        assert_eq!(annotation.wisdom[0].content, "Must print to stdout");
        assert_eq!(annotation.wisdom[0].file.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn test_validation_rejects_empty_summary() {
        let mock = MockGitOps::new("abc123");

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input);
        assert!(result.is_err());
    }

    #[test]
    fn test_overwrite_existing_note_warns() {
        let mock = MockGitOps::new("abc123de")
            .with_diffs(vec![test_diff("src/lib.rs")])
            .with_note_exists(true);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(result.success);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Overwriting existing annotation")),
            "Expected overwrite warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_no_overwrite_warning_when_no_existing_note() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("Overwriting")),
            "Should not have overwrite warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_quality_multi_file_without_wisdom() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![
            test_diff("src/a.rs"),
            test_diff("src/b.rs"),
            test_diff("src/c.rs"),
            test_diff("src/d.rs"),
        ]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Refactor multiple modules for consistency".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Multi-file change without wisdom")),
            "Expected multi-file wisdom warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_quality_summary_matches_commit_message() {
        let mock = MockGitOps::new("abc123def456")
            .with_diffs(vec![test_diff("src/lib.rs")])
            .with_commit_message("Fix the bug in parser");

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Fix the bug in parser".to_string(),
            wisdom: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Summary matches commit message verbatim")),
            "Expected verbatim summary warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_wisdom_entry_roundtrip() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Test all wisdom categories for round-trip serialization",
            "wisdom": [
                {"category": "dead_end", "content": "Tried approach X"},
                {"category": "gotcha", "content": "Must validate input before processing", "file": "src/input.rs"},
                {"category": "insight", "content": "HashMap gives O(1) lookups", "file": "src/cache.rs", "lines": {"start": 10, "end": 20}},
                {"category": "unfinished_thread", "content": "Need to add jitter"}
            ]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.wisdom.len(), 4);

        let mock = MockGitOps::new("abc123").with_diffs(vec![
            test_diff("src/input.rs"),
            test_diff("src/cache.rs"),
        ]);

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.wisdom_written, 4);

        let notes = mock.written_notes();
        let annotation: v3::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.wisdom.len(), 4);
        assert_eq!(annotation.wisdom[0].category, v3::WisdomCategory::DeadEnd);
        assert_eq!(annotation.wisdom[1].category, v3::WisdomCategory::Gotcha);
        assert_eq!(annotation.wisdom[2].category, v3::WisdomCategory::Insight);
        assert_eq!(
            annotation.wisdom[3].category,
            v3::WisdomCategory::UnfinishedThread
        );
    }

    #[test]
    fn test_wisdom_default_empty() {
        let json = r#"{"commit": "HEAD", "summary": "No wisdom provided here at all"}"#;
        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert!(input.wisdom.is_empty());
    }

    #[test]
    fn test_staged_notes_in_provenance() {
        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Test that staged notes appear in provenance".to_string(),
            wisdom: vec![],
            staged_notes: Some("staged: some context".to_string()),
        };

        let result = handle_annotate_v3(&mock, input).unwrap();
        assert!(result.success);

        let notes = mock.written_notes();
        let annotation: v3::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(
            annotation.provenance.notes.as_deref(),
            Some("staged: some context")
        );
    }
}
