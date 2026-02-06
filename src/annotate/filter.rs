use crate::annotate::gather::AnnotationContext;

/// Decision from pre-LLM filtering.
#[derive(Debug, PartialEq, Eq)]
pub enum FilterDecision {
    /// Proceed with full LLM annotation.
    Annotate,
    /// Skip annotation entirely (lockfile-only, merge commits, etc.)
    Skip(String),
    /// Produce a minimal local annotation without calling the LLM.
    Trivial(String),
}

/// Lockfile patterns that indicate no meaningful code changes.
const LOCKFILE_PATTERNS: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Gemfile.lock",
    "poetry.lock",
];

/// Default trivial threshold: if total changed lines <= this, mark as trivial.
const TRIVIAL_THRESHOLD: usize = 3;

/// Check if this commit should be annotated, skipped, or trivially handled.
pub fn pre_llm_filter(context: &AnnotationContext) -> FilterDecision {
    // Skip: commit message matches skip patterns
    let msg = context.commit_message.trim();
    if msg.starts_with("Merge branch") {
        return FilterDecision::Skip("merge commit".to_string());
    }
    if msg.starts_with("WIP") {
        return FilterDecision::Skip("work-in-progress commit".to_string());
    }
    if msg.starts_with("fixup!") {
        return FilterDecision::Skip("fixup commit".to_string());
    }
    if msg.starts_with("squash!") {
        return FilterDecision::Skip("squash commit".to_string());
    }

    // Skip: all files are lockfiles
    if !context.diffs.is_empty()
        && context.diffs.iter().all(|d| {
            LOCKFILE_PATTERNS.iter().any(|pattern| {
                d.path.ends_with(pattern)
            })
        })
    {
        return FilterDecision::Skip("lockfile-only changes".to_string());
    }

    // Trivial: total changed lines <= threshold
    let total_changed: usize = context
        .diffs
        .iter()
        .map(|d| d.changed_line_count())
        .sum();

    if total_changed <= TRIVIAL_THRESHOLD {
        return FilterDecision::Trivial(format!(
            "trivial change ({} lines changed)",
            total_changed
        ));
    }

    FilterDecision::Annotate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{FileDiff, DiffStatus, Hunk, HunkLine};

    fn make_context(message: &str, diffs: Vec<FileDiff>) -> AnnotationContext {
        AnnotationContext {
            commit_sha: "abc123".to_string(),
            commit_message: message.to_string(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            diffs,
            author_context: None,
        }
    }

    fn make_diff(path: &str, added: usize, removed: usize) -> FileDiff {
        let mut lines = Vec::new();
        for _ in 0..added {
            lines.push(HunkLine::Added("+ line".to_string()));
        }
        for _ in 0..removed {
            lines.push(HunkLine::Removed("- line".to_string()));
        }
        FileDiff {
            path: path.to_string(),
            old_path: None,
            status: DiffStatus::Modified,
            hunks: vec![Hunk {
                old_start: 1,
                old_count: removed as u32,
                new_start: 1,
                new_count: added as u32,
                header: String::new(),
                lines,
            }],
        }
    }

    #[test]
    fn test_skip_merge() {
        let ctx = make_context("Merge branch 'feature' into main", vec![]);
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(_)));
    }

    #[test]
    fn test_skip_wip() {
        let ctx = make_context("WIP stuff", vec![]);
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(_)));
    }

    #[test]
    fn test_skip_lockfile_only() {
        let ctx = make_context(
            "Update deps",
            vec![make_diff("Cargo.lock", 10, 5)],
        );
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(_)));
    }

    #[test]
    fn test_trivial() {
        let ctx = make_context(
            "Fix typo",
            vec![make_diff("src/main.rs", 1, 1)],
        );
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Trivial(_)));
    }

    #[test]
    fn test_annotate() {
        let ctx = make_context(
            "Add new feature",
            vec![make_diff("src/main.rs", 20, 5)],
        );
        assert_eq!(pre_llm_filter(&ctx), FilterDecision::Annotate);
    }
}
