use super::{v1, v2};

/// Migrate a v1 annotation to v2 (canonical) format.
pub fn v1_to_v2(ann: v1::Annotation) -> v2::Annotation {
    let mut markers = Vec::new();
    let mut files_changed: Vec<String> = Vec::new();

    for region in &ann.regions {
        // Track files for the narrative
        if !files_changed.contains(&region.file) {
            files_changed.push(region.file.clone());
        }

        // Convert constraints -> Contract markers
        for constraint in &region.constraints {
            markers.push(v2::CodeMarker {
                file: region.file.clone(),
                anchor: Some(region.ast_anchor.clone()),
                lines: Some(region.lines),
                kind: v2::MarkerKind::Contract {
                    description: constraint.text.clone(),
                    source: match constraint.source {
                        v1::ConstraintSource::Author => v2::ContractSource::Author,
                        v1::ConstraintSource::Inferred => v2::ContractSource::Inferred,
                    },
                },
            });
        }

        // Convert risk_notes -> Hazard markers
        if let Some(ref risk) = region.risk_notes {
            markers.push(v2::CodeMarker {
                file: region.file.clone(),
                anchor: Some(region.ast_anchor.clone()),
                lines: Some(region.lines),
                kind: v2::MarkerKind::Hazard {
                    description: risk.clone(),
                },
            });
        }

        // Convert semantic_dependencies -> Dependency markers
        for dep in &region.semantic_dependencies {
            markers.push(v2::CodeMarker {
                file: region.file.clone(),
                anchor: Some(region.ast_anchor.clone()),
                lines: Some(region.lines),
                kind: v2::MarkerKind::Dependency {
                    target_file: dep.file.clone(),
                    target_anchor: dep.anchor.clone(),
                    assumption: dep.nature.clone(),
                },
            });
        }
    }

    // Build narrative from commit summary + region intents/reasoning
    let mut summary_parts = vec![ann.summary.clone()];
    for region in &ann.regions {
        if let Some(ref reasoning) = region.reasoning {
            summary_parts.push(format!(
                "{} ({}): {}",
                region.file, region.ast_anchor.name, reasoning
            ));
        }
    }
    let summary = if summary_parts.len() == 1 {
        summary_parts.into_iter().next().unwrap()
    } else {
        // For multi-region v1 annotations, join with the first being the main summary
        summary_parts[0].clone()
    };

    // Convert cross-cutting concerns to decisions
    let decisions: Vec<v2::Decision> = ann
        .cross_cutting
        .iter()
        .map(|cc| {
            let scope: Vec<String> = cc
                .regions
                .iter()
                .map(|r| format!("{}:{}", r.file, r.anchor))
                .collect();
            v2::Decision {
                what: cc.description.clone(),
                why: "Migrated from v1 cross-cutting concern".to_string(),
                stability: v2::Stability::Permanent,
                revisit_when: None,
                scope,
            }
        })
        .collect();

    // Convert provenance
    let provenance = v2::Provenance {
        source: v2::ProvenanceSource::MigratedV1,
        derived_from: ann.provenance.derived_from,
        notes: ann.provenance.synthesis_notes,
    };

    // Build effort link from task if present
    let effort = ann.task.map(|task| v2::EffortLink {
        id: task.clone(),
        description: task,
        phase: v2::EffortPhase::InProgress,
    });

    v2::Annotation {
        schema: "chronicle/v2".to_string(),
        commit: ann.commit,
        timestamp: ann.timestamp,
        narrative: v2::Narrative {
            summary,
            motivation: None,
            rejected_alternatives: Vec::new(),
            follow_up: None,
            files_changed,
        },
        decisions,
        markers,
        effort,
        provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::common::{AstAnchor, LineRange};
    use crate::schema::v1;

    fn make_v1_annotation() -> v1::Annotation {
        v1::Annotation {
            schema: "chronicle/v1".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            task: Some("TASK-42".to_string()),
            summary: "Add reconnect logic".to_string(),
            context_level: v1::ContextLevel::Enhanced,
            regions: vec![v1::RegionAnnotation {
                file: "src/mqtt/reconnect.rs".to_string(),
                ast_anchor: AstAnchor {
                    unit_type: "function".to_string(),
                    name: "attempt_reconnect".to_string(),
                    signature: Some("fn attempt_reconnect(&mut self)".to_string()),
                },
                lines: LineRange { start: 10, end: 30 },
                intent: "Handle reconnection with exponential backoff".to_string(),
                reasoning: Some("Broker rate-limits rapid reconnects".to_string()),
                constraints: vec![v1::Constraint {
                    text: "Must not exceed 60s backoff".to_string(),
                    source: v1::ConstraintSource::Author,
                }],
                semantic_dependencies: vec![v1::SemanticDependency {
                    file: "src/tls/session.rs".to_string(),
                    anchor: "TlsSessionCache::max_sessions".to_string(),
                    nature: "assumes max_sessions is 4".to_string(),
                }],
                related_annotations: vec![],
                tags: vec!["mqtt".to_string()],
                risk_notes: Some("Backoff timer is not persisted across restarts".to_string()),
                corrections: vec![],
            }],
            cross_cutting: vec![v1::CrossCuttingConcern {
                description: "All reconnect paths must use exponential backoff".to_string(),
                regions: vec![v1::CrossCuttingRegionRef {
                    file: "src/mqtt/reconnect.rs".to_string(),
                    anchor: "attempt_reconnect".to_string(),
                }],
                tags: vec![],
            }],
            provenance: v1::Provenance {
                operation: v1::ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }
    }

    #[test]
    fn test_v1_to_v2_basic() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        assert_eq!(v2_ann.schema, "chronicle/v2");
        assert_eq!(v2_ann.commit, "abc123");
        assert_eq!(v2_ann.timestamp, "2025-01-01T00:00:00Z");
        assert_eq!(v2_ann.narrative.summary, "Add reconnect logic");
        assert_eq!(
            v2_ann.narrative.files_changed,
            vec!["src/mqtt/reconnect.rs"]
        );
    }

    #[test]
    fn test_v1_to_v2_markers() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        // Should have 3 markers: 1 contract, 1 hazard, 1 dependency
        assert_eq!(v2_ann.markers.len(), 3);

        // Contract from constraint
        assert!(matches!(
            &v2_ann.markers[0].kind,
            v2::MarkerKind::Contract { description, .. } if description == "Must not exceed 60s backoff"
        ));

        // Hazard from risk_notes
        assert!(matches!(
            &v2_ann.markers[1].kind,
            v2::MarkerKind::Hazard { description } if description.contains("not persisted")
        ));

        // Dependency from semantic_dependencies
        assert!(matches!(
            &v2_ann.markers[2].kind,
            v2::MarkerKind::Dependency { target_file, target_anchor, assumption }
                if target_file == "src/tls/session.rs"
                && target_anchor == "TlsSessionCache::max_sessions"
                && assumption == "assumes max_sessions is 4"
        ));
    }

    #[test]
    fn test_v1_to_v2_decisions() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        // Cross-cutting concern becomes a decision
        assert_eq!(v2_ann.decisions.len(), 1);
        assert_eq!(
            v2_ann.decisions[0].what,
            "All reconnect paths must use exponential backoff"
        );
    }

    #[test]
    fn test_v1_to_v2_effort() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        // Task becomes effort link
        let effort = v2_ann.effort.unwrap();
        assert_eq!(effort.id, "TASK-42");
    }

    #[test]
    fn test_v1_to_v2_provenance() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        assert_eq!(v2_ann.provenance.source, v2::ProvenanceSource::MigratedV1);
    }

    #[test]
    fn test_v1_to_v2_validates() {
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);

        assert!(v2_ann.validate().is_ok());
    }

    #[test]
    fn test_v1_to_v2_empty_regions() {
        let v1_ann = v1::Annotation {
            schema: "chronicle/v1".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            task: None,
            summary: "Simple commit".to_string(),
            context_level: v1::ContextLevel::Inferred,
            regions: vec![],
            cross_cutting: vec![],
            provenance: v1::Provenance {
                operation: v1::ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        };
        let v2_ann = v1_to_v2(v1_ann);

        assert!(v2_ann.markers.is_empty());
        assert!(v2_ann.decisions.is_empty());
        assert!(v2_ann.effort.is_none());
        assert!(v2_ann.validate().is_ok());
    }
}
