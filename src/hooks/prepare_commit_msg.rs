use std::path::Path;

use crate::annotate::squash::{write_pending_squash, PendingSquash};
use crate::error::chronicle_error::IoSnafu;
use crate::error::Result;
use snafu::ResultExt;

/// Handle the prepare-commit-msg hook.
///
/// Detects squash operations and writes pending-squash.json so that
/// the post-commit hook can route to squash synthesis.
///
/// Detection signals (any one sufficient):
/// 1. `commit_source` argument is "squash"
/// 2. `.git/SQUASH_MSG` file exists
/// 3. `CHRONICLE_SQUASH_SOURCES` environment variable is set
pub fn handle_prepare_commit_msg(git_dir: &Path, commit_source: Option<&str>) -> Result<()> {
    let source_commits = match detect_squash(commit_source, git_dir)? {
        Some(commits) => commits,
        None => return Ok(()), // Not a squash
    };

    if source_commits.is_empty() {
        tracing::debug!("Squash detected but no source commits resolved");
        return Ok(());
    }

    let pending = PendingSquash {
        source_commits,
        source_ref: None,
        timestamp: chrono::Utc::now(),
    };

    write_pending_squash(git_dir, &pending)?;
    tracing::info!(
        "Wrote pending-squash.json with {} source commits",
        pending.source_commits.len()
    );

    Ok(())
}

/// Detect whether this commit is a squash operation and resolve source commit SHAs.
fn detect_squash(commit_source: Option<&str>, git_dir: &Path) -> Result<Option<Vec<String>>> {
    // Check 1: hook argument
    if commit_source == Some("squash") {
        return resolve_squash_sources_from_squash_msg(git_dir);
    }

    // Check 2: SQUASH_MSG file existence
    let squash_msg_path = git_dir.join("SQUASH_MSG");
    if squash_msg_path.exists() {
        return resolve_squash_sources_from_squash_msg(git_dir);
    }

    // Check 3: environment variable
    if let Ok(sources) = std::env::var("CHRONICLE_SQUASH_SOURCES") {
        if !sources.is_empty() {
            return Ok(Some(parse_squash_sources_env(&sources)));
        }
    }

    Ok(None)
}

/// Parse source commit SHAs from the SQUASH_MSG file.
///
/// During `git merge --squash`, SQUASH_MSG contains lines like:
/// ```text
/// Squashed commit of the following:
///
/// commit abc1234...
/// Author: ...
/// Date: ...
///     First commit message
///
/// commit def5678...
/// ...
/// ```
fn resolve_squash_sources_from_squash_msg(git_dir: &Path) -> Result<Option<Vec<String>>> {
    let squash_msg_path = git_dir.join("SQUASH_MSG");
    if !squash_msg_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&squash_msg_path).context(IoSnafu)?;
    let shas = parse_squash_msg_commits(&content);

    if shas.is_empty() {
        Ok(None)
    } else {
        Ok(Some(shas))
    }
}

/// Parse commit SHAs from SQUASH_MSG content.
fn parse_squash_msg_commits(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("commit ") {
                // Take the first word (the SHA), ignoring any trailing info
                let sha = rest.split_whitespace().next()?;
                if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    Some(sha.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
}

/// Parse source commits from the CHRONICLE_SQUASH_SOURCES env var.
/// Supports comma-separated SHA list.
fn parse_squash_sources_env(sources: &str) -> Vec<String> {
    sources
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_squash_msg_commits() {
        let content = r#"Squashed commit of the following:

commit abc1234567890abcdef1234567890abcdef123456
Author: Test User <test@example.com>
Date:   Mon Dec 15 10:30:00 2025 +0000

    First commit message

commit def4567890abcdef1234567890abcdef123456ab
Author: Test User <test@example.com>
Date:   Mon Dec 15 10:35:00 2025 +0000

    Second commit message
"#;

        let shas = parse_squash_msg_commits(content);
        assert_eq!(shas.len(), 2);
        assert_eq!(shas[0], "abc1234567890abcdef1234567890abcdef123456");
        assert_eq!(shas[1], "def4567890abcdef1234567890abcdef123456ab");
    }

    #[test]
    fn test_parse_squash_msg_no_commits() {
        let content = "Just a regular commit message\nwith no commit lines\n";
        let shas = parse_squash_msg_commits(content);
        assert!(shas.is_empty());
    }

    #[test]
    fn test_parse_squash_sources_env_comma_separated() {
        let sources = "abc123,def456,ghi789";
        let shas = parse_squash_sources_env(sources);
        assert_eq!(shas, vec!["abc123", "def456", "ghi789"]);
    }

    #[test]
    fn test_parse_squash_sources_env_with_spaces() {
        let sources = "abc123 , def456 , ghi789";
        let shas = parse_squash_sources_env(sources);
        assert_eq!(shas, vec!["abc123", "def456", "ghi789"]);
    }

    #[test]
    fn test_parse_squash_sources_env_empty() {
        let sources = "";
        let shas = parse_squash_sources_env(sources);
        assert!(shas.is_empty());
    }

    #[test]
    fn test_detect_squash_hook_arg() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();

        // Create a SQUASH_MSG file so the resolution path works
        let squash_msg = "Squashed commit of the following:\n\ncommit abc1234567890abcdef1234567890abcdef123456\nAuthor: Test\nDate: now\n\n    msg\n";
        std::fs::write(git_dir.join("SQUASH_MSG"), squash_msg).unwrap();

        let result = detect_squash(Some("squash"), git_dir).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_detect_squash_message_arg() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_squash(Some("message"), dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_squash_no_signals() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect_squash(None, dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_squash_squash_msg_file() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        let squash_msg =
            "Squashed commit of the following:\n\ncommit abcdef1234567\nAuthor: Test\n\n    msg\n";
        std::fs::write(git_dir.join("SQUASH_MSG"), squash_msg).unwrap();

        // No hook argument, but SQUASH_MSG exists
        let result = detect_squash(None, git_dir).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_handle_prepare_commit_msg_writes_pending() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        let squash_msg =
            "Squashed commit of the following:\n\ncommit abcdef1234567\nAuthor: Test\n\n    msg\n";
        std::fs::write(git_dir.join("SQUASH_MSG"), squash_msg).unwrap();

        handle_prepare_commit_msg(git_dir, Some("squash")).unwrap();

        let pending_path = git_dir.join("chronicle").join("pending-squash.json");
        assert!(pending_path.exists());

        let content = std::fs::read_to_string(pending_path).unwrap();
        let pending: PendingSquash = serde_json::from_str(&content).unwrap();
        assert_eq!(pending.source_commits.len(), 1);
        assert_eq!(pending.source_commits[0], "abcdef1234567");
    }

    #[test]
    fn test_handle_prepare_commit_msg_no_squash() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();

        handle_prepare_commit_msg(git_dir, Some("message")).unwrap();

        let pending_path = git_dir.join("chronicle").join("pending-squash.json");
        assert!(!pending_path.exists());
    }
}
