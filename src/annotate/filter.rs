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

/// Binary file extensions that indicate non-code content.
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "woff", "woff2", "ttf", "eot", "pdf",
    "zip", "tar", "gz", "bz2", "exe", "dll", "so", "dylib", "pyc", "class", "o", "obj",
];

/// Generated/vendored file patterns that aren't worth annotating.
const GENERATED_PATTERNS: &[&str] = &[
    ".min.js",
    ".min.css",
    "vendor/",
    "vendored/",
    "node_modules/",
    ".generated.",
    "_generated.",
    "dist/",
    "build/",
];

/// Check if a file path refers to a binary file based on extension.
fn is_binary_path(path: &str) -> bool {
    if let Some(ext) = path.rsplit('.').next() {
        BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    } else {
        false
    }
}

/// Check if a file path refers to a generated or vendored file.
fn is_generated_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    GENERATED_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

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

    // Skip: all files are binary
    if !context.diffs.is_empty() && context.diffs.iter().all(|d| is_binary_path(&d.path)) {
        return FilterDecision::Skip("binary-only changes".to_string());
    }

    // Skip: all files are generated/vendored
    if !context.diffs.is_empty() && context.diffs.iter().all(|d| is_generated_path(&d.path)) {
        return FilterDecision::Skip("generated/vendored changes".to_string());
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

    #[test]
    fn test_skip_binary_only() {
        let ctx = make_context(
            "Add logo",
            vec![make_diff("assets/logo.png", 10, 0)],
        );
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(ref s) if s.contains("binary")));
    }

    #[test]
    fn test_skip_generated_only() {
        let ctx = make_context(
            "Update vendored deps",
            vec![make_diff("vendor/lib.js", 100, 50)],
        );
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(ref s) if s.contains("generated")));
    }

    #[test]
    fn test_mixed_binary_and_code() {
        let ctx = make_context(
            "Add feature with icon",
            vec![
                make_diff("src/main.rs", 20, 5),
                make_diff("assets/icon.png", 10, 0),
            ],
        );
        assert_eq!(pre_llm_filter(&ctx), FilterDecision::Annotate);
    }

    #[test]
    fn test_skip_min_js_only() {
        let ctx = make_context(
            "Rebuild minified assets",
            vec![make_diff("dist/app.min.js", 500, 400)],
        );
        assert!(matches!(pre_llm_filter(&ctx), FilterDecision::Skip(ref s) if s.contains("generated")));
    }

    #[test]
    fn test_is_binary_path() {
        assert!(is_binary_path("logo.png"));
        assert!(is_binary_path("path/to/image.JPG"));
        assert!(is_binary_path("lib.so"));
        assert!(!is_binary_path("src/main.rs"));
        assert!(!is_binary_path("README.md"));
    }

    #[test]
    fn test_is_generated_path() {
        assert!(is_generated_path("vendor/lib.js"));
        assert!(is_generated_path("dist/bundle.js"));
        assert!(is_generated_path("app.min.js"));
        assert!(is_generated_path("node_modules/foo/index.js"));
        assert!(!is_generated_path("src/main.rs"));
    }
}
