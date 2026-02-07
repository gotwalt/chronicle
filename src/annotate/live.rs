use std::path::Path;

use schemars::JsonSchema;
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use snafu::ResultExt;

use crate::ast::{self, AnchorMatch, Language};
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
            formatter
                .write_str("a list of strings or {\"approach\": \"...\", \"reason\": \"...\"}")
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
    pub anchor_resolutions: Vec<AnchorResolution>,
}

/// How an anchor was resolved during annotation.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorResolution {
    pub file: String,
    pub requested_name: String,
    pub resolution: AnchorResolutionKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AnchorResolutionKind {
    Exact,
    Qualified { resolved_name: String },
    Fuzzy { resolved_name: String, distance: u32 },
    Unresolved,
}

// ---------------------------------------------------------------------------
// Quality checks (non-blocking warnings)
// ---------------------------------------------------------------------------

fn check_quality(input: &LiveInput) -> Vec<String> {
    let mut warnings = Vec::new();

    if input.summary.len() < 20 {
        warnings.push("Summary is very short — consider adding more detail".to_string());
    }

    warnings
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Core v2 handler: validates input, resolves marker anchors via AST,
/// auto-populates files_changed from diff, builds and writes a v2 annotation.
pub fn handle_annotate_v2(git_ops: &dyn GitOps, input: LiveInput) -> Result<LiveResult> {
    // 1. Resolve commit ref to full SHA
    let full_sha = git_ops
        .resolve_ref(&input.commit)
        .context(chronicle_error::GitSnafu)?;

    // 2. Quality warnings (non-blocking)
    let warnings = check_quality(&input);

    // 3. Auto-populate files_changed from diff
    let files_changed = {
        let diffs = git_ops.diff(&full_sha).context(chronicle_error::GitSnafu)?;
        diffs.into_iter().map(|d| d.path).collect::<Vec<_>>()
    };

    // 4. Resolve marker anchors and build markers
    let mut markers = Vec::new();
    let mut anchor_resolutions = Vec::new();

    for marker_input in &input.markers {
        let (marker, resolution) = resolve_and_build_marker(git_ops, &full_sha, marker_input)?;
        markers.push(marker);
        if let Some(res) = resolution {
            anchor_resolutions.push(res);
        }
    }

    // 5. Build decisions
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

    // 6. Build effort link
    let effort = input.effort.as_ref().map(|e| v2::EffortLink {
        id: e.id.clone(),
        description: e.description.clone(),
        phase: e.phase.clone(),
    });

    // 7. Build rejected alternatives
    let rejected_alternatives: Vec<v2::RejectedAlternative> = input
        .rejected_alternatives
        .iter()
        .map(|ra| v2::RejectedAlternative {
            approach: ra.approach.clone(),
            reason: ra.reason.clone(),
        })
        .collect();

    // 8. Build annotation
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
        },
        decisions,
        markers,
        effort,
        provenance: v2::Provenance {
            source: v2::ProvenanceSource::Live,
            derived_from: Vec::new(),
            notes: None,
        },
    };

    // 9. Validate (reject on structural errors)
    annotation
        .validate()
        .map_err(|msg| crate::error::ChronicleError::Validation {
            message: msg,
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    // 10. Serialize and write git note
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
        anchor_resolutions,
    })
}

/// Resolve a marker's anchor against the AST outline and build the final `CodeMarker`.
fn resolve_and_build_marker(
    git_ops: &dyn GitOps,
    commit: &str,
    input: &MarkerInput,
) -> Result<(v2::CodeMarker, Option<AnchorResolution>)> {
    let anchor_input = match &input.anchor {
        Some(a) => a,
        None => {
            // No anchor — file-level marker, no resolution needed
            let marker = v2::CodeMarker {
                file: input.file.clone(),
                anchor: None,
                lines: input.lines,
                kind: convert_marker_kind(&input.kind),
            };
            return Ok((marker, None));
        }
    };

    let file_path = Path::new(&input.file);
    let lang = Language::from_path(&input.file);

    let (ast_anchor, lines, resolution_kind) = match lang {
        Language::Unsupported => (
            AstAnchor {
                unit_type: anchor_input.unit_type.clone(),
                name: anchor_input.name.clone(),
                signature: None,
            },
            input.lines,
            AnchorResolutionKind::Unresolved,
        ),
        _ => match git_ops.file_at_commit(file_path, commit) {
            Ok(source) => match ast::extract_outline(&source, lang) {
                Ok(outline) => {
                    match ast::resolve_anchor(&outline, &anchor_input.unit_type, &anchor_input.name)
                    {
                        Some(anchor_match) => {
                            let entry = anchor_match.entry();
                            let corrected_lines = anchor_match.lines();
                            let res_kind = match &anchor_match {
                                AnchorMatch::Exact(_) => AnchorResolutionKind::Exact,
                                AnchorMatch::Qualified(e) => AnchorResolutionKind::Qualified {
                                    resolved_name: e.name.clone(),
                                },
                                AnchorMatch::Fuzzy(e, d) => AnchorResolutionKind::Fuzzy {
                                    resolved_name: e.name.clone(),
                                    distance: *d,
                                },
                            };
                            (
                                AstAnchor {
                                    unit_type: entry.kind.as_str().to_string(),
                                    name: entry.name.clone(),
                                    signature: entry.signature.clone(),
                                },
                                Some(corrected_lines),
                                res_kind,
                            )
                        }
                        None => (
                            AstAnchor {
                                unit_type: anchor_input.unit_type.clone(),
                                name: anchor_input.name.clone(),
                                signature: None,
                            },
                            input.lines,
                            AnchorResolutionKind::Unresolved,
                        ),
                    }
                }
                Err(_) => (
                    AstAnchor {
                        unit_type: anchor_input.unit_type.clone(),
                        name: anchor_input.name.clone(),
                        signature: None,
                    },
                    input.lines,
                    AnchorResolutionKind::Unresolved,
                ),
            },
            Err(_) => (
                AstAnchor {
                    unit_type: anchor_input.unit_type.clone(),
                    name: anchor_input.name.clone(),
                    signature: None,
                },
                input.lines,
                AnchorResolutionKind::Unresolved,
            ),
        },
    };

    let marker = v2::CodeMarker {
        file: input.file.clone(),
        anchor: Some(ast_anchor),
        lines,
        kind: convert_marker_kind(&input.kind),
    };

    let resolution = AnchorResolution {
        file: input.file.clone(),
        requested_name: anchor_input.name.clone(),
        resolution: resolution_kind,
    };

    Ok((marker, Some(resolution)))
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
    }

    impl MockGitOps {
        fn new(sha: &str) -> Self {
            Self {
                resolved_sha: sha.to_string(),
                files: HashMap::new(),
                diffs: Vec::new(),
                written_notes: Mutex::new(Vec::new()),
            }
        }

        fn with_file(mut self, path: &str, content: &str) -> Self {
            self.files.insert(path.to_string(), content.to_string());
            self
        }

        fn with_diffs(mut self, diffs: Vec<FileDiff>) -> Self {
            self.diffs = diffs;
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
            Ok(false)
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
                message: "test commit".to_string(),
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

    fn sample_rust_source() -> &'static str {
        r#"
pub fn hello_world() {
    println!("Hello, world!");
}

pub struct Config {
    pub name: String,
}

impl Config {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}
"#
    }

    #[test]
    fn test_minimal_input() {
        let json = r#"{"commit": "HEAD", "summary": "Switch to exponential backoff for MQTT reconnect"}"#;
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
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.commit, "abc123def456");
        assert_eq!(result.markers_written, 0);

        let notes = mock.written_notes();
        assert_eq!(notes.len(), 1);
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.schema, "chronicle/v2");
        assert_eq!(annotation.narrative.summary, "Add hello_world function and Config struct");
        assert_eq!(annotation.narrative.files_changed, vec!["src/lib.rs"]);
        assert_eq!(annotation.provenance.source, v2::ProvenanceSource::Live);
    }

    #[test]
    fn test_handle_annotate_v2_with_markers() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/lib.rs", sample_rust_source())
            .with_diffs(vec![test_diff("src/lib.rs")]);

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
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.markers_written, 1);

        // Should have resolved anchor
        assert!(!result.anchor_resolutions.is_empty());
        assert!(matches!(
            result.anchor_resolutions[0].resolution,
            AnchorResolutionKind::Exact
        ));

        let notes = mock.written_notes();
        let annotation: v2::Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.markers.len(), 1);
        assert!(annotation.markers[0].anchor.is_some());
        assert!(annotation.markers[0].anchor.as_ref().unwrap().signature.is_some());
    }

    #[test]
    fn test_files_changed_auto_populated() {
        let mock = MockGitOps::new("abc123").with_diffs(vec![
            test_diff("src/lib.rs"),
            test_diff("src/main.rs"),
        ]);

        let input = LiveInput {
            commit: "HEAD".to_string(),
            summary: "Multi-file change for testing auto-population".to_string(),
            motivation: None,
            rejected_alternatives: vec![],
            follow_up: None,
            decisions: vec![],
            markers: vec![],
            effort: None,
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
        };

        let result = handle_annotate_v2(&mock, input).unwrap();
        assert!(result.success);
        assert_eq!(result.markers_written, 1);
        assert!(result.anchor_resolutions.is_empty());
    }
}
