use crate::annotate::squash::{
    collect_source_annotations_v3, collect_source_messages, migrate_amend_annotation,
    synthesize_squash_annotation_v3, AmendMigrationContext, SquashSynthesisContextV3,
};
use crate::error::chronicle_error::{GitSnafu, JsonSnafu};
use crate::error::Result;
use crate::git::{CliOps, GitOps};
use snafu::ResultExt;

pub struct AnnotateArgs {
    pub commit: String,
    pub live: bool,
    pub squash_sources: Option<String>,
    pub amend_source: Option<String>,
    pub summary: Option<String>,
    pub json_input: Option<String>,
    pub auto: bool,
}

pub fn run(args: AnnotateArgs) -> Result<()> {
    let AnnotateArgs {
        commit,
        live,
        squash_sources,
        amend_source,
        summary,
        json_input,
        auto,
    } = args;
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir.clone());

    // Read staged notes (best-effort, don't fail annotation if staging is broken)
    let git_dir = repo_dir.join(".git");
    let staged_notes_text = crate::annotate::staging::read_staged(&git_dir)
        .ok()
        .filter(|notes| !notes.is_empty())
        .map(|notes| crate::annotate::staging::format_for_provenance(&notes));

    // --summary: quick annotation with just a summary string
    if let Some(summary_text) = summary {
        let input = crate::annotate::live::LiveInput {
            commit,
            summary: summary_text,
            wisdom: vec![],
            staged_notes: staged_notes_text.clone(),
        };
        let result = crate::annotate::live::handle_annotate_v3(&git_ops, input)?;
        let _ = crate::annotate::staging::clear_staged(&git_dir);
        let json = serde_json::to_string_pretty(&result).context(JsonSnafu)?;
        println!("{json}");
        return Ok(());
    }

    // --json: full annotation JSON on command line
    if let Some(json_str) = json_input {
        let mut input: crate::annotate::live::LiveInput =
            serde_json::from_str(&json_str).context(JsonSnafu)?;
        input.staged_notes = staged_notes_text.clone();
        let result = crate::annotate::live::handle_annotate_v3(&git_ops, input)?;
        let _ = crate::annotate::staging::clear_staged(&git_dir);
        let json = serde_json::to_string_pretty(&result).context(JsonSnafu)?;
        println!("{json}");
        return Ok(());
    }

    // --auto: use commit message as summary
    if auto {
        let full_sha = git_ops.resolve_ref(&commit).context(GitSnafu)?;
        let commit_info = git_ops.commit_info(&full_sha).context(GitSnafu)?;
        let input = crate::annotate::live::LiveInput {
            commit,
            summary: commit_info.message,
            wisdom: vec![],
            staged_notes: staged_notes_text.clone(),
        };
        let result = crate::annotate::live::handle_annotate_v3(&git_ops, input)?;
        let _ = crate::annotate::staging::clear_staged(&git_dir);
        let json = serde_json::to_string_pretty(&result).context(JsonSnafu)?;
        println!("{json}");
        return Ok(());
    }

    if live {
        return run_live(&git_ops);
    }

    // Handle --squash-sources
    if let Some(sources) = squash_sources {
        return run_squash_synthesis(&git_ops, &commit, &sources);
    }

    // Handle --amend-source
    if let Some(old_sha) = amend_source {
        return run_amend_migration(&git_ops, &commit, &old_sha);
    }

    Err(crate::error::ChronicleError::Validation {
        message: "no annotation mode specified; use --live, --summary, --json, --auto, --squash-sources, or --amend-source".to_string(),
        location: snafu::Location::default(),
    })
}

/// Run squash synthesis from explicit source SHAs (for CI).
fn run_squash_synthesis(git_ops: &CliOps, commit: &str, sources: &str) -> Result<()> {
    let source_shas: Vec<String> = sources
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if source_shas.is_empty() {
        return Err(crate::error::ChronicleError::Validation {
            message: "--squash-sources requires at least one source SHA".to_string(),
            location: snafu::Location::default(),
        });
    }

    // Resolve the commit SHA
    let resolved_commit = git_ops.resolve_ref(commit).context(GitSnafu)?;

    // Collect source annotations (v3-normalized) and messages
    let source_ann_pairs = collect_source_annotations_v3(git_ops, &source_shas);
    let source_annotations: Vec<_> = source_ann_pairs
        .into_iter()
        .filter_map(|(_, ann)| ann)
        .collect();
    let source_messages = collect_source_messages(git_ops, &source_shas);

    // Get squash commit info
    let commit_info = git_ops.commit_info(&resolved_commit).context(GitSnafu)?;

    let ctx = SquashSynthesisContextV3 {
        squash_commit: resolved_commit.clone(),
        source_annotations,
        source_messages,
        squash_message: commit_info.message,
    };

    let annotation = synthesize_squash_annotation_v3(&ctx);

    // Write as git note
    let json = serde_json::to_string_pretty(&annotation).context(JsonSnafu)?;
    git_ops
        .note_write(&resolved_commit, &json)
        .context(GitSnafu)?;

    println!("{json}");
    Ok(())
}

/// Run amend migration from an explicit old SHA.
fn run_amend_migration(git_ops: &CliOps, commit: &str, old_sha: &str) -> Result<()> {
    let resolved_commit = git_ops.resolve_ref(commit).context(GitSnafu)?;

    // Read old annotation
    let old_note = git_ops.note_read(old_sha).context(GitSnafu)?;
    let old_json = match old_note {
        Some(json) => json,
        None => {
            return Err(crate::error::ChronicleError::Validation {
                message: format!("No annotation found for old commit {old_sha}"),
                location: snafu::Location::default(),
            });
        }
    };

    let old_annotation: crate::schema::v1::Annotation =
        serde_json::from_str(&old_json).context(JsonSnafu)?;

    let new_info = git_ops.commit_info(&resolved_commit).context(GitSnafu)?;

    // Compute diff comparison to determine if code changed
    let new_diffs = git_ops.diff(&resolved_commit).context(GitSnafu)?;
    let old_diffs = git_ops.diff(old_sha).context(GitSnafu)?;
    let new_diff_text = format!("{:?}", new_diffs);
    let old_diff_text = format!("{:?}", old_diffs);
    let diff_for_migration = if new_diff_text == old_diff_text {
        String::new()
    } else {
        new_diff_text
    };

    let ctx = AmendMigrationContext {
        new_commit: resolved_commit.clone(),
        new_diff: diff_for_migration,
        old_annotation,
        new_message: new_info.message,
    };

    let annotation = migrate_amend_annotation(&ctx);

    let json = serde_json::to_string_pretty(&annotation).context(JsonSnafu)?;
    git_ops
        .note_write(&resolved_commit, &json)
        .context(GitSnafu)?;

    println!("{json}");
    Ok(())
}

/// Live annotation path: read v3 JSON from stdin, write annotation. Zero LLM cost.
fn run_live(git_ops: &CliOps) -> Result<()> {
    let stdin = std::io::read_to_string(std::io::stdin()).map_err(|e| {
        crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    let value: serde_json::Value =
        serde_json::from_str(&stdin).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;

    if value.get("regions").is_some() {
        return Err(crate::error::ChronicleError::Validation {
            message: "v1 annotation format is no longer supported for writing; use v3 format"
                .to_string(),
            location: snafu::Location::default(),
        });
    }

    let input: crate::annotate::live::LiveInput =
        serde_json::from_value(value).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;

    let result = crate::annotate::live::handle_annotate_v3(git_ops, input)?;
    let json =
        serde_json::to_string_pretty(&result).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;
    println!("{json}");

    Ok(())
}
