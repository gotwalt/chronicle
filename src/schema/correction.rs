use serde::{Deserialize, Serialize};

/// The type of correction applied to an annotation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionType {
    /// General flag that the annotation may be inaccurate
    Flag,
    /// Specific removal of a value from a field
    Remove,
    /// Amendment of a field with new content
    Amend,
}

/// A single correction entry on a region annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    /// Which annotation field this correction targets
    pub field: String,

    /// The type of correction
    pub correction_type: CorrectionType,

    /// Human/agent-readable explanation of the correction
    pub reason: String,

    /// The specific value being removed or amended (for array fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<String>,

    /// Replacement value (for amend corrections)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,

    /// When the correction was made
    pub timestamp: String,

    /// Who made the correction (git author, agent session, etc.)
    pub author: String,
}

/// Confidence penalty per correction on a region.
pub const CORRECTION_PENALTY: f64 = 0.15;

/// Minimum confidence floor (corrections can't reduce below this).
pub const CORRECTION_FLOOR: f64 = 0.1;

/// Apply the confidence penalty for accumulated corrections.
pub fn apply_correction_penalty(base_confidence: f64, correction_count: usize) -> f64 {
    let penalty = correction_count as f64 * CORRECTION_PENALTY;
    (base_confidence - penalty).max(CORRECTION_FLOOR)
}

/// Resolve the author for a correction from git config or environment.
pub fn resolve_author(git: &dyn crate::git::GitOps) -> String {
    // Check ULTRAGIT_SESSION env var first
    if let Ok(session) = std::env::var("ULTRAGIT_SESSION") {
        if !session.is_empty() {
            return session;
        }
    }

    // Fall back to git user.name + user.email
    let name = git
        .config_get("user.name")
        .ok()
        .flatten()
        .unwrap_or_default();
    let email = git
        .config_get("user.email")
        .ok()
        .flatten()
        .unwrap_or_default();

    if !name.is_empty() && !email.is_empty() {
        format!("{name} <{email}>")
    } else if !name.is_empty() {
        name
    } else if !email.is_empty() {
        email
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correction_roundtrip() {
        let correction = Correction {
            field: "constraints".to_string(),
            correction_type: CorrectionType::Remove,
            reason: "No longer required since v2.3".to_string(),
            target_value: Some("Must drain queue".to_string()),
            replacement: None,
            timestamp: "2025-12-20T14:30:00Z".to_string(),
            author: "test-user".to_string(),
        };

        let json = serde_json::to_string(&correction).unwrap();
        let parsed: Correction = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.field, "constraints");
        assert_eq!(parsed.correction_type, CorrectionType::Remove);
        assert_eq!(parsed.reason, "No longer required since v2.3");
        assert_eq!(parsed.target_value.as_deref(), Some("Must drain queue"));
        assert!(parsed.replacement.is_none());
        assert_eq!(parsed.author, "test-user");
    }

    #[test]
    fn test_correction_type_serialization() {
        assert_eq!(
            serde_json::to_string(&CorrectionType::Flag).unwrap(),
            "\"flag\""
        );
        assert_eq!(
            serde_json::to_string(&CorrectionType::Remove).unwrap(),
            "\"remove\""
        );
        assert_eq!(
            serde_json::to_string(&CorrectionType::Amend).unwrap(),
            "\"amend\""
        );
    }

    #[test]
    fn test_correction_type_deserialization() {
        let flag: CorrectionType = serde_json::from_str("\"flag\"").unwrap();
        assert_eq!(flag, CorrectionType::Flag);
        let remove: CorrectionType = serde_json::from_str("\"remove\"").unwrap();
        assert_eq!(remove, CorrectionType::Remove);
        let amend: CorrectionType = serde_json::from_str("\"amend\"").unwrap();
        assert_eq!(amend, CorrectionType::Amend);
    }

    #[test]
    fn test_apply_correction_penalty() {
        assert_eq!(apply_correction_penalty(0.85, 0), 0.85);
        assert_eq!(apply_correction_penalty(0.85, 1), 0.7);
        assert_eq!(apply_correction_penalty(0.85, 2), 0.55);
        // Floor kicks in
        assert_eq!(apply_correction_penalty(0.85, 10), CORRECTION_FLOOR);
        assert_eq!(apply_correction_penalty(0.3, 2), CORRECTION_FLOOR);
    }

    #[test]
    fn test_flag_correction_no_target_value() {
        let correction = Correction {
            field: "intent".to_string(),
            correction_type: CorrectionType::Flag,
            reason: "Annotation seems wrong".to_string(),
            target_value: None,
            replacement: None,
            timestamp: "2025-12-20T14:30:00Z".to_string(),
            author: "tester".to_string(),
        };

        let json = serde_json::to_string(&correction).unwrap();
        // target_value and replacement should be absent due to skip_serializing_if
        assert!(!json.contains("target_value"));
        assert!(!json.contains("replacement"));
    }

    #[test]
    fn test_amend_correction_with_replacement() {
        let correction = Correction {
            field: "reasoning".to_string(),
            correction_type: CorrectionType::Amend,
            reason: "Updated reasoning".to_string(),
            target_value: None,
            replacement: Some("New reasoning text".to_string()),
            timestamp: "2025-12-20T14:30:00Z".to_string(),
            author: "tester".to_string(),
        };

        let json = serde_json::to_string(&correction).unwrap();
        let parsed: Correction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.correction_type, CorrectionType::Amend);
        assert_eq!(parsed.replacement.as_deref(), Some("New reasoning text"));
    }
}
