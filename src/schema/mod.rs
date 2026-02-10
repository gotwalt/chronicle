pub mod common;
pub mod correction;
pub mod knowledge;
pub mod migrate;
pub mod v1;
pub mod v2;
pub mod v3;

// Re-export shared types used across all versions.
pub use common::{AstAnchor, LineRange};

// Re-export correction types (version-independent).
pub use correction::*;

// The canonical annotation type is always the latest version.
pub use v3::Annotation;
pub use v3::*;

/// Parse an annotation from JSON, detecting the schema version and migrating
/// to the canonical (latest) type.
///
/// This is the single deserialization chokepoint. All code that reads
/// annotations from git notes should call this instead of using
/// `serde_json::from_str` directly.
pub fn parse_annotation(json: &str) -> Result<v3::Annotation, ParseAnnotationError> {
    // Peek at the schema field to determine version.
    let peek: SchemaVersion =
        serde_json::from_str(json).map_err(|e| ParseAnnotationError::InvalidJson { source: e })?;

    match peek.schema.as_str() {
        "chronicle/v3" => serde_json::from_str::<v3::Annotation>(json)
            .map_err(|e| ParseAnnotationError::InvalidJson { source: e }),
        "chronicle/v2" => {
            let v2_ann: v2::Annotation = serde_json::from_str(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })?;
            Ok(migrate::v2_to_v3(v2_ann))
        }
        "chronicle/v1" => {
            let v1_ann: v1::Annotation = serde_json::from_str(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })?;
            let v2_ann = migrate::v1_to_v2(v1_ann);
            Ok(migrate::v2_to_v3(v2_ann))
        }
        other => Err(ParseAnnotationError::UnknownVersion {
            version: other.to_string(),
        }),
    }
}

/// Minimal struct to peek at the schema version without full deserialization.
#[derive(serde::Deserialize)]
struct SchemaVersion {
    schema: String,
}

/// Peek at the schema version string from raw annotation JSON without full parsing.
pub fn peek_version(json: &str) -> Option<String> {
    serde_json::from_str::<SchemaVersion>(json)
        .ok()
        .map(|sv| sv.schema)
}

/// Errors from `parse_annotation`.
#[derive(Debug)]
pub enum ParseAnnotationError {
    InvalidJson { source: serde_json::Error },
    UnknownVersion { version: String },
}

impl std::fmt::Display for ParseAnnotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseAnnotationError::InvalidJson { source } => {
                write!(f, "invalid annotation JSON: {source}")
            }
            ParseAnnotationError::UnknownVersion { version } => {
                write!(f, "unknown annotation schema version: {version}")
            }
        }
    }
}

impl std::error::Error for ParseAnnotationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseAnnotationError::InvalidJson { source } => Some(source),
            ParseAnnotationError::UnknownVersion { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_v1_annotation() {
        let json = r#"{
            "schema": "chronicle/v1",
            "commit": "abc123",
            "timestamp": "2025-01-01T00:00:00Z",
            "summary": "Test commit",
            "context_level": "enhanced",
            "regions": [],
            "provenance": {
                "operation": "initial",
                "derived_from": [],
                "original_annotations_preserved": false
            }
        }"#;

        let ann = parse_annotation(json).unwrap();
        // v1 -> v2 -> v3
        assert_eq!(ann.schema, "chronicle/v3");
        assert_eq!(ann.commit, "abc123");
        assert_eq!(ann.summary, "Test commit");
        assert_eq!(ann.provenance.source, ProvenanceSource::MigratedV1);
    }

    #[test]
    fn test_parse_v2_annotation() {
        let json = r#"{
            "schema": "chronicle/v2",
            "commit": "def456",
            "timestamp": "2025-01-02T00:00:00Z",
            "narrative": {
                "summary": "Direct v2 annotation"
            },
            "provenance": {
                "source": "live"
            }
        }"#;

        let ann = parse_annotation(json).unwrap();
        // v2 -> v3
        assert_eq!(ann.schema, "chronicle/v3");
        assert_eq!(ann.commit, "def456");
        assert_eq!(ann.summary, "Direct v2 annotation");
        assert_eq!(ann.provenance.source, ProvenanceSource::Live);
    }

    #[test]
    fn test_parse_v3_annotation() {
        let json = r#"{
            "schema": "chronicle/v3",
            "commit": "ghi789",
            "timestamp": "2025-06-01T00:00:00Z",
            "summary": "Native v3 annotation",
            "provenance": {
                "source": "live"
            }
        }"#;

        let ann = parse_annotation(json).unwrap();
        assert_eq!(ann.schema, "chronicle/v3");
        assert_eq!(ann.commit, "ghi789");
        assert_eq!(ann.summary, "Native v3 annotation");
        assert_eq!(ann.provenance.source, ProvenanceSource::Live);
    }

    #[test]
    fn test_parse_unknown_version() {
        let json = r#"{"schema": "chronicle/v99", "commit": "abc"}"#;
        let result = parse_annotation(json);
        assert!(matches!(
            result,
            Err(ParseAnnotationError::UnknownVersion { .. })
        ));
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_annotation("not json");
        assert!(matches!(
            result,
            Err(ParseAnnotationError::InvalidJson { .. })
        ));
    }

    #[test]
    fn test_v1_roundtrip_preserves_data() {
        let json = r#"{
            "schema": "chronicle/v1",
            "commit": "abc123",
            "timestamp": "2025-01-01T00:00:00Z",
            "summary": "Test commit",
            "context_level": "enhanced",
            "regions": [{
                "file": "src/foo.rs",
                "ast_anchor": {"unit_type": "function", "name": "foo"},
                "lines": {"start": 1, "end": 10},
                "intent": "Do something",
                "constraints": [{"text": "Must not allocate", "source": "author"}],
                "risk_notes": "Could panic on empty input",
                "semantic_dependencies": [
                    {"file": "src/bar.rs", "anchor": "bar", "nature": "calls bar"}
                ]
            }],
            "cross_cutting": [{
                "description": "All paths validate input",
                "regions": [{"file": "src/foo.rs", "anchor": "foo"}]
            }],
            "provenance": {
                "operation": "initial",
                "derived_from": [],
                "original_annotations_preserved": false
            }
        }"#;

        let ann = parse_annotation(json).unwrap();
        assert_eq!(ann.schema, "chronicle/v3");
        assert_eq!(ann.summary, "Test commit");

        // v1 constraint -> v2 Contract marker -> v3 gotcha wisdom
        assert!(ann
            .wisdom
            .iter()
            .any(|w| w.category == WisdomCategory::Gotcha && w.content == "Must not allocate"));

        // v1 risk_notes -> v2 Hazard -> v3 gotcha wisdom
        assert!(ann
            .wisdom
            .iter()
            .any(|w| w.category == WisdomCategory::Gotcha && w.content.contains("panic")));

        // v1 semantic_dependencies -> v2 Dependency -> v3 insight wisdom
        assert!(ann
            .wisdom
            .iter()
            .any(|w| w.category == WisdomCategory::Insight && w.content.contains("src/bar.rs")));

        // v1 cross-cutting -> v2 Decision -> v3 insight wisdom
        assert!(ann
            .wisdom
            .iter()
            .any(|w| w.category == WisdomCategory::Insight
                && w.content.contains("All paths validate input")));
    }
}
