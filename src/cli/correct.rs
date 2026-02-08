use crate::error::Result;
use crate::git::{CliOps, GitOps};
use crate::schema::v1;
type Annotation = v1::Annotation;
use crate::schema::correction::{resolve_author, Correction, CorrectionType};

/// Run the `git chronicle correct` command.
///
/// Applies a precise correction to a specific field in a region annotation.
pub fn run(
    sha: String,
    region_anchor: String,
    field: String,
    remove: Option<String>,
    amend: Option<String>,
) -> Result<()> {
    if remove.is_none() && amend.is_none() {
        return Err(crate::error::ChronicleError::Config {
            message: "At least one of --remove or --amend must be specified.".to_string(),
            location: snafu::Location::default(),
        });
    }

    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    // Resolve short SHA to full if needed
    let full_sha = git_ops
        .resolve_ref(&sha)
        .map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    // Read the existing annotation
    let note_opt = git_ops
        .note_read(&full_sha)
        .map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    let note = match note_opt {
        Some(n) => n,
        None => {
            return Err(crate::error::ChronicleError::Config {
                message: format!("No annotation found for commit {sha}. Cannot apply correction."),
                location: snafu::Location::default(),
            });
        }
    };

    let mut annotation: Annotation =
        serde_json::from_str(&note).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;

    // Find the matching region by anchor name
    let region_idx = annotation
        .regions
        .iter()
        .position(|r| r.ast_anchor.name == region_anchor);

    let region_idx = match region_idx {
        Some(i) => i,
        None => {
            let available: Vec<&str> = annotation
                .regions
                .iter()
                .map(|r| r.ast_anchor.name.as_str())
                .collect();
            return Err(crate::error::ChronicleError::Config {
                message: format!(
                    "No region matching '{}' found in annotation for commit {}. Available regions: {}",
                    region_anchor,
                    sha,
                    available.join(", ")
                ),
                location: snafu::Location::default(),
            });
        }
    };

    // Validate that the field exists and is non-empty
    validate_field(&annotation.regions[region_idx], &field)?;

    // Determine correction type
    let correction_type = if remove.is_some() && amend.is_some() {
        // Both --remove and --amend: remove the old, provide replacement
        CorrectionType::Amend
    } else if remove.is_some() {
        CorrectionType::Remove
    } else {
        CorrectionType::Amend
    };

    let author = resolve_author(&git_ops);
    let timestamp = chrono::Utc::now().to_rfc3339();

    let reason = match (&remove, &amend) {
        (Some(val), Some(replacement)) => {
            format!("Removed '{}', replaced with '{}'", val, replacement)
        }
        (Some(val), None) => format!("Removed '{}'", val),
        (None, Some(text)) => format!("Amended with '{}'", text),
        (None, None) => unreachable!(),
    };

    let correction = Correction {
        field: field.clone(),
        correction_type,
        reason,
        target_value: remove.clone(),
        replacement: amend.clone(),
        timestamp,
        author,
    };

    annotation.regions[region_idx].corrections.push(correction);

    let updated_json = serde_json::to_string_pretty(&annotation).map_err(|e| {
        crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    git_ops.note_write(&full_sha, &updated_json).map_err(|e| {
        crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    let short_sha = &full_sha[..7.min(full_sha.len())];
    eprintln!("Corrected annotation on commit {short_sha}, region {region_anchor}");
    eprintln!("  Field: {field}");
    if let Some(ref val) = remove {
        eprintln!("  Removed: \"{val}\"");
    }
    if let Some(ref val) = amend {
        eprintln!("  Amended: \"{val}\"");
    }
    eprintln!("  Correction stored in refs/notes/chronicle");

    Ok(())
}

/// Validate that the given field name corresponds to a non-empty field on the region.
fn validate_field(region: &crate::schema::v1::RegionAnnotation, field: &str) -> Result<()> {
    let is_empty = match field {
        "intent" => region.intent.is_empty(),
        "reasoning" => region.reasoning.as_ref().is_none_or(|s| s.is_empty()),
        "constraints" => region.constraints.is_empty(),
        "risk_notes" => region.risk_notes.as_ref().is_none_or(|s| s.is_empty()),
        "semantic_dependencies" => region.semantic_dependencies.is_empty(),
        "tags" => region.tags.is_empty(),
        other => {
            return Err(crate::error::ChronicleError::Config {
                message: format!(
                    "Unknown field '{}'. Valid fields: intent, reasoning, constraints, risk_notes, semantic_dependencies, tags",
                    other
                ),
                location: snafu::Location::default(),
            });
        }
    };

    if is_empty {
        return Err(crate::error::ChronicleError::Config {
            message: format!(
                "Field '{}' is empty in region '{}'. Nothing to correct.",
                field, region.ast_anchor.name
            ),
            location: snafu::Location::default(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::*;
    use crate::schema::correction::Correction;
    use crate::schema::v1::*;

    fn make_region() -> RegionAnnotation {
        RegionAnnotation {
            file: "src/main.rs".to_string(),
            ast_anchor: AstAnchor {
                unit_type: "fn".to_string(),
                name: "connect".to_string(),
                signature: None,
            },
            lines: LineRange { start: 1, end: 10 },
            intent: "Connects to broker".to_string(),
            reasoning: Some("Uses mTLS for security".to_string()),
            constraints: vec![Constraint {
                text: "Must drain queue before reconnecting".to_string(),
                source: ConstraintSource::Author,
            }],
            semantic_dependencies: vec![SemanticDependency {
                file: "src/tls.rs".to_string(),
                anchor: "TlsConfig".to_string(),
                nature: "uses".to_string(),
            }],
            related_annotations: vec![],
            tags: vec!["mqtt".to_string(), "networking".to_string()],
            risk_notes: Some("High risk if TLS config changes".to_string()),
            corrections: vec![],
        }
    }

    #[test]
    fn test_validate_field_valid() {
        let region = make_region();
        assert!(validate_field(&region, "intent").is_ok());
        assert!(validate_field(&region, "reasoning").is_ok());
        assert!(validate_field(&region, "constraints").is_ok());
        assert!(validate_field(&region, "risk_notes").is_ok());
        assert!(validate_field(&region, "semantic_dependencies").is_ok());
        assert!(validate_field(&region, "tags").is_ok());
    }

    #[test]
    fn test_validate_field_unknown() {
        let region = make_region();
        let result = validate_field(&region, "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown field"));
    }

    #[test]
    fn test_validate_field_empty() {
        let mut region = make_region();
        region.constraints = vec![];
        let result = validate_field(&region, "constraints");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("empty"));
    }

    #[test]
    fn test_validate_field_none_reasoning() {
        let mut region = make_region();
        region.reasoning = None;
        let result = validate_field(&region, "reasoning");
        assert!(result.is_err());
    }

    #[test]
    fn test_correction_accumulates_on_region() {
        let mut region = make_region();

        let c1 = Correction {
            field: "constraints".to_string(),
            correction_type: CorrectionType::Remove,
            reason: "No longer required".to_string(),
            target_value: Some("Must drain queue before reconnecting".to_string()),
            replacement: None,
            timestamp: "2025-12-20T14:30:00Z".to_string(),
            author: "tester".to_string(),
        };
        region.corrections.push(c1);

        let c2 = Correction {
            field: "reasoning".to_string(),
            correction_type: CorrectionType::Amend,
            reason: "Updated reasoning".to_string(),
            target_value: None,
            replacement: Some("Uses mTLS v2".to_string()),
            timestamp: "2025-12-21T10:00:00Z".to_string(),
            author: "tester".to_string(),
        };
        region.corrections.push(c2);

        assert_eq!(region.corrections.len(), 2);
        assert_eq!(
            region.corrections[0].correction_type,
            CorrectionType::Remove
        );
        assert_eq!(region.corrections[1].correction_type, CorrectionType::Amend);
    }

    #[test]
    fn test_corrections_survive_json_roundtrip() {
        let mut region = make_region();
        region.corrections.push(Correction {
            field: "constraints".to_string(),
            correction_type: CorrectionType::Flag,
            reason: "Seems wrong".to_string(),
            target_value: None,
            replacement: None,
            timestamp: "2025-12-20T14:30:00Z".to_string(),
            author: "tester".to_string(),
        });

        let json = serde_json::to_string_pretty(&region).unwrap();
        let parsed: RegionAnnotation = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.corrections.len(), 1);
        assert_eq!(parsed.corrections[0].field, "constraints");
        assert_eq!(parsed.corrections[0].correction_type, CorrectionType::Flag);
    }

    #[test]
    fn test_annotation_without_corrections_deserializes() {
        // Simulate an annotation from before corrections were added (no corrections field)
        let json = r#"{
            "file": "src/main.rs",
            "ast_anchor": {"unit_type": "fn", "name": "main", "signature": null},
            "lines": {"start": 1, "end": 10},
            "intent": "entry point",
            "reasoning": null,
            "constraints": [],
            "semantic_dependencies": [],
            "related_annotations": [],
            "tags": [],
            "risk_notes": null
        }"#;

        let region: RegionAnnotation = serde_json::from_str(json).unwrap();
        assert!(region.corrections.is_empty());
    }
}
