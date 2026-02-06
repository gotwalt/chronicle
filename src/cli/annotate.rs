use crate::annotate::squash::{
    collect_source_annotations, collect_source_messages, migrate_amend_annotation,
    synthesize_squash_annotation, AmendMigrationContext, SquashSynthesisContext,
};
use crate::error::chronicle_error::{GitSnafu, JsonSnafu};
use crate::error::Result;
use crate::git::{CliOps, GitOps};
use snafu::ResultExt;

pub async fn run(
    commit: String,
    sync: bool,
    live: bool,
    squash_sources: Option<String>,
    amend_source: Option<String>,
) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

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

    let provider = crate::provider::discover_provider()
        .map_err(|e| crate::error::ChronicleError::Provider {
            source: e,
            location: snafu::Location::default(),
        })?;

    let annotation = crate::annotate::run(&git_ops, provider.as_ref(), &commit, sync)
        .await?;

    let json = serde_json::to_string_pretty(&annotation).map_err(|e| {
        crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;
    println!("{json}");

    Ok(())
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

    // Collect source annotations and messages
    let source_ann_pairs = collect_source_annotations(git_ops, &source_shas);
    let source_annotations: Vec<_> = source_ann_pairs
        .into_iter()
        .filter_map(|(_, ann)| ann)
        .collect();
    let source_messages = collect_source_messages(git_ops, &source_shas);

    // Get squash commit info
    let commit_info = git_ops.commit_info(&resolved_commit).context(GitSnafu)?;

    let ctx = SquashSynthesisContext {
        squash_commit: resolved_commit.clone(),
        diff: String::new(), // MVP: not used for deterministic merge
        source_annotations,
        source_messages,
        squash_message: commit_info.message,
    };

    let annotation = synthesize_squash_annotation(&ctx);

    // Write as git note
    let json = serde_json::to_string_pretty(&annotation).context(JsonSnafu)?;
    git_ops.note_write(&resolved_commit, &json).context(GitSnafu)?;

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

    let old_annotation: crate::schema::Annotation =
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
    git_ops.note_write(&resolved_commit, &json).context(GitSnafu)?;

    println!("{json}");
    Ok(())
}

/// Live annotation path: read AnnotateInput JSON from stdin, call handle_annotate,
/// print AnnotateResult JSON to stdout. Zero LLM cost.
fn run_live(git_ops: &CliOps) -> Result<()> {
    let stdin = std::io::read_to_string(std::io::stdin()).map_err(|e| {
        crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    let input: crate::mcp::annotate_handler::AnnotateInput =
        serde_json::from_str(&stdin).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;

    let result = crate::mcp::annotate_handler::handle_annotate(git_ops, input)?;

    let json = serde_json::to_string_pretty(&result).map_err(|e| {
        crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;
    println!("{json}");

    Ok(())
}
