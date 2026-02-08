/// Check if two file paths refer to the same file.
/// Normalizes by stripping leading "./" if present.
pub(crate) fn file_matches(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> &str {
        s.strip_prefix("./").unwrap_or(s)
    }
    norm(a) == norm(b)
}

/// Check if an annotation anchor matches a query anchor.
/// Supports unqualified matching: "max_sessions" matches "TlsSessionCache::max_sessions"
/// and vice versa.
pub(crate) fn anchor_matches(region_anchor: &str, query_anchor: &str) -> bool {
    if region_anchor == query_anchor {
        return true;
    }
    let region_short = region_anchor.rsplit("::").next().unwrap_or(region_anchor);
    let query_short = query_anchor.rsplit("::").next().unwrap_or(query_anchor);
    region_short == query_anchor || region_anchor == query_short || region_short == query_short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_matches_exact() {
        assert!(file_matches("src/main.rs", "src/main.rs"));
    }

    #[test]
    fn test_file_matches_dot_slash() {
        assert!(file_matches("./src/main.rs", "src/main.rs"));
        assert!(file_matches("src/main.rs", "./src/main.rs"));
    }

    #[test]
    fn test_file_no_match() {
        assert!(!file_matches("src/lib.rs", "src/main.rs"));
    }

    #[test]
    fn test_anchor_matches_exact() {
        assert!(anchor_matches("max_sessions", "max_sessions"));
    }

    #[test]
    fn test_anchor_matches_unqualified_dep() {
        assert!(anchor_matches(
            "max_sessions",
            "TlsSessionCache::max_sessions"
        ));
    }

    #[test]
    fn test_anchor_matches_unqualified_query() {
        assert!(anchor_matches(
            "TlsSessionCache::max_sessions",
            "max_sessions"
        ));
    }

    #[test]
    fn test_anchor_no_match() {
        assert!(!anchor_matches("other_fn", "max_sessions"));
    }
}
