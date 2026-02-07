use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::chronicle_error::{IoSnafu, JsonSnafu};
use crate::error::Result;
use crate::git::GitOps;
use crate::schema::v1::{
    self, ContextLevel, CrossCuttingConcern, Provenance, ProvenanceOperation,
    RegionAnnotation,
};
type Annotation = v1::Annotation;
use snafu::ResultExt;

/// Expiry time for pending-squash.json files, in seconds.
const PENDING_SQUASH_EXPIRY_SECS: i64 = 60;

/// Written to .git/chronicle/pending-squash.json by prepare-commit-msg.
/// Consumed and deleted by post-commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSquash {
    pub source_commits: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Context for squash synthesis, assembled before calling the agent.
#[derive(Debug, Clone)]
pub struct SquashSynthesisContext {
    /// The squash commit's SHA.
    pub squash_commit: String,
    /// The squash commit's combined diff as text.
    pub diff: String,
    /// Annotations from source commits (those that had annotations).
    pub source_annotations: Vec<Annotation>,
    /// Commit messages from source commits: (sha, message).
    pub source_messages: Vec<(String, String)>,
    /// The squash commit's own commit message.
    pub squash_message: String,
}

/// Context for amend migration, assembled before calling the agent.
#[derive(Debug, Clone)]
pub struct AmendMigrationContext {
    /// The new (post-amend) commit SHA.
    pub new_commit: String,
    /// The new commit's diff (against its parent).
    pub new_diff: String,
    /// The old (pre-amend) annotation.
    pub old_annotation: Annotation,
    /// The new commit message.
    pub new_message: String,
}

fn pending_squash_path(git_dir: &Path) -> std::path::PathBuf {
    git_dir.join("chronicle").join("pending-squash.json")
}

/// Write pending-squash.json to .git/chronicle/.
pub fn write_pending_squash(git_dir: &Path, pending: &PendingSquash) -> Result<()> {
    let path = pending_squash_path(git_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context(IoSnafu)?;
    }
    let json = serde_json::to_string_pretty(pending).context(JsonSnafu)?;
    std::fs::write(&path, json).context(IoSnafu)?;
    Ok(())
}

/// Read pending-squash.json. Returns None if missing, stale, or invalid.
/// Stale or invalid files are deleted with a warning.
pub fn read_pending_squash(git_dir: &Path) -> Result<Option<PendingSquash>> {
    let path = pending_squash_path(git_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).context(IoSnafu)?;
    let pending: PendingSquash = match serde_json::from_str(&content) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Invalid pending-squash.json, deleting: {e}");
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
    };

    let age = Utc::now() - pending.timestamp;
    if age.num_seconds() > PENDING_SQUASH_EXPIRY_SECS {
        tracing::warn!(
            "Stale pending-squash.json ({}s old), deleting",
            age.num_seconds()
        );
        std::fs::remove_file(&path).context(IoSnafu)?;
        return Ok(None);
    }

    Ok(Some(pending))
}

/// Delete the pending-squash.json file.
pub fn delete_pending_squash(git_dir: &Path) -> Result<()> {
    let path = pending_squash_path(git_dir);
    if path.exists() {
        std::fs::remove_file(&path).context(IoSnafu)?;
    }
    Ok(())
}

/// Synthesize an annotation from multiple source annotations (squash merge).
///
/// This merges regions, combines cross-cutting concerns, and sets provenance.
/// For MVP, this does not call the LLM — it performs a deterministic merge.
/// A future version will pass SquashSynthesisContext to the writing agent.
pub fn synthesize_squash_annotation(ctx: &SquashSynthesisContext) -> Annotation {
    let mut all_regions: Vec<RegionAnnotation> = Vec::new();
    let mut all_cross_cutting: Vec<CrossCuttingConcern> = Vec::new();
    let mut source_shas: Vec<String> = Vec::new();
    let has_annotations = !ctx.source_annotations.is_empty();

    for ann in &ctx.source_annotations {
        source_shas.push(ann.commit.clone());

        // Merge regions: collect all, deduplicating by (file, ast_anchor.name)
        for region in &ann.regions {
            let already_exists = all_regions
                .iter()
                .any(|r| r.file == region.file && r.ast_anchor.name == region.ast_anchor.name);
            if already_exists {
                // Find existing and append reasoning
                if let Some(existing) = all_regions
                    .iter_mut()
                    .find(|r| r.file == region.file && r.ast_anchor.name == region.ast_anchor.name)
                {
                    // Merge constraints (never drop)
                    for constraint in &region.constraints {
                        if !existing
                            .constraints
                            .iter()
                            .any(|c| c.text == constraint.text)
                        {
                            existing.constraints.push(constraint.clone());
                        }
                    }
                    // Merge semantic dependencies
                    for dep in &region.semantic_dependencies {
                        if !existing
                            .semantic_dependencies
                            .iter()
                            .any(|d| d.file == dep.file && d.anchor == dep.anchor)
                        {
                            existing.semantic_dependencies.push(dep.clone());
                        }
                    }
                    // Consolidate reasoning
                    if let Some(new_reasoning) = &region.reasoning {
                        if let Some(ref mut existing_reasoning) = existing.reasoning {
                            existing_reasoning.push_str("\n\n");
                            existing_reasoning.push_str(new_reasoning);
                        } else {
                            existing.reasoning = Some(new_reasoning.clone());
                        }
                    }
                    // Update line range to encompass both
                    existing.lines.start = existing.lines.start.min(region.lines.start);
                    existing.lines.end = existing.lines.end.max(region.lines.end);
                }
            } else {
                all_regions.push(region.clone());
            }
        }

        // Merge cross-cutting concerns (deduplicate by description)
        for cc in &ann.cross_cutting {
            if !all_cross_cutting
                .iter()
                .any(|c| c.description == cc.description)
            {
                all_cross_cutting.push(cc.clone());
            }
        }
    }

    // Collect source SHAs from source_messages for any that didn't have annotations
    for (sha, _) in &ctx.source_messages {
        if !source_shas.contains(sha) {
            source_shas.push(sha.clone());
        }
    }

    let annotations_count = ctx.source_annotations.len();
    let total_sources = ctx.source_messages.len();
    let all_had_annotations = annotations_count == total_sources && total_sources > 0;

    let synthesis_notes = if has_annotations {
        Some(format!(
            "Synthesized from {} commits ({} of {} had annotations).",
            total_sources, annotations_count, total_sources,
        ))
    } else {
        Some(format!(
            "Synthesized from {} commits (none had annotations).",
            total_sources,
        ))
    };

    Annotation {
        schema: "chronicle/v1".to_string(),
        commit: ctx.squash_commit.clone(),
        timestamp: Utc::now().to_rfc3339(),
        task: None,
        summary: ctx.squash_message.clone(),
        context_level: if has_annotations {
            ContextLevel::Enhanced
        } else {
            ContextLevel::Inferred
        },
        regions: all_regions,
        cross_cutting: all_cross_cutting,
        provenance: Provenance {
            operation: ProvenanceOperation::Squash,
            derived_from: source_shas,
            original_annotations_preserved: all_had_annotations,
            synthesis_notes,
        },
    }
}

/// Migrate an annotation from a pre-amend commit to a post-amend commit.
///
/// If the diff is empty (message-only amend), copies the annotation unchanged
/// except for updating the commit SHA and provenance.
pub fn migrate_amend_annotation(ctx: &AmendMigrationContext) -> Annotation {
    let mut new_annotation = ctx.old_annotation.clone();
    new_annotation.commit = ctx.new_commit.clone();
    new_annotation.timestamp = Utc::now().to_rfc3339();

    let is_message_only = ctx.new_diff.trim().is_empty();

    new_annotation.provenance = Provenance {
        operation: ProvenanceOperation::Amend,
        derived_from: vec![ctx.old_annotation.commit.clone()],
        original_annotations_preserved: true,
        synthesis_notes: if is_message_only {
            Some("Message-only amend; annotation unchanged.".to_string())
        } else {
            Some("Migrated from amend. Regions preserved from original annotation.".to_string())
        },
    };

    // For message-only amends, update the summary to match new message
    if is_message_only {
        new_annotation.summary = ctx.new_message.clone();
    }

    new_annotation
}

/// Collect annotations from source commits using git notes.
pub fn collect_source_annotations(
    git_ops: &dyn GitOps,
    source_shas: &[String],
) -> Vec<(String, Option<Annotation>)> {
    source_shas
        .iter()
        .map(|sha| {
            let annotation = git_ops
                .note_read(sha)
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str::<Annotation>(&json).ok());
            (sha.clone(), annotation)
        })
        .collect()
}

/// Collect commit messages from source commits.
pub fn collect_source_messages(
    git_ops: &dyn GitOps,
    source_shas: &[String],
) -> Vec<(String, String)> {
    source_shas
        .iter()
        .filter_map(|sha| {
            git_ops
                .commit_info(sha)
                .ok()
                .map(|info| (sha.clone(), info.message))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v1::{
        Constraint, ConstraintSource, CrossCuttingConcern, CrossCuttingRegionRef,
        SemanticDependency,
    };

    fn make_test_annotation(commit: &str, file: &str, anchor: &str) -> Annotation {
        Annotation {
            schema: "chronicle/v1".to_string(),
            commit: commit.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            task: None,
            summary: format!("Commit {commit}"),
            context_level: ContextLevel::Inferred,
            regions: vec![RegionAnnotation {
                file: file.to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "function".to_string(),
                    name: anchor.to_string(),
                    signature: None,
                },
                lines: LineRange { start: 1, end: 10 },
                intent: format!("Modified {anchor}"),
                reasoning: Some(format!("Reasoning for {anchor} in {commit}")),
                constraints: vec![Constraint {
                    text: format!("Constraint from {commit}"),
                    source: ConstraintSource::Inferred,
                }],
                semantic_dependencies: vec![SemanticDependency {
                    file: "other.rs".to_string(),
                    anchor: "helper".to_string(),
                    nature: "calls".to_string(),
                }],
                related_annotations: Vec::new(),
                tags: Vec::new(),
                risk_notes: None,
                corrections: vec![],
            }],
            cross_cutting: vec![CrossCuttingConcern {
                description: format!("Cross-cutting from {commit}"),
                regions: vec![CrossCuttingRegionRef {
                    file: file.to_string(),
                    anchor: anchor.to_string(),
                }],
                tags: Vec::new(),
            }],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: Vec::new(),
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }
    }

    #[test]
    fn test_pending_squash_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::create_dir_all(git_dir.join("chronicle")).unwrap();

        let pending = PendingSquash {
            source_commits: vec!["abc123".to_string(), "def456".to_string()],
            source_ref: Some("feature-branch".to_string()),
            timestamp: Utc::now(),
        };

        write_pending_squash(git_dir, &pending).unwrap();
        let read_back = read_pending_squash(git_dir).unwrap().unwrap();

        assert_eq!(read_back.source_commits, pending.source_commits);
        assert_eq!(read_back.source_ref, pending.source_ref);
    }

    #[test]
    fn test_pending_squash_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = read_pending_squash(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_pending_squash_stale_file() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::create_dir_all(git_dir.join("chronicle")).unwrap();

        let pending = PendingSquash {
            source_commits: vec!["abc123".to_string()],
            source_ref: None,
            timestamp: Utc::now() - chrono::Duration::seconds(120),
        };

        write_pending_squash(git_dir, &pending).unwrap();
        let result = read_pending_squash(git_dir).unwrap();
        assert!(result.is_none());
        // File should have been deleted
        assert!(!pending_squash_path(git_dir).exists());
    }

    #[test]
    fn test_pending_squash_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        let chronicle_dir = git_dir.join("chronicle");
        std::fs::create_dir_all(&chronicle_dir).unwrap();
        std::fs::write(chronicle_dir.join("pending-squash.json"), "not json").unwrap();

        let result = read_pending_squash(git_dir).unwrap();
        assert!(result.is_none());
        // File should have been deleted
        assert!(!pending_squash_path(git_dir).exists());
    }

    #[test]
    fn test_delete_pending_squash() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();

        let pending = PendingSquash {
            source_commits: vec!["abc123".to_string()],
            source_ref: None,
            timestamp: Utc::now(),
        };

        write_pending_squash(git_dir, &pending).unwrap();
        assert!(pending_squash_path(git_dir).exists());

        delete_pending_squash(git_dir).unwrap();
        assert!(!pending_squash_path(git_dir).exists());
    }

    #[test]
    fn test_delete_pending_squash_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        // Should not error when file doesn't exist
        delete_pending_squash(dir.path()).unwrap();
    }

    #[test]
    fn test_synthesize_squash_distinct_regions() {
        let ann1 = make_test_annotation("abc123", "src/foo.rs", "foo_fn");
        let ann2 = make_test_annotation("def456", "src/bar.rs", "bar_fn");
        let ann3 = make_test_annotation("ghi789", "src/baz.rs", "baz_fn");

        let ctx = SquashSynthesisContext {
            squash_commit: "squash001".to_string(),
            diff: "some diff".to_string(),
            source_annotations: vec![ann1, ann2, ann3],
            source_messages: vec![
                ("abc123".to_string(), "Commit abc".to_string()),
                ("def456".to_string(), "Commit def".to_string()),
                ("ghi789".to_string(), "Commit ghi".to_string()),
            ],
            squash_message: "Squash merge".to_string(),
        };

        let result = synthesize_squash_annotation(&ctx);

        assert_eq!(result.commit, "squash001");
        assert_eq!(result.regions.len(), 3);
        assert_eq!(result.cross_cutting.len(), 3);
        assert_eq!(result.provenance.operation, ProvenanceOperation::Squash);
        assert_eq!(result.provenance.derived_from.len(), 3);
        assert!(result.provenance.original_annotations_preserved);
    }

    #[test]
    fn test_synthesize_squash_overlapping_regions() {
        let ann1 = make_test_annotation("abc123", "src/foo.rs", "connect");
        let mut ann2 = make_test_annotation("def456", "src/foo.rs", "connect");
        // Give ann2 a different constraint
        ann2.regions[0].constraints[0].text = "Constraint from def456".to_string();
        ann2.regions[0].lines = LineRange { start: 5, end: 20 };

        let ctx = SquashSynthesisContext {
            squash_commit: "squash001".to_string(),
            diff: "some diff".to_string(),
            source_annotations: vec![ann1, ann2],
            source_messages: vec![
                ("abc123".to_string(), "First".to_string()),
                ("def456".to_string(), "Second".to_string()),
            ],
            squash_message: "Squash merge".to_string(),
        };

        let result = synthesize_squash_annotation(&ctx);

        // Should have merged into 1 region
        assert_eq!(result.regions.len(), 1);
        // Constraints from both should be preserved
        assert_eq!(result.regions[0].constraints.len(), 2);
        // Line range should encompass both
        assert_eq!(result.regions[0].lines.start, 1);
        assert_eq!(result.regions[0].lines.end, 20);
        // Reasoning should be consolidated
        assert!(result.regions[0]
            .reasoning
            .as_ref()
            .unwrap()
            .contains("abc123"));
        assert!(result.regions[0]
            .reasoning
            .as_ref()
            .unwrap()
            .contains("def456"));
    }

    #[test]
    fn test_synthesize_squash_partial_annotations() {
        let ann1 = make_test_annotation("abc123", "src/foo.rs", "foo_fn");

        let ctx = SquashSynthesisContext {
            squash_commit: "squash001".to_string(),
            diff: "some diff".to_string(),
            source_annotations: vec![ann1],
            source_messages: vec![
                ("abc123".to_string(), "First".to_string()),
                ("def456".to_string(), "Second".to_string()),
                ("ghi789".to_string(), "Third".to_string()),
            ],
            squash_message: "Squash merge".to_string(),
        };

        let result = synthesize_squash_annotation(&ctx);

        assert!(!result.provenance.original_annotations_preserved);
        assert!(result
            .provenance
            .synthesis_notes
            .as_ref()
            .unwrap()
            .contains("1 of 3"));
    }

    #[test]
    fn test_synthesize_squash_no_annotations() {
        let ctx = SquashSynthesisContext {
            squash_commit: "squash001".to_string(),
            diff: "some diff".to_string(),
            source_annotations: vec![],
            source_messages: vec![
                ("abc123".to_string(), "First".to_string()),
                ("def456".to_string(), "Second".to_string()),
            ],
            squash_message: "Squash merge".to_string(),
        };

        let result = synthesize_squash_annotation(&ctx);

        assert_eq!(result.context_level, ContextLevel::Inferred);
        assert!(result.regions.is_empty());
        assert!(!result.provenance.original_annotations_preserved);
    }

    #[test]
    fn test_synthesize_preserves_cross_cutting() {
        let ann1 = make_test_annotation("abc123", "src/foo.rs", "foo_fn");
        let mut ann2 = make_test_annotation("def456", "src/bar.rs", "bar_fn");
        // Add a second cross-cutting concern to ann2
        ann2.cross_cutting.push(CrossCuttingConcern {
            description: "Another concern".to_string(),
            regions: vec![CrossCuttingRegionRef {
                file: "src/bar.rs".to_string(),
                anchor: "bar_fn".to_string(),
            }],
            tags: Vec::new(),
        });

        let ctx = SquashSynthesisContext {
            squash_commit: "squash001".to_string(),
            diff: "some diff".to_string(),
            source_annotations: vec![ann1, ann2],
            source_messages: vec![
                ("abc123".to_string(), "First".to_string()),
                ("def456".to_string(), "Second".to_string()),
            ],
            squash_message: "Squash merge".to_string(),
        };

        let result = synthesize_squash_annotation(&ctx);
        // 1 from ann1, 2 from ann2 = 3 unique cross-cutting concerns
        assert_eq!(result.cross_cutting.len(), 3);
    }

    #[test]
    fn test_migrate_amend_message_only() {
        let old_ann = make_test_annotation("old_sha", "src/foo.rs", "foo_fn");

        let ctx = AmendMigrationContext {
            new_commit: "new_sha".to_string(),
            new_diff: "".to_string(), // empty = message-only
            old_annotation: old_ann,
            new_message: "Updated commit message".to_string(),
        };

        let result = migrate_amend_annotation(&ctx);

        assert_eq!(result.commit, "new_sha");
        assert_eq!(result.provenance.operation, ProvenanceOperation::Amend);
        assert_eq!(result.provenance.derived_from, vec!["old_sha".to_string()]);
        assert!(result.provenance.original_annotations_preserved);
        assert!(result
            .provenance
            .synthesis_notes
            .as_ref()
            .unwrap()
            .contains("Message-only"));
        assert_eq!(result.summary, "Updated commit message");
        // Regions should be preserved
        assert_eq!(result.regions.len(), 1);
    }

    #[test]
    fn test_migrate_amend_with_code_changes() {
        let old_ann = make_test_annotation("old_sha", "src/foo.rs", "foo_fn");

        let ctx = AmendMigrationContext {
            new_commit: "new_sha".to_string(),
            new_diff: "+some new code\n-some old code\n".to_string(),
            old_annotation: old_ann,
            new_message: "Updated commit".to_string(),
        };

        let result = migrate_amend_annotation(&ctx);

        assert_eq!(result.commit, "new_sha");
        assert_eq!(result.provenance.operation, ProvenanceOperation::Amend);
        assert_eq!(result.provenance.derived_from, vec!["old_sha".to_string()]);
        assert!(result
            .provenance
            .synthesis_notes
            .as_ref()
            .unwrap()
            .contains("Migrated from amend"));
        // Regions preserved from original (MVP doesn't re-analyze)
        assert_eq!(result.regions.len(), 1);
    }
}
