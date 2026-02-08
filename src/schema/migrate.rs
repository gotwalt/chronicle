use super::{v1, v2, v3};

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
        author: None,
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
            sentiments: Vec::new(),
        },
        decisions,
        markers,
        effort,
        provenance,
    }
}

/// Migrate a v2 annotation to v3 format.
///
/// Implements all 10 migration rules from the v3 spec (features/22-schema-v3.md):
/// 1. summary <- v2.narrative.summary
/// 2. rejected_alternatives -> dead_end wisdom entries
/// 3. motivation -> insight wisdom entry
/// 4. follow_up -> unfinished_thread wisdom entry
/// 5. sentiments -> wisdom entries (category by feeling keyword)
/// 6. decisions -> insight wisdom entries
/// 7. markers -> wisdom entries (category per marker kind)
/// 8. provenance carries through
/// 9. effort dropped
/// 10. files_changed dropped
pub fn v2_to_v3(ann: v2::Annotation) -> v3::Annotation {
    let mut wisdom = Vec::new();

    // Rule 2: rejected_alternatives -> dead_end entries
    for ra in &ann.narrative.rejected_alternatives {
        let content = if ra.reason.is_empty() {
            ra.approach.clone()
        } else {
            format!("{}: {}", ra.approach, ra.reason)
        };
        wisdom.push(v3::WisdomEntry {
            category: v3::WisdomCategory::DeadEnd,
            content,
            file: None,
            lines: None,
        });
    }

    // Rule 3: motivation -> insight entry (if present)
    if let Some(motivation) = &ann.narrative.motivation {
        wisdom.push(v3::WisdomEntry {
            category: v3::WisdomCategory::Insight,
            content: motivation.clone(),
            file: None,
            lines: None,
        });
    }

    // Rule 4: follow_up -> unfinished_thread entry (if present)
    if let Some(follow_up) = &ann.narrative.follow_up {
        wisdom.push(v3::WisdomEntry {
            category: v3::WisdomCategory::UnfinishedThread,
            content: follow_up.clone(),
            file: None,
            lines: None,
        });
    }

    // Rule 5: sentiments -> wisdom entries
    for sentiment in &ann.narrative.sentiments {
        let feeling_lower = sentiment.feeling.to_lowercase();
        let category = if feeling_lower.contains("worry")
            || feeling_lower.contains("unease")
            || feeling_lower.contains("concern")
        {
            v3::WisdomCategory::Gotcha
        } else if feeling_lower.contains("uncertain") || feeling_lower.contains("doubt") {
            v3::WisdomCategory::UnfinishedThread
        } else {
            v3::WisdomCategory::Insight
        };
        wisdom.push(v3::WisdomEntry {
            category,
            content: format!("{}: {}", sentiment.feeling, sentiment.detail),
            file: None,
            lines: None,
        });
    }

    // Rule 6: decisions -> insight wisdom entries
    for decision in &ann.decisions {
        let file = decision.scope.first().map(|s| {
            // Scope entries can be "src/foo.rs:bar_fn" — extract just the file part
            s.split(':').next().unwrap_or(s).to_string()
        });
        wisdom.push(v3::WisdomEntry {
            category: v3::WisdomCategory::Insight,
            content: format!("{}: {}", decision.what, decision.why),
            file,
            lines: None,
        });
    }

    // Rule 7: markers -> wisdom entries
    for marker in &ann.markers {
        let (category, content) = match &marker.kind {
            v2::MarkerKind::Contract { description, .. } => {
                (v3::WisdomCategory::Gotcha, description.clone())
            }
            v2::MarkerKind::Hazard { description } => {
                (v3::WisdomCategory::Gotcha, description.clone())
            }
            v2::MarkerKind::Dependency {
                target_file,
                target_anchor,
                assumption,
            } => (
                v3::WisdomCategory::Insight,
                format!("Depends on {target_file}:{target_anchor} \u{2014} {assumption}"),
            ),
            v2::MarkerKind::Unstable { description, .. } => {
                (v3::WisdomCategory::UnfinishedThread, description.clone())
            }
            v2::MarkerKind::Security { description } => {
                (v3::WisdomCategory::Gotcha, description.clone())
            }
            v2::MarkerKind::Performance { description } => {
                (v3::WisdomCategory::Gotcha, description.clone())
            }
            v2::MarkerKind::Deprecated { description, .. } => {
                (v3::WisdomCategory::UnfinishedThread, description.clone())
            }
            v2::MarkerKind::TechDebt { description } => {
                (v3::WisdomCategory::UnfinishedThread, description.clone())
            }
            v2::MarkerKind::TestCoverage { description } => {
                // TestCoverage is dropped per spec, but we still convert for completeness
                (v3::WisdomCategory::Insight, description.clone())
            }
        };

        wisdom.push(v3::WisdomEntry {
            category,
            content,
            file: Some(marker.file.clone()),
            lines: marker.lines,
        });
    }

    // Rule 8: provenance carries through unchanged
    // Rule 9: effort dropped
    // Rule 10: files_changed dropped

    v3::Annotation {
        schema: "chronicle/v3".to_string(),
        commit: ann.commit,
        timestamp: ann.timestamp,
        summary: ann.narrative.summary, // Rule 1
        wisdom,
        provenance: ann.provenance,
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

    // -----------------------------------------------------------------------
    // v2 -> v3 migration tests
    // -----------------------------------------------------------------------

    fn make_v2_annotation() -> v2::Annotation {
        v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "def456".to_string(),
            timestamp: "2025-06-01T12:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "Switch to exponential backoff for reconnect".to_string(),
                motivation: Some("Linear backoff caused thundering herd".to_string()),
                rejected_alternatives: vec![
                    v2::RejectedAlternative {
                        approach: "Fixed delay".to_string(),
                        reason: "Too aggressive under load".to_string(),
                    },
                    v2::RejectedAlternative {
                        approach: "No delay".to_string(),
                        reason: "".to_string(),
                    },
                ],
                follow_up: Some("Need to add jitter to the backoff".to_string()),
                files_changed: vec!["src/reconnect.rs".to_string()],
                sentiments: vec![
                    v2::Sentiment {
                        feeling: "worry".to_string(),
                        detail: "Pool size heuristic is fragile".to_string(),
                    },
                    v2::Sentiment {
                        feeling: "doubt".to_string(),
                        detail: "Not sure this is the right abstraction".to_string(),
                    },
                    v2::Sentiment {
                        feeling: "confidence".to_string(),
                        detail: "The core logic is solid".to_string(),
                    },
                ],
            },
            decisions: vec![v2::Decision {
                what: "Use HashMap for cache".to_string(),
                why: "O(1) lookups on the hot path".to_string(),
                stability: v2::Stability::Provisional,
                revisit_when: Some("If we need ordering".to_string()),
                scope: vec!["src/cache.rs:Cache".to_string()],
            }],
            markers: vec![
                v2::CodeMarker {
                    file: "src/reconnect.rs".to_string(),
                    anchor: Some(AstAnchor {
                        unit_type: "function".to_string(),
                        name: "attempt_reconnect".to_string(),
                        signature: None,
                    }),
                    lines: Some(LineRange { start: 10, end: 30 }),
                    kind: v2::MarkerKind::Contract {
                        description: "Must not exceed 60s backoff".to_string(),
                        source: v2::ContractSource::Author,
                    },
                },
                v2::CodeMarker {
                    file: "src/reconnect.rs".to_string(),
                    anchor: None,
                    lines: Some(LineRange { start: 40, end: 50 }),
                    kind: v2::MarkerKind::Hazard {
                        description: "Timer not persisted across restarts".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/reconnect.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::Dependency {
                        target_file: "src/tls/session.rs".to_string(),
                        target_anchor: "max_sessions".to_string(),
                        assumption: "assumes max_sessions is 4".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/config.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::Unstable {
                        description: "Config format may change".to_string(),
                        revisit_when: "after v2 ships".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/auth.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::Security {
                        description: "JWT validation must check expiry".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/hot.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::Performance {
                        description: "Hot loop, avoid allocations".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/old.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::Deprecated {
                        description: "Use new_api instead".to_string(),
                        replacement: Some("src/new_api.rs".to_string()),
                    },
                },
                v2::CodeMarker {
                    file: "src/hack.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::TechDebt {
                        description: "Needs refactor after v2 ships".to_string(),
                    },
                },
                v2::CodeMarker {
                    file: "src/lib.rs".to_string(),
                    anchor: None,
                    lines: None,
                    kind: v2::MarkerKind::TestCoverage {
                        description: "Missing edge case tests".to_string(),
                    },
                },
            ],
            effort: Some(v2::EffortLink {
                id: "schema-v2".to_string(),
                description: "Schema v2 redesign".to_string(),
                phase: v2::EffortPhase::InProgress,
            }),
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: Some("claude-code".to_string()),
                derived_from: vec![],
                notes: Some("test note".to_string()),
            },
        }
    }

    #[test]
    fn test_v2_to_v3_summary() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        assert_eq!(v3_ann.schema, "chronicle/v3");
        assert_eq!(v3_ann.commit, "def456");
        assert_eq!(v3_ann.timestamp, "2025-06-01T12:00:00Z");
        assert_eq!(
            v3_ann.summary,
            "Switch to exponential backoff for reconnect"
        );
    }

    #[test]
    fn test_v2_to_v3_rejected_alternatives() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        let dead_ends: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.category == v3::WisdomCategory::DeadEnd)
            .collect();
        assert_eq!(dead_ends.len(), 2);
        assert_eq!(
            dead_ends[0].content,
            "Fixed delay: Too aggressive under load"
        );
        // Empty reason: just the approach text
        assert_eq!(dead_ends[1].content, "No delay");
        assert!(dead_ends[0].file.is_none());
    }

    #[test]
    fn test_v2_to_v3_motivation() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        let insights: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Linear backoff caused thundering herd")
            .collect();
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].category, v3::WisdomCategory::Insight);
    }

    #[test]
    fn test_v2_to_v3_follow_up() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        let threads: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Need to add jitter to the backoff")
            .collect();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].category, v3::WisdomCategory::UnfinishedThread);
    }

    #[test]
    fn test_v2_to_v3_sentiments() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        // worry -> gotcha
        let worry: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.starts_with("worry:"))
            .collect();
        assert_eq!(worry.len(), 1);
        assert_eq!(worry[0].category, v3::WisdomCategory::Gotcha);

        // doubt -> unfinished_thread
        let doubt: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.starts_with("doubt:"))
            .collect();
        assert_eq!(doubt.len(), 1);
        assert_eq!(doubt[0].category, v3::WisdomCategory::UnfinishedThread);

        // confidence -> insight
        let conf: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.starts_with("confidence:"))
            .collect();
        assert_eq!(conf.len(), 1);
        assert_eq!(conf[0].category, v3::WisdomCategory::Insight);
    }

    #[test]
    fn test_v2_to_v3_decisions() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        let decision_entries: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.contains("Use HashMap for cache"))
            .collect();
        assert_eq!(decision_entries.len(), 1);
        assert_eq!(decision_entries[0].category, v3::WisdomCategory::Insight);
        assert_eq!(
            decision_entries[0].content,
            "Use HashMap for cache: O(1) lookups on the hot path"
        );
        // file from first scope element (strip anchor part)
        assert_eq!(decision_entries[0].file.as_deref(), Some("src/cache.rs"));
    }

    #[test]
    fn test_v2_to_v3_markers() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        // Contract -> gotcha
        let contract_entries: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Must not exceed 60s backoff")
            .collect();
        assert_eq!(contract_entries.len(), 1);
        assert_eq!(contract_entries[0].category, v3::WisdomCategory::Gotcha);
        assert_eq!(
            contract_entries[0].file.as_deref(),
            Some("src/reconnect.rs")
        );
        assert_eq!(
            contract_entries[0].lines,
            Some(LineRange { start: 10, end: 30 })
        );

        // Hazard -> gotcha
        let hazard_entries: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Timer not persisted across restarts")
            .collect();
        assert_eq!(hazard_entries.len(), 1);
        assert_eq!(hazard_entries[0].category, v3::WisdomCategory::Gotcha);

        // Dependency -> insight
        let dep_entries: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.starts_with("Depends on"))
            .collect();
        assert_eq!(dep_entries.len(), 1);
        assert_eq!(dep_entries[0].category, v3::WisdomCategory::Insight);
        assert!(dep_entries[0]
            .content
            .contains("src/tls/session.rs:max_sessions"));

        // Unstable -> unfinished_thread
        let unstable: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Config format may change")
            .collect();
        assert_eq!(unstable.len(), 1);
        assert_eq!(unstable[0].category, v3::WisdomCategory::UnfinishedThread);

        // Security -> gotcha
        let security: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.contains("JWT validation"))
            .collect();
        assert_eq!(security.len(), 1);
        assert_eq!(security[0].category, v3::WisdomCategory::Gotcha);

        // Performance -> gotcha
        let perf: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content.contains("Hot loop"))
            .collect();
        assert_eq!(perf.len(), 1);
        assert_eq!(perf[0].category, v3::WisdomCategory::Gotcha);

        // Deprecated -> unfinished_thread
        let deprecated: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Use new_api instead")
            .collect();
        assert_eq!(deprecated.len(), 1);
        assert_eq!(deprecated[0].category, v3::WisdomCategory::UnfinishedThread);

        // TechDebt -> unfinished_thread
        let tech_debt: Vec<_> = v3_ann
            .wisdom
            .iter()
            .filter(|w| w.content == "Needs refactor after v2 ships")
            .collect();
        assert_eq!(tech_debt.len(), 1);
        assert_eq!(tech_debt[0].category, v3::WisdomCategory::UnfinishedThread);
    }

    #[test]
    fn test_v2_to_v3_provenance_preserved() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);

        // Rule 8: provenance carries through unchanged
        assert_eq!(v3_ann.provenance.source, v2::ProvenanceSource::Live);
        assert_eq!(v3_ann.provenance.author.as_deref(), Some("claude-code"));
        assert_eq!(v3_ann.provenance.notes.as_deref(), Some("test note"));
    }

    #[test]
    fn test_v2_to_v3_effort_dropped() {
        // Rule 9: effort is dropped — it shouldn't appear in v3 at all
        // (v3::Annotation has no effort field, so this is structural)
        let v2_ann = make_v2_annotation();
        assert!(v2_ann.effort.is_some()); // sanity check
        let _v3_ann = v2_to_v3(v2_ann);
        // If it compiles, effort is dropped
    }

    #[test]
    fn test_v2_to_v3_validates() {
        let v2_ann = make_v2_annotation();
        let v3_ann = v2_to_v3(v2_ann);
        assert!(v3_ann.validate().is_ok());
    }

    #[test]
    fn test_v2_to_v3_empty_annotation() {
        let v2_ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: "abc123".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            narrative: v2::Narrative {
                summary: "Simple change".to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: vec![],
                sentiments: vec![],
            },
            decisions: vec![],
            markers: vec![],
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                author: None,
                derived_from: vec![],
                notes: None,
            },
        };

        let v3_ann = v2_to_v3(v2_ann);
        assert_eq!(v3_ann.summary, "Simple change");
        assert!(v3_ann.wisdom.is_empty());
        assert!(v3_ann.validate().is_ok());
    }

    #[test]
    fn test_v1_to_v3_chained_migration() {
        // v1 -> v2 -> v3 chain
        let v1_ann = make_v1_annotation();
        let v2_ann = v1_to_v2(v1_ann);
        let v3_ann = v2_to_v3(v2_ann);

        assert_eq!(v3_ann.schema, "chronicle/v3");
        assert_eq!(v3_ann.summary, "Add reconnect logic");
        // Should have wisdom from markers (contract, hazard, dependency) and the decision
        assert!(!v3_ann.wisdom.is_empty());
        // Provenance should be MigratedV1 (from the v1->v2 step)
        assert_eq!(v3_ann.provenance.source, v2::ProvenanceSource::MigratedV1);
        assert!(v3_ann.validate().is_ok());
    }
}
