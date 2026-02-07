use std::io::Write;

use super::data::ShowData;

/// Render annotated file as plain text. Used when stdout is not a TTY or --no-tui.
pub fn run_plain(data: &ShowData, w: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        w,
        "{} @ {}",
        data.file_path,
        &data.commit[..7.min(data.commit.len())]
    )?;
    writeln!(w)?;

    if data.regions.is_empty() {
        writeln!(w, "  (no annotations)")?;
        return Ok(());
    }

    for r in &data.regions {
        // Region header
        writeln!(
            w,
            "  {}-{}  {} ({})",
            r.region.lines.start,
            r.region.lines.end,
            r.region.ast_anchor.name,
            r.region.ast_anchor.unit_type,
        )?;

        // Intent (always)
        writeln!(w, "        intent:  {}", r.region.intent)?;

        // Reasoning
        if let Some(ref reasoning) = r.region.reasoning {
            writeln!(w, "        reasoning: {reasoning}")?;
        }

        // Constraints
        if !r.region.constraints.is_empty() {
            writeln!(w, "        constraints:")?;
            for c in &r.region.constraints {
                let source = match c.source {
                    crate::schema::v1::ConstraintSource::Author => "author",
                    crate::schema::v1::ConstraintSource::Inferred => "inferred",
                };
                writeln!(w, "          - {} [{source}]", c.text)?;
            }
        }

        // Semantic dependencies
        if !r.region.semantic_dependencies.is_empty() {
            writeln!(w, "        deps:")?;
            for d in &r.region.semantic_dependencies {
                writeln!(w, "          -> {} :: {}", d.file, d.anchor)?;
            }
        }

        // Risk notes
        if let Some(ref risk) = r.region.risk_notes {
            writeln!(w, "        risk: {risk}")?;
        }

        // Corrections
        if !r.region.corrections.is_empty() {
            writeln!(w, "        corrections: {}", r.region.corrections.len())?;
        }

        writeln!(w)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::v1::*;
    use crate::schema::common::*;
    use crate::show::data::RegionRef;

    fn make_test_data() -> ShowData {
        ShowData {
            file_path: "src/main.rs".to_string(),
            commit: "abc1234567890".to_string(),
            source_lines: vec!["fn main() {}".to_string()],
            outline: vec![],
            regions: vec![RegionRef {
                region: RegionAnnotation {
                    file: "src/main.rs".to_string(),
                    ast_anchor: AstAnchor {
                        unit_type: "function".to_string(),
                        name: "main".to_string(),
                        signature: None,
                    },
                    lines: LineRange { start: 1, end: 1 },
                    intent: "Entry point".to_string(),
                    reasoning: Some("Standard main".to_string()),
                    constraints: vec![Constraint {
                        text: "Must not panic".to_string(),
                        source: ConstraintSource::Author,
                    }],
                    semantic_dependencies: vec![SemanticDependency {
                        file: "src/lib.rs".to_string(),
                        anchor: "run".to_string(),
                        nature: "calls".to_string(),
                    }],
                    related_annotations: vec![],
                    tags: vec![],
                    risk_notes: Some("None currently".to_string()),
                    corrections: vec![],
                },
                commit: "abc1234567890".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                summary: "test".to_string(),
                context_level: ContextLevel::Inferred,
                provenance: Provenance {
                    operation: ProvenanceOperation::Initial,
                    derived_from: vec![],
                    original_annotations_preserved: false,
                    synthesis_notes: None,
                },
            }],
            annotation_map: crate::show::data::LineAnnotationMap::build_from_regions(&[], 1),
        }
    }

    #[test]
    fn test_plain_output_contains_intent() {
        let data = make_test_data();
        let mut buf = Vec::new();
        run_plain(&data, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("intent:  Entry point"));
    }

    #[test]
    fn test_plain_output_contains_reasoning() {
        let data = make_test_data();
        let mut buf = Vec::new();
        run_plain(&data, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("reasoning: Standard main"));
    }

    #[test]
    fn test_plain_output_contains_constraints() {
        let data = make_test_data();
        let mut buf = Vec::new();
        run_plain(&data, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Must not panic [author]"));
    }

    #[test]
    fn test_plain_output_contains_deps() {
        let data = make_test_data();
        let mut buf = Vec::new();
        run_plain(&data, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("-> src/lib.rs :: run"));
    }

    #[test]
    fn test_plain_output_empty_annotations() {
        let data = ShowData {
            file_path: "src/empty.rs".to_string(),
            commit: "abc1234".to_string(),
            source_lines: vec![],
            outline: vec![],
            regions: vec![],
            annotation_map: crate::show::data::LineAnnotationMap::build_from_regions(&[], 0),
        };
        let mut buf = Vec::new();
        run_plain(&data, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("(no annotations)"));
    }
}
