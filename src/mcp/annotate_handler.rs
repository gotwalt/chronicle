use std::path::Path;

use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::ast::{self, AnchorMatch, Language};
use crate::error::{chronicle_error, Result};
use crate::git::GitOps;
use crate::schema::{
    Annotation, AstAnchor, Constraint, ConstraintSource, ContextLevel, CrossCuttingConcern,
    LineRange, Provenance, ProvenanceOperation, RegionAnnotation, SemanticDependency,
};

// ---------------------------------------------------------------------------
// Input types (from the calling agent)
// ---------------------------------------------------------------------------

/// Input provided by the calling agent when annotating a commit.
#[derive(Debug, Clone, Deserialize)]
pub struct AnnotateInput {
    pub commit: String,
    pub summary: String,
    pub task: Option<String>,
    pub regions: Vec<RegionInput>,
    pub cross_cutting: Vec<CrossCuttingConcern>,
}

/// A single region the agent wants to annotate.
#[derive(Debug, Clone, Deserialize)]
pub struct RegionInput {
    pub file: String,
    pub anchor: AnchorInput,
    pub lines: LineRange,
    pub intent: String,
    pub reasoning: Option<String>,
    pub constraints: Vec<ConstraintInput>,
    pub semantic_dependencies: Vec<SemanticDependency>,
    pub tags: Vec<String>,
    pub risk_notes: Option<String>,
}

/// Simplified anchor — the agent provides unit_type and name;
/// the handler resolves the full signature and corrected lines via AST.
#[derive(Debug, Clone, Deserialize)]
pub struct AnchorInput {
    pub unit_type: String,
    pub name: String,
}

/// A constraint supplied by the author (source is always `Author`).
#[derive(Debug, Clone, Deserialize)]
pub struct ConstraintInput {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Result returned after writing the annotation.
#[derive(Debug, Clone, Serialize)]
pub struct AnnotateResult {
    pub success: bool,
    pub commit: String,
    pub regions_written: usize,
    pub warnings: Vec<String>,
    pub anchor_resolutions: Vec<AnchorResolution>,
}

/// How an anchor was resolved (or not) during annotation.
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

fn check_quality(input: &AnnotateInput) -> Vec<String> {
    let mut warnings = Vec::new();

    if input.summary.len() < 20 {
        warnings.push("Summary is very short — consider adding more detail".to_string());
    }

    for (i, region) in input.regions.iter().enumerate() {
        if region.intent.len() < 10 {
            warnings.push(format!(
                "region[{}] ({}/{}): intent is very short",
                i, region.file, region.anchor.name
            ));
        }
        if region.reasoning.is_none() {
            warnings.push(format!(
                "region[{}] ({}/{}): no reasoning provided",
                i, region.file, region.anchor.name
            ));
        }
        if region.constraints.is_empty() {
            warnings.push(format!(
                "region[{}] ({}/{}): no constraints provided",
                i, region.file, region.anchor.name
            ));
        }
    }

    warnings
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Core handler: validates input, resolves anchors via AST, builds and writes
/// the annotation as a git note.
///
/// This is the "live path" — called by the agent directly after committing,
/// with zero LLM cost.
pub fn handle_annotate(git_ops: &dyn GitOps, input: AnnotateInput) -> Result<AnnotateResult> {
    // 1. Resolve commit ref to full SHA
    let full_sha = git_ops
        .resolve_ref(&input.commit)
        .context(chronicle_error::GitSnafu)?;

    // 2. Quality warnings (non-blocking)
    let warnings = check_quality(&input);

    // 3. Resolve anchors and build regions
    let mut regions = Vec::new();
    let mut anchor_resolutions = Vec::new();

    for region_input in &input.regions {
        let (region, resolution) =
            resolve_and_build_region(git_ops, &full_sha, region_input)?;
        regions.push(region);
        anchor_resolutions.push(resolution);
    }

    // 4. Build annotation
    let annotation = Annotation {
        schema: "chronicle/v1".to_string(),
        commit: full_sha.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        task: input.task.clone(),
        summary: input.summary.clone(),
        context_level: ContextLevel::Enhanced,
        regions,
        cross_cutting: input.cross_cutting.clone(),
        provenance: Provenance {
            operation: ProvenanceOperation::Initial,
            derived_from: Vec::new(),
            original_annotations_preserved: false,
            synthesis_notes: None,
        },
    };

    // 5. Validate (reject on structural errors)
    annotation.validate().map_err(|msg| {
        crate::error::ChronicleError::Validation {
            message: msg,
            location: snafu::Location::new(file!(), line!(), 0),
        }
    })?;

    // 6. Serialize and write git note
    let json = serde_json::to_string_pretty(&annotation)
        .context(chronicle_error::JsonSnafu)?;
    git_ops
        .note_write(&full_sha, &json)
        .context(chronicle_error::GitSnafu)?;

    Ok(AnnotateResult {
        success: true,
        commit: full_sha,
        regions_written: annotation.regions.len(),
        warnings,
        anchor_resolutions,
    })
}

/// Resolve a single region's anchor against the AST outline and build the
/// final `RegionAnnotation`.
fn resolve_and_build_region(
    git_ops: &dyn GitOps,
    commit: &str,
    input: &RegionInput,
) -> Result<(RegionAnnotation, AnchorResolution)> {
    let file_path = Path::new(&input.file);
    let lang = Language::from_path(&input.file);

    // Try to load the file and resolve the anchor via AST
    let (ast_anchor, lines, resolution_kind) = match lang {
        Language::Unsupported => {
            // No AST support — use the input as-is
            (
                AstAnchor {
                    unit_type: input.anchor.unit_type.clone(),
                    name: input.anchor.name.clone(),
                    signature: None,
                },
                input.lines,
                AnchorResolutionKind::Unresolved,
            )
        }
        _ => {
            match git_ops.file_at_commit(file_path, commit) {
                Ok(source) => {
                    match ast::extract_outline(&source, lang) {
                        Ok(outline) => {
                            match ast::resolve_anchor(
                                &outline,
                                &input.anchor.unit_type,
                                &input.anchor.name,
                            ) {
                                Some(anchor_match) => {
                                    let entry = anchor_match.entry();
                                    let corrected_lines = anchor_match.lines();
                                    let resolution_kind = match &anchor_match {
                                        AnchorMatch::Exact(_) => AnchorResolutionKind::Exact,
                                        AnchorMatch::Qualified(e) => {
                                            AnchorResolutionKind::Qualified {
                                                resolved_name: e.name.clone(),
                                            }
                                        }
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
                                        corrected_lines,
                                        resolution_kind,
                                    )
                                }
                                None => {
                                    // No match — use input as-is
                                    (
                                        AstAnchor {
                                            unit_type: input.anchor.unit_type.clone(),
                                            name: input.anchor.name.clone(),
                                            signature: None,
                                        },
                                        input.lines,
                                        AnchorResolutionKind::Unresolved,
                                    )
                                }
                            }
                        }
                        Err(_) => {
                            // Outline extraction failed — use input as-is
                            (
                                AstAnchor {
                                    unit_type: input.anchor.unit_type.clone(),
                                    name: input.anchor.name.clone(),
                                    signature: None,
                                },
                                input.lines,
                                AnchorResolutionKind::Unresolved,
                            )
                        }
                    }
                }
                Err(_) => {
                    // File not available at commit — use input as-is
                    (
                        AstAnchor {
                            unit_type: input.anchor.unit_type.clone(),
                            name: input.anchor.name.clone(),
                            signature: None,
                        },
                        input.lines,
                        AnchorResolutionKind::Unresolved,
                    )
                }
            }
        }
    };

    let constraints: Vec<Constraint> = input
        .constraints
        .iter()
        .map(|c| Constraint {
            text: c.text.clone(),
            source: ConstraintSource::Author,
        })
        .collect();

    let region = RegionAnnotation {
        file: input.file.clone(),
        ast_anchor,
        lines,
        intent: input.intent.clone(),
        reasoning: input.reasoning.clone(),
        constraints,
        semantic_dependencies: input.semantic_dependencies.clone(),
        related_annotations: Vec::new(),
        tags: input.tags.clone(),
        risk_notes: input.risk_notes.clone(),
        corrections: Vec::new(),
    };

    let resolution = AnchorResolution {
        file: input.file.clone(),
        requested_name: input.anchor.name.clone(),
        resolution: resolution_kind,
    };

    Ok((region, resolution))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GitError;
    use crate::git::diff::FileDiff;
    use crate::git::CommitInfo;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A mock GitOps for testing the annotate handler.
    struct MockGitOps {
        resolved_sha: String,
        files: HashMap<String, String>,
        written_notes: Mutex<Vec<(String, String)>>,
    }

    impl MockGitOps {
        fn new(sha: &str) -> Self {
            Self {
                resolved_sha: sha.to_string(),
                files: HashMap::new(),
                written_notes: Mutex::new(Vec::new()),
            }
        }

        fn with_file(mut self, path: &str, content: &str) -> Self {
            self.files.insert(path.to_string(), content.to_string());
            self
        }

        fn written_notes(&self) -> Vec<(String, String)> {
            self.written_notes.lock().unwrap().clone()
        }
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> std::result::Result<Vec<FileDiff>, GitError> {
            Ok(Vec::new())
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
                timestamp: "2024-01-01T00:00:00Z".to_string(),
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

        fn list_annotated_commits(&self, _limit: u32) -> std::result::Result<Vec<String>, GitError> {
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

    fn make_basic_input() -> AnnotateInput {
        AnnotateInput {
            commit: "HEAD".to_string(),
            summary: "Add hello_world function and Config struct".to_string(),
            task: Some("TASK-123".to_string()),
            regions: vec![RegionInput {
                file: "src/lib.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "hello_world".to_string(),
                },
                lines: LineRange { start: 2, end: 4 },
                intent: "Add a greeting function for the CLI entrypoint".to_string(),
                reasoning: Some("Needed a simple entry point for testing".to_string()),
                constraints: vec![ConstraintInput {
                    text: "Must print to stdout, not stderr".to_string(),
                }],
                semantic_dependencies: vec![],
                tags: vec!["cli".to_string()],
                risk_notes: None,
            }],
            cross_cutting: vec![],
        }
    }

    #[test]
    fn test_handle_annotate_writes_note() {
        let mock = MockGitOps::new("abc123def456")
            .with_file("src/lib.rs", sample_rust_source());

        let input = make_basic_input();
        let result = handle_annotate(&mock, input).unwrap();

        assert!(result.success);
        assert_eq!(result.commit, "abc123def456");
        assert_eq!(result.regions_written, 1);

        // Verify a note was written
        let notes = mock.written_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, "abc123def456");

        // Verify the note is valid JSON with the expected schema
        let annotation: Annotation = serde_json::from_str(&notes[0].1).unwrap();
        assert_eq!(annotation.schema, "chronicle/v1");
        assert_eq!(annotation.commit, "abc123def456");
        assert_eq!(annotation.context_level, ContextLevel::Enhanced);
        assert_eq!(annotation.task, Some("TASK-123".to_string()));
    }

    #[test]
    fn test_anchor_resolution_exact() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/lib.rs", sample_rust_source());

        let input = make_basic_input();
        let result = handle_annotate(&mock, input).unwrap();

        // Verify the anchor was resolved
        assert!(!result.anchor_resolutions.is_empty());

        // hello_world should resolve exactly
        assert!(matches!(
            result.anchor_resolutions[0].resolution,
            AnchorResolutionKind::Exact
        ));
    }

    #[test]
    fn test_anchor_resolution_corrects_lines() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/lib.rs", sample_rust_source());

        let input = make_basic_input();
        let _result = handle_annotate(&mock, input).unwrap();

        // Verify the note was written
        let notes = mock.written_notes();
        let annotation: Annotation = serde_json::from_str(&notes[0].1).unwrap();

        // The AST should correct the line range to the actual function location
        let region = &annotation.regions[0];
        assert!(region.lines.start > 0);
        assert!(region.lines.end >= region.lines.start);
        // Signature should be filled in by AST
        assert!(region.ast_anchor.signature.is_some());
    }

    #[test]
    fn test_constraints_have_author_source() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/lib.rs", sample_rust_source());

        let input = make_basic_input();
        handle_annotate(&mock, input).unwrap();

        let notes = mock.written_notes();
        let annotation: Annotation = serde_json::from_str(&notes[0].1).unwrap();

        for constraint in &annotation.regions[0].constraints {
            assert_eq!(constraint.source, ConstraintSource::Author);
        }
    }

    #[test]
    fn test_quality_warnings() {
        let input = AnnotateInput {
            commit: "HEAD".to_string(),
            summary: "short".to_string(), // too short
            task: None,
            regions: vec![RegionInput {
                file: "src/lib.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "foo".to_string(),
                },
                lines: LineRange { start: 1, end: 5 },
                intent: "short".to_string(), // too short
                reasoning: None,             // missing
                constraints: vec![],         // missing
                semantic_dependencies: vec![],
                tags: vec![],
                risk_notes: None,
            }],
            cross_cutting: vec![],
        };

        let warnings = check_quality(&input);
        assert!(warnings.iter().any(|w| w.contains("Summary is very short")));
        assert!(warnings.iter().any(|w| w.contains("intent is very short")));
        assert!(warnings.iter().any(|w| w.contains("no reasoning")));
        assert!(warnings.iter().any(|w| w.contains("no constraints")));
    }

    #[test]
    fn test_validation_rejects_empty_summary() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/lib.rs", sample_rust_source());

        let input = AnnotateInput {
            commit: "HEAD".to_string(),
            summary: "".to_string(),
            task: None,
            regions: vec![],
            cross_cutting: vec![],
        };

        let result = handle_annotate(&mock, input);
        assert!(result.is_err());
    }

    #[test]
    fn test_unsupported_language_uses_input_as_is() {
        let mock = MockGitOps::new("abc123")
            .with_file("src/data.toml", "[section]\nkey = \"value\"\n");

        let input = AnnotateInput {
            commit: "HEAD".to_string(),
            summary: "Add TOML config data".to_string(),
            task: None,
            regions: vec![RegionInput {
                file: "src/data.toml".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "section".to_string(),
                },
                lines: LineRange { start: 1, end: 2 },
                intent: "Add a config section".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                tags: vec![],
                risk_notes: None,
            }],
            cross_cutting: vec![],
        };

        let result = handle_annotate(&mock, input).unwrap();
        assert!(result.success);
        assert!(matches!(
            result.anchor_resolutions[0].resolution,
            AnchorResolutionKind::Unresolved
        ));
    }

    #[test]
    fn test_file_not_at_commit_uses_input_as_is() {
        // No files registered in mock — file_at_commit will fail
        let mock = MockGitOps::new("abc123");

        let input = AnnotateInput {
            commit: "HEAD".to_string(),
            summary: "Update something in a file".to_string(),
            task: None,
            regions: vec![RegionInput {
                file: "src/missing.rs".to_string(),
                anchor: AnchorInput {
                    unit_type: "function".to_string(),
                    name: "missing_fn".to_string(),
                },
                lines: LineRange { start: 1, end: 10 },
                intent: "Modify a function that was deleted".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![],
                tags: vec![],
                risk_notes: None,
            }],
            cross_cutting: vec![],
        };

        let result = handle_annotate(&mock, input).unwrap();
        assert!(result.success);
        assert!(matches!(
            result.anchor_resolutions[0].resolution,
            AnchorResolutionKind::Unresolved
        ));
    }
}
