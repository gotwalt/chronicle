pub mod common;
pub mod correction;
pub mod migrate;
pub mod v1;
pub mod v2;

// Re-export shared types used across all versions.
pub use common::{AstAnchor, LineRange};

// Re-export correction types (version-independent).
pub use correction::*;

// The canonical annotation type is always the latest version.
pub use v2::Annotation;
pub use v2::*;

/// Parse an annotation from JSON, detecting the schema version and migrating
/// to the canonical (latest) type.
///
/// This is the single deserialization chokepoint. All code that reads
/// annotations from git notes should call this instead of using
/// `serde_json::from_str` directly.
pub fn parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError> {
    // Peek at the schema field to determine version.
    let peek: SchemaVersion =
        serde_json::from_str(json).map_err(|e| ParseAnnotationError::InvalidJson {
            source: e,
        })?;

    match peek.schema.as_str() {
        "chronicle/v2" => {
            serde_json::from_str::<v2::Annotation>(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })
        }
        "chronicle/v1" => {
            let v1_ann: v1::Annotation = serde_json::from_str(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })?;
            Ok(migrate::v1_to_v2(v1_ann))
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
        assert_eq!(ann.schema, "chronicle/v2");
        assert_eq!(ann.commit, "abc123");
        assert_eq!(ann.narrative.summary, "Test commit");
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
        assert_eq!(ann.schema, "chronicle/v2");
        assert_eq!(ann.commit, "def456");
        assert_eq!(ann.narrative.summary, "Direct v2 annotation");
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
        assert_eq!(ann.schema, "chronicle/v2");
        assert_eq!(ann.narrative.summary, "Test commit");
        assert_eq!(ann.narrative.files_changed, vec!["src/foo.rs"]);

        // Constraint -> Contract marker
        assert!(ann.markers.iter().any(|m| matches!(
            &m.kind,
            MarkerKind::Contract { description, .. } if description == "Must not allocate"
        )));

        // risk_notes -> Hazard marker
        assert!(ann.markers.iter().any(|m| matches!(
            &m.kind,
            MarkerKind::Hazard { description } if description.contains("panic")
        )));

        // semantic_dependencies -> Dependency marker
        assert!(ann.markers.iter().any(|m| matches!(
            &m.kind,
            MarkerKind::Dependency { target_file, target_anchor, .. }
                if target_file == "src/bar.rs" && target_anchor == "bar"
        )));

        // Cross-cutting -> Decision
        assert_eq!(ann.decisions.len(), 1);
        assert_eq!(ann.decisions[0].what, "All paths validate input");
    }
}
