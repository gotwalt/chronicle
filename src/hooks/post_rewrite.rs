use crate::annotate::squash::{migrate_amend_annotation, AmendMigrationContext};
use crate::error::chronicle_error::{GitSnafu, JsonSnafu};
use crate::error::Result;
use crate::git::GitOps;
use crate::schema::v1;
type Annotation = v1::Annotation;
use snafu::ResultExt;

/// A mapping from old SHA to new SHA, as provided by git's post-rewrite hook.
#[derive(Debug, Clone)]
pub struct RewriteMapping {
    pub old_sha: String,
    pub new_sha: String,
}

/// Handle the post-rewrite hook.
///
/// Git calls post-rewrite with the rewrite type as the first argument
/// ("amend" or "rebase") and old→new SHA mappings on stdin.
///
/// For v1, we only handle "amend" rewrites. Rebase is logged and skipped.
pub fn handle_post_rewrite(
    git_ops: &dyn GitOps,
    rewrite_type: &str,
    mappings: &[RewriteMapping],
) -> Result<()> {
    if rewrite_type != "amend" {
        tracing::info!(
            "post-rewrite: {} rewrites not yet supported, skipping {} mappings",
            rewrite_type,
            mappings.len()
        );
        return Ok(());
    }

    for mapping in mappings {
        if let Err(e) = handle_single_amend(git_ops, &mapping.old_sha, &mapping.new_sha) {
            tracing::warn!(
                "Failed to migrate annotation from {} to {}: {}",
                mapping.old_sha,
                mapping.new_sha,
                e
            );
            // Continue with other mappings; don't fail the whole hook
        }
    }

    Ok(())
}

/// Handle a single amend migration: copy/update the annotation from old SHA to new SHA.
fn handle_single_amend(git_ops: &dyn GitOps, old_sha: &str, new_sha: &str) -> Result<()> {
    // Read the old annotation
    let old_note = git_ops.note_read(old_sha).context(GitSnafu)?;
    let old_json = match old_note {
        Some(json) => json,
        None => {
            tracing::debug!("No annotation for old commit {old_sha}, skipping amend migration");
            return Ok(());
        }
    };

    let old_annotation: Annotation = serde_json::from_str(&old_json).context(JsonSnafu)?;

    // Get the new commit's message
    let new_info = git_ops.commit_info(new_sha).context(GitSnafu)?;

    // Compute diff to determine if code changed or message-only amend
    // We use the diff of the new commit (new_sha vs its parent)
    let new_diffs = git_ops.diff(new_sha).context(GitSnafu)?;
    let old_diffs = git_ops.diff(old_sha).context(GitSnafu)?;

    // Simple heuristic: if the diffs have the same content, it's message-only
    let new_diff_text = format!("{:?}", new_diffs);
    let old_diff_text = format!("{:?}", old_diffs);
    let diff_for_migration = if new_diff_text == old_diff_text {
        String::new() // message-only amend
    } else {
        new_diff_text
    };

    let ctx = AmendMigrationContext {
        new_commit: new_sha.to_string(),
        new_diff: diff_for_migration,
        old_annotation,
        new_message: new_info.message,
    };

    let new_annotation = migrate_amend_annotation(&ctx);

    let json = serde_json::to_string_pretty(&new_annotation).context(JsonSnafu)?;
    git_ops.note_write(new_sha, &json).context(GitSnafu)?;

    tracing::info!("Migrated annotation from {old_sha} to {new_sha}");
    Ok(())
}

/// Parse stdin lines from git post-rewrite into RewriteMapping pairs.
///
/// Format: `old_sha new_sha\n` per line. Extra fields (like newline) are ignored.
pub fn parse_rewrite_mappings(input: &str) -> Vec<RewriteMapping> {
    input
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(RewriteMapping {
                    old_sha: parts[0].to_string(),
                    new_sha: parts[1].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rewrite_mappings_single() {
        let input = "abc123 def456\n";
        let mappings = parse_rewrite_mappings(input);
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].old_sha, "abc123");
        assert_eq!(mappings[0].new_sha, "def456");
    }

    #[test]
    fn test_parse_rewrite_mappings_multiple() {
        let input = "abc123 def456\nghi789 jkl012\nmno345 pqr678\n";
        let mappings = parse_rewrite_mappings(input);
        assert_eq!(mappings.len(), 3);
        assert_eq!(mappings[0].old_sha, "abc123");
        assert_eq!(mappings[0].new_sha, "def456");
        assert_eq!(mappings[1].old_sha, "ghi789");
        assert_eq!(mappings[1].new_sha, "jkl012");
        assert_eq!(mappings[2].old_sha, "mno345");
        assert_eq!(mappings[2].new_sha, "pqr678");
    }

    #[test]
    fn test_parse_rewrite_mappings_empty() {
        let input = "";
        let mappings = parse_rewrite_mappings(input);
        assert!(mappings.is_empty());
    }

    #[test]
    fn test_parse_rewrite_mappings_blank_lines() {
        let input = "abc123 def456\n\nghi789 jkl012\n";
        let mappings = parse_rewrite_mappings(input);
        assert_eq!(mappings.len(), 2);
    }

    #[test]
    fn test_parse_rewrite_mappings_extra_fields() {
        // Git may include extra info after the two SHAs
        let input = "abc123 def456 extra info\n";
        let mappings = parse_rewrite_mappings(input);
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].old_sha, "abc123");
        assert_eq!(mappings[0].new_sha, "def456");
    }

    #[test]
    fn test_parse_rewrite_mappings_malformed_line() {
        let input = "only_one_sha\nabc123 def456\n";
        let mappings = parse_rewrite_mappings(input);
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].old_sha, "abc123");
    }
}
