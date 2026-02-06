use crate::ast::outline::{OutlineEntry, SemanticKind};
use crate::schema::LineRange;

/// How well an anchor matched against the outline.
#[derive(Debug, Clone)]
pub enum AnchorMatch {
    /// Exact match on name and unit_type.
    Exact(OutlineEntry),
    /// Qualified suffix match (e.g. "method" matches "Type::method").
    Qualified(OutlineEntry),
    /// Fuzzy match (within Levenshtein distance 3).
    Fuzzy(OutlineEntry, u32),
}

impl AnchorMatch {
    pub fn entry(&self) -> &OutlineEntry {
        match self {
            AnchorMatch::Exact(e) => e,
            AnchorMatch::Qualified(e) => e,
            AnchorMatch::Fuzzy(e, _) => e,
        }
    }

    pub fn lines(&self) -> LineRange {
        self.entry().lines
    }
}

/// Resolve an anchor name against an outline, returning the best match.
///
/// Matching priority:
/// 1. Exact: kind matches unit_type AND name matches exactly
/// 2. Qualified: name is a suffix of a qualified entry name (e.g. "method" matches "Type::method")
/// 3. Fuzzy: Levenshtein distance <= 3
pub fn resolve(
    outline: &[OutlineEntry],
    unit_type: &str,
    name: &str,
) -> Option<AnchorMatch> {
    let target_kind = SemanticKind::from_str_loose(unit_type);

    // 1. Exact match: kind matches AND name matches exactly
    for entry in outline {
        let kind_matches = target_kind
            .as_ref()
            .is_some_and(|k| *k == entry.kind);
        if kind_matches && entry.name == name {
            return Some(AnchorMatch::Exact(entry.clone()));
        }
    }

    // 2. Qualified suffix match: name is a suffix after "::" in the entry name
    for entry in outline {
        let kind_matches = target_kind
            .as_ref()
            .is_none_or(|k| *k == entry.kind);
        if kind_matches {
            if let Some(suffix) = entry.name.rsplit("::").next() {
                if suffix == name && entry.name != name {
                    return Some(AnchorMatch::Qualified(entry.clone()));
                }
            }
        }
    }

    // 3. Fuzzy match: Levenshtein distance <= 3
    let mut best_match: Option<(OutlineEntry, u32)> = None;
    for entry in outline {
        let kind_matches = target_kind
            .as_ref()
            .is_none_or(|k| *k == entry.kind);
        if !kind_matches {
            continue;
        }

        // Compare against both the full name and the unqualified suffix
        let distances = [
            levenshtein(&entry.name, name),
            entry
                .name
                .rsplit("::")
                .next()
                .map(|suffix| levenshtein(suffix, name))
                .unwrap_or(u32::MAX),
        ];
        let dist = distances.into_iter().min().unwrap_or(u32::MAX);

        if dist <= 3
            && best_match.as_ref().is_none_or(|(_, d)| dist < *d) {
                best_match = Some((entry.clone(), dist));
            }
    }

    best_match.map(|(entry, dist)| AnchorMatch::Fuzzy(entry, dist))
}

/// Simple Levenshtein distance implementation.
fn levenshtein(a: &str, b: &str) -> u32 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n as u32;
    }
    if n == 0 {
        return m as u32;
    }

    // Use two rows instead of full matrix for space efficiency
    let mut prev = vec![0u32; n + 1];
    let mut curr = vec![0u32; n + 1];

    for j in 0..=n {
        prev[j] = j as u32;
    }

    for i in 1..=m {
        curr[0] = i as u32;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "abd"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }
}
