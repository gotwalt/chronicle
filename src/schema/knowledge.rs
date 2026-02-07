use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::v2::Stability;

/// The top-level knowledge store, stored as a git note on the empty tree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KnowledgeStore {
    pub schema: String, // "chronicle/knowledge-v1"
    #[serde(default)]
    pub conventions: Vec<Convention>,
    #[serde(default)]
    pub boundaries: Vec<ModuleBoundary>,
    #[serde(default)]
    pub anti_patterns: Vec<AntiPattern>,
}

impl KnowledgeStore {
    pub fn new() -> Self {
        Self {
            schema: "chronicle/knowledge-v1".to_string(),
            conventions: Vec::new(),
            boundaries: Vec::new(),
            anti_patterns: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.conventions.is_empty() && self.boundaries.is_empty() && self.anti_patterns.is_empty()
    }

    /// Remove an entry by ID. Returns true if found and removed.
    pub fn remove_by_id(&mut self, id: &str) -> bool {
        let len_before =
            self.conventions.len() + self.boundaries.len() + self.anti_patterns.len();

        self.conventions.retain(|c| c.id != id);
        self.boundaries.retain(|b| b.id != id);
        self.anti_patterns.retain(|a| a.id != id);

        let len_after =
            self.conventions.len() + self.boundaries.len() + self.anti_patterns.len();
        len_after < len_before
    }
}

impl Default for KnowledgeStore {
    fn default() -> Self {
        Self::new()
    }
}

/// A coding convention or rule.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Convention {
    pub id: String,
    pub scope: String,
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_in: Option<String>,
    pub stability: Stability,
}

/// A module boundary definition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleBoundary {
    pub id: String,
    pub module: String,
    pub owns: String,
    pub boundary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_in: Option<String>,
}

/// An anti-pattern to avoid.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AntiPattern {
    pub id: String,
    pub pattern: String,
    pub instead: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learned_from: Option<String>,
}

/// Filtered knowledge applicable to a specific file.
#[derive(Debug, Clone, Serialize)]
pub struct FilteredKnowledge {
    pub conventions: Vec<Convention>,
    pub boundaries: Vec<ModuleBoundary>,
    pub anti_patterns: Vec<AntiPattern>,
}

impl FilteredKnowledge {
    pub fn is_empty(&self) -> bool {
        self.conventions.is_empty() && self.boundaries.is_empty() && self.anti_patterns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_store_new() {
        let store = KnowledgeStore::new();
        assert_eq!(store.schema, "chronicle/knowledge-v1");
        assert!(store.is_empty());
    }

    #[test]
    fn test_knowledge_store_remove_by_id() {
        let mut store = KnowledgeStore::new();
        store.conventions.push(Convention {
            id: "conv-1".to_string(),
            scope: "src/".to_string(),
            rule: "Use snafu for errors".to_string(),
            decided_in: None,
            stability: Stability::Permanent,
        });
        store.anti_patterns.push(AntiPattern {
            id: "ap-1".to_string(),
            pattern: "unwrap() in production code".to_string(),
            instead: "Use proper error handling".to_string(),
            learned_from: None,
        });

        assert!(store.remove_by_id("conv-1"));
        assert!(store.conventions.is_empty());
        assert_eq!(store.anti_patterns.len(), 1);

        assert!(!store.remove_by_id("nonexistent"));
    }

    #[test]
    fn test_knowledge_store_roundtrip() {
        let mut store = KnowledgeStore::new();
        store.conventions.push(Convention {
            id: "conv-1".to_string(),
            scope: "src/schema/".to_string(),
            rule: "Use parse_annotation() for all deserialization".to_string(),
            decided_in: Some("abc123".to_string()),
            stability: Stability::Permanent,
        });
        store.boundaries.push(ModuleBoundary {
            id: "bound-1".to_string(),
            module: "src/git/".to_string(),
            owns: "All git operations".to_string(),
            boundary: "Must not import from provider module".to_string(),
            decided_in: None,
        });
        store.anti_patterns.push(AntiPattern {
            id: "ap-1".to_string(),
            pattern: "serde_json::from_str for annotations".to_string(),
            instead: "Use schema::parse_annotation()".to_string(),
            learned_from: Some("BUG-42".to_string()),
        });

        let json = serde_json::to_string_pretty(&store).unwrap();
        let parsed: KnowledgeStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.conventions.len(), 1);
        assert_eq!(parsed.boundaries.len(), 1);
        assert_eq!(parsed.anti_patterns.len(), 1);
        assert_eq!(parsed.conventions[0].id, "conv-1");
    }
}
