use schemars::JsonSchema;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use snafu::ResultExt;

use crate::error::{chronicle_error, Result};
use crate::git::GitOps;
use crate::schema::common::{AstAnchor, LineRange};
use crate::schema::v2;

// ---------------------------------------------------------------------------
// Input types (v2 live path)
// ---------------------------------------------------------------------------

/// Input for the v2 live annotation path.
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
///   "motivation": "...",
///   "rejected_alternatives": [...],
///   "decisions": [...],
///   "markers": [...],
///   "effort": { "id": "...", "description": "...", "phase": "in_progress" }
/// }
/// ```
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LiveInput {
    pub commit: String,

    /// What this commit does and why (becomes narrative.summary).
    pub summary: String,

    /// What triggered this change? (narrative.motivation)
    pub motivation: Option<String>,

    /// What alternatives were considered and rejected.
    /// Accepts either strings ("Tried X but Y") or objects ({"approach": "...", "reason": "..."}).
    #[serde(default, deserialize_with = "deserialize_flexible_alternatives")]
    #[schemars(with = "Vec<RejectedAlternativeInput>")]
    pub rejected_alternatives: Vec<RejectedAlternativeInput>,

    /// Expected follow-up work (narrative.follow_up).
    pub follow_up: Option<String>,

    /// Design decisions.
    #[serde(default)]
    pub decisions: Vec<DecisionInput>,

    /// Code-level markers.
    #[serde(default)]
    pub markers: Vec<MarkerInput>,

    /// Link to broader effort.
    pub effort: Option<EffortInput>,

    /// Agent sentiments: worries, hunches, confidence, unease.
    #[serde(default)]
    pub sentiments: Vec<SentimentInput>,

    /// Pre-loaded staged notes text (appended to provenance.notes).
    /// Not part of the user-facing JSON schema; populated by the CLI layer.
    #[serde(skip)]
    pub staged_notes: Option<String>,
}

/// An agent sentiment — feeling + detail.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SentimentInput {
    pub feeling: String,
    pub detail: String,
}

/// A rejected alternative — accepts either a string or a struct.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RejectedAlternativeInput {
    pub approach: String,
    #[serde(default)]
    pub reason: String,
}

/// A design decision from the caller.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DecisionInput {
    pub what: String,
    pub why: String,
    #[serde(default = "default_stability")]
    pub stability: v2::Stability,
    pub revisit_when: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
}

fn default_stability() -> v2::Stability {
    v2::Stability::Provisional
}

/// A code-level marker from the caller.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MarkerInput {
    #[serde(alias = "path")]
    pub file: String,
    pub anchor: Option<AnchorInput>,
    pub lines: Option<LineRange>,
    pub kind: MarkerKindInput,
}

/// Simplified anchor for marker input.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AnchorInput {
    pub unit_type: String,
    pub name: String,
}

/// Marker kind from the caller — flexible string-based tags.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MarkerKindInput {
    Contract {
        description: String,
    },
    Hazard {
        description: String,
    },
    Dependency {
        target_file: String,
        target_anchor: String,
        assumption: String,
    },
    Unstable {
        description: String,
        revisit_when: String,
    },
    Security {
        description: String,
    },
    Performance {
        description: String,
    },
    Deprecated {
        description: String,
        replacement: Option<String>,
    },
    TechDebt {
        description: String,
    },
    TestCoverage {
        description: String,
    },
}

/// Effort link from the caller.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EffortInput {
    pub id: String,
    pub description: String,
    #[serde(default = "default_effort_phase")]
    pub phase: v2::EffortPhase,
}

fn default_effort_phase() -> v2::EffortPhase {
    v2::EffortPhase::InProgress
}

// ---------------------------------------------------------------------------
// Flexible deserialization helpers
// ---------------------------------------------------------------------------

/// Accepts rejected_alternatives as either:
/// - strings: "Tried X but Y" -> { approach: "Tried X but Y", reason: "" }
/// - structs: { approach: "...", reason: "..." }
fn deserialize_flexible_alternatives<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<RejectedAlternativeInput>, D::Error>
where
    D: Deserializer<'de>,
{
    struct FlexibleAlternativesVisitor;

    impl<'de> Visitor<'de> for FlexibleAlternativesVisitor {
        type Value = Vec<RejectedAlternativeInput>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a list of strings or {\"approach\": \"...\", \"reason\": \"...\"}")
        }

        fn visit_seq<S>(
            self,
            mut seq: S,
        ) -> std::result::Result<Vec<RejectedAlternativeInput>, S::Error>
        where
            S: SeqAccess<'de>,
        {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element::<FlexibleAlternative>()? {
                items.push(item.into());
            }
            Ok(items)
        }
    }

    deserializer.deserialize_seq(FlexibleAlternativesVisitor)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum FlexibleAlternative {
    Struct {
        approach: String,
        #[serde(default)]
        reason: String,
    },
    Plain(String),
}

impl From<FlexibleAlternative> for RejectedAlternativeInput {
    fn from(fa: FlexibleAlternative) -> Self {
        match fa {
            FlexibleAlternative::Struct { approach, reason } => {
                RejectedAlternativeInput { approach, reason }
            }
            FlexibleAlternative::Plain(text) => RejectedAlternativeInput {
                approach: text,
                reason: String::new(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result returned after writing a v2 annotation.
#[derive(Debug, Clone, Serialize)]
pub struct LiveResult {
    pub success: bool,
    pub commit: String,
    pub markers_written: usize,
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

    if files_changed.len() > 3 && input.motivation.is_none() {
        warnings.push("Multi-file change without motivation — consider adding why".to_string());
    }

    if input.summary.trim() == commit_message.trim() {
        warnings.push(
            "Summary matches commit message verbatim — consider adding why this approach was chosen"
                .to_string(),
        );
    }

    if files_changed.len() > 5 && input.decisions.is_empty() {
        warnings.push(
            "Large change without decisions — consider documenting key design choices".to_string(),
        );
    }

    warnings
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Core v2 handler: validates input, auto-populates files_changed from diff,
/// builds and writes a v2 annotation.
pub fn handle_annotate_v2(git_ops: &dyn GitOps, input: LiveInput) -> Result<LiveResult> {
    // 1. Resolve commit ref to full SHA
    let full_sha = git_ops
        .resolve_ref(&input.commit)
        .context(chronicle_error::GitSnafu)?;

    // 2. Check for existing note (warn before overwriting)
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

    // 3. Auto-populate files_changed from diff
    let files_changed = {
        let diffs = git_ops.diff(&full_sha).context(chronicle_error::GitSnafu)?;
        diffs.into_iter().map(|d| d.path).collect::<Vec<_>>()
    };

    // 4. Quality warnings (non-blocking)
    let commit_message = git_ops
        .commit_info(&full_sha)
        .context(chronicle_error::GitSnafu)?
        .message;
    warnings.extend(check_quality(&input, &files_changed, &commit_message));

    // 5. Build markers
    let mut markers = Vec::new();
    for marker_input in &input.markers {
        markers.push(build_marker(marker_input));
    }

    // 6. Build decisions
    let decisions: Vec<v2::Decision> = input
        .decisions
        .iter()
        .map(|d| v2::Decision {
            what: d.what.clone(),
            why: d.why.clone(),
            stability: d.stability.clone(),
            revisit_when: d.revisit_when.clone(),
            scope: d.scope.clone(),
        })
        .collect();

    // 7. Build effort link
    let effort = input.effort.as_ref().map(|e| v2::EffortLink {
        id: e.id.clone(),
        description: e.description.clone(),
        phase: e.phase.clone(),
    });

    // 8. Build rejected alternatives
    let rejected_alternatives: Vec<v2::RejectedAlternative> = input
        .rejected_alternatives
        .iter()
        .map(|ra| v2::RejectedAlternative {
            approach: ra.approach.clone(),
            reason: ra.reason.clone(),
        })
        .collect();

    // 8b. Build sentiments
    let sentiments: Vec<v2::Sentiment> = input
        .sentiments
        .iter()
        .map(|s| v2::Sentiment {
            feeling: s.feeling.clone(),
            detail: s.detail.clone(),
        })
        .collect();

    // 9. Build annotation
    let annotation = v2::Annotation {
        schema: "chronicle/v2".to_string(),
        commit: full_sha.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        narrative: v2::Narrative {
            summary: input.summary.clone(),
            motivation: input.motivation.clone(),
            rejected_alternatives,
            follow_up: input.follow_up.clone(),
            files_changed,
            sentiments,
        },
        decisions,
        markers,
        effort,
        provenance: v2::Provenance {
            source: v2::ProvenanceSource::Live,
            author: git_ops
                .config_get("chronicle.author")
                .ok()
                .flatten()
                .or_else(|| git_ops.config_get("user.name").ok().flatten()),
            derived_from: Vec::new(),
            notes: input.staged_notes.clone(),
        },
    };

    // 10. Validate (reject on structural errors)
    annotation
        .validate()
        .map_err(|msg| crate::error::ChronicleError::Validation {
            message: msg,
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    // 11. Serialize and write git note
    let json = serde_json::to_string_pretty(&annotation).context(chronicle_error::JsonSnafu)?;
    git_ops
        .note_write(&full_sha, &json)
        .context(chronicle_error::GitSnafu)?;

    let markers_written = annotation.markers.len();

    Ok(LiveResult {
        success: true,
        commit: full_sha,
        markers_written,
        warnings,
    })
}

/// Build a `CodeMarker` from input, passing through anchor as-is.
fn build_marker(input: &MarkerInput) -> v2::CodeMarker {
    let anchor = input.anchor.as_ref().map(|a| AstAnchor {
        unit_type: a.unit_type.clone(),
        name: a.name.clone(),
        signature: None,
    });

    v2::CodeMarker {
        file: input.file.clone(),
        anchor,
        lines: input.lines,
        kind: convert_marker_kind(&input.kind),
    }
}

fn convert_marker_kind(input: &MarkerKindInput) -> v2::MarkerKind {
    match input {
        MarkerKindInput::Contract { description } => v2::MarkerKind::Contract {
            description: description.clone(),
            source: v2::ContractSource::Author,
        },
        MarkerKindInput::Hazard { description } => v2::MarkerKind::Hazard {
            description: description.clone(),
        },
        MarkerKindInput::Dependency {
            target_file,
            target_anchor,
            assumption,
        } => v2::MarkerKind::Dependency {
            target_file: target_file.clone(),
            target_anchor: target_anchor.clone(),
            assumption: assumption.clone(),
        },
        MarkerKindInput::Unstable {
            description,
            revisit_when,
        } => v2::MarkerKind::Unstable {
            description: description.clone(),
            revisit_when: revisit_when.clone(),
        },
        MarkerKindInput::Security { description } => v2::MarkerKind::Security {
            description: description.clone(),
        },
        MarkerKindInput::Performance { description } => v2::MarkerKind::Performance {
            description: description.clone(),
        },
        MarkerKindInput::Deprecated {
            description,
            replacement,
        } => v2::MarkerKind::Deprecated {
            description: description.clone(),
            replacement: replacement.clone(),
        },
        MarkerKindInput::TechDebt { description } => v2::MarkerKind::TechDebt {
            description: description.clone(),
        },
        MarkerKindInput::TestCoverage { description } => v2::MarkerKind::TestCoverage {
            description: description.clone(),
        },
    }
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
        assert!(input.markers.is_empty());
        assert!(input.decisions.is_empty());
        assert!(input.effort.is_none());
        assert!(input.rejected_alternatives.is_empty());
    }

    #[test]
    fn test_rich_input() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Redesign annotation schema",
            "motivation": "Current annotations restate diffs",
            "rejected_alternatives": [
                {"approach": "Enrich v1 with optional fields", "reason": "Too noisy"},
                "Tried migrating all notes in bulk"
            ],
            "decisions": [
                {"what": "Lazy migration", "why": "Avoids risky bulk rewrite", "stability": "permanent"}
            ],
            "markers": [
                {"file": "src/schema/v2.rs", "anchor": {"unit_type": "function", "name": "validate"}, "kind": {"type": "contract", "description": "Must be called before writing"}}
            ],
            "effort": {"id": "schema-v2", "description": "Schema v2 redesign", "phase": "start"}
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.rejected_alternatives.len(), 2);
        assert_eq!(
            input.rejected_alternatives[0].approach,
            "Enrich v1 with optional fields"
        );
        assert_eq!(
            input.rejected_alternatives[1].approach,
            "Tried migrating all notes in bulk"
        );
        assert_eq!(input.decisions.len(), 1);
        assert_eq!(input.decisions[0].stability, v2::Stability::Permanent);
        assert_eq!(input.markers.len(), 1);
        assert!(input.effort.is_some());
        assert_eq!(input.effort.as_ref().unwrap().phase, v2::EffortPhase::Start);
    }

    #[test]
    fn test_handle_annotate_v2_minimal() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.commit, "abc123def456");
        assert_eq!(result.markers_written, 0);

        let notes = mock.written_notes();
        assert_eq!(notes.len(), 1);
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.schema, "chronicle/v2");
        assert_eq!(
            annotation.narrative.summary,
            "Add hello_world function and Config struct"
        );
        assert_eq!(annotation.narrative.files_changed, vec!["src/lib.rs"]);
        assert_eq!(annotation.provenance.source, v2::ProvenanceSource::Live);
    }

    #[test]
    fn test_handle_annotate_v2_with_markers() {
        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("src/lib.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![MarkerInput {
                file: "src/lib.rs".to_string(),
                anchor: Some(AnchorInput {
                    unit_type: "function".to_string(),
                    name: "hello_world".to_string(),
                }),
                lines: Some(LineRange { start: 2, end: 4 }),
                kind: MarkerKindInput::Contract {
                    description: "Must print to stdout".to_string(),
                },
            }],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.markers_written, 1);

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.markers.len(), 1);
        assert!(annotation.markers[0].anchor.is_some());
    }

    #[test]
    fn test_files_changed_auto_populated() {
        let mock = MockGitOps::new("abc123")
            .with_diffs(vec![test_diff("src/lib.rs"), test_diff("src/main.rs")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Multi-file change for testing auto-population".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(
            annotation.narrative.files_changed,
            vec!["src/lib.rs", "src/main.rs"]
        );
    }

    #[test]
    fn test_validation_rejects_empty_summary() {
        let mock = MockGitOps::new("abc123");

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input);
        assert!(result.is_err());
    }

    #[test]
    fn test_effort_defaults_to_in_progress() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Test effort defaults",
            "effort": {"id": "test-1", "description": "Test effort"}
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.effort.as_ref().unwrap().phase,
            v2::EffortPhase::InProgress
        );
    }

    #[test]
    fn test_decision_defaults_to_provisional() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Test decision defaults",
            "decisions": [{"what": "Use X", "why": "Because Y"}]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.decisions[0].stability, v2::Stability::Provisional);
    }

    #[test]
    fn test_overwrite_existing_note_warns() {
        let mock = MockGitOps::new("abc123de")
            .with_diffs(vec![test_diff("src/lib.rs")])
            .with_note_exists(true);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
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
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("Overwriting")),
            "Should not have overwrite warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_quality_multi_file_without_motivation() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![
            test_diff("src/a.rs"),
            test_diff("src/b.rs"),
            test_diff("src/c.rs"),
            test_diff("src/d.rs"),
        ]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Refactor multiple modules for consistency".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Multi-file change without motivation")),
            "Expected multi-file motivation warning, got: {:?}",
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
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
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
    fn test_quality_large_change_without_decisions() {
        let mock = MockGitOps::new("abc123def456").with_diffs(vec![
            test_diff("src/a.rs"),
            test_diff("src/b.rs"),
            test_diff("src/c.rs"),
            test_diff("src/d.rs"),
            test_diff("src/e.rs"),
            test_diff("src/f.rs"),
        ]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Large refactor across many modules for improved architecture".to_string(),
            motivation: Some("Needed for the next feature".to_string()),
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Large change without decisions")),
            "Expected large-change decisions warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_marker_without_anchor() {
        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("config.toml")]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Update config with new settings for testing".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![MarkerInput {
                file: "config.toml".to_string(),
                anchor: None,
                lines: None,
                kind: MarkerKindInput::Hazard {
                    description: "Config format is not validated at startup".to_string(),
                },
            }],
            effort: None,
            sentiments: vec![],
            staged_notes: None,
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.markers_written, 1);
    }

    #[test]
    fn test_new_marker_kinds_roundtrip() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Test all new marker kinds for round-trip serialization",
            "markers": [
                {"file": "src/auth.rs", "kind": {"type": "security", "description": "Validates JWT tokens"}},
                {"file": "src/hot.rs", "kind": {"type": "performance", "description": "Hot loop, avoid allocations"}},
                {"file": "src/old.rs", "kind": {"type": "deprecated", "description": "Use new_api instead", "replacement": "src/new_api.rs"}},
                {"file": "src/hack.rs", "kind": {"type": "tech_debt", "description": "Needs refactor after v2 ships"}},
                {"file": "src/lib.rs", "kind": {"type": "test_coverage", "description": "Missing edge case tests for empty input"}}
            ]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.markers.len(), 5);

        let mock = MockGitOps::new("abc123").with_diffs(vec![
            test_diff("src/auth.rs"),
            test_diff("src/hot.rs"),
            test_diff("src/old.rs"),
            test_diff("src/hack.rs"),
            test_diff("src/lib.rs"),
        ]);

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.markers_written, 5);

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.markers.len(), 5);

        assert!(matches!(
            &annotation.markers[0].kind,
            v2::MarkerKind::Security { description } if description == "Validates JWT tokens"
        ));
        assert!(matches!(
            &annotation.markers[1].kind,
            v2::MarkerKind::Performance { description } if description == "Hot loop, avoid allocations"
        ));
        assert!(matches!(
            &annotation.markers[2].kind,
            v2::MarkerKind::Deprecated { description, replacement }
                if description == "Use new_api instead" && replacement.as_deref() == Some("src/new_api.rs")
        ));
        assert!(matches!(
            &annotation.markers[3].kind,
            v2::MarkerKind::TechDebt { description } if description == "Needs refactor after v2 ships"
        ));
        assert!(matches!(
            &annotation.markers[4].kind,
            v2::MarkerKind::TestCoverage { description } if description == "Missing edge case tests for empty input"
        ));
    }

    #[test]
    fn test_deprecated_marker_without_replacement() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Test deprecated marker without replacement field",
            "markers": [
                {"file": "src/old.rs", "kind": {"type": "deprecated", "description": "Will be removed in v3"}}
            ]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("src/old.rs")]);

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert!(matches!(
            &annotation.markers[0].kind,
            v2::MarkerKind::Deprecated { replacement, .. } if replacement.is_none()
        ));
    }

    #[test]
    fn test_sentiments_roundtrip() {
        let json = r#"{
            "commit": "HEAD",
            "summary": "Refactor connection pool with sentiment tracking test",
            "sentiments": [
                {"feeling": "worry", "detail": "The pool size heuristic is fragile under high concurrency"},
                {"feeling": "confidence", "detail": "The drain logic is well-tested and straightforward"}
            ]
        }"#;

        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.sentiments.len(), 2);
        assert_eq!(input.sentiments[0].feeling, "worry");

        let mock = MockGitOps::new("abc123").with_diffs(vec![test_diff("src/pool.rs")]);

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.narrative.sentiments.len(), 2);
        assert_eq!(annotation.narrative.sentiments[0].feeling, "worry");
        assert_eq!(
            annotation.narrative.sentiments[0].detail,
            "The pool size heuristic is fragile under high concurrency"
        );
        assert_eq!(annotation.narrative.sentiments[1].feeling, "confidence");
    }

    #[test]
    fn test_sentiments_default_empty() {
        let json = r#"{"commit": "HEAD", "summary": "No sentiments provided here at all"}"#;
        let input: LiveInput = serde_json::from_str(json).unwrap();
        assert!(input.sentiments.is_empty());
    }
}
