use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::{self, v2};

/// Query parameters for timeline reconstruction.
#[derive(Debug, Clone)]
pub struct HistoryQuery {
    pub file: String,
    pub anchor: Option<String>,
    pub limit: u32,
    pub follow_related: bool,
}

/// A single timeline entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineEntry {
    pub commit: String,
    pub timestamp: String,
    pub commit_message: String,
    pub context_level: String,
    pub provenance: String,
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_context: Vec<RelatedContext>,
}

/// Related annotation context included in timeline entries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelatedContext {
    pub commit: String,
    pub anchor: String,
    pub relationship: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

/// Statistics about the history query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryStats {
    pub commits_in_log: u32,
    pub annotations_found: u32,
    pub related_followed: u32,
}

/// Output of a history query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryOutput {
    pub schema: String,
    pub query: QueryEcho,
    pub timeline: Vec<TimelineEntry>,
    pub stats: HistoryStats,
}

/// Echo of the query parameters in the output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryEcho {
    pub file: String,
    pub anchor: Option<String>,
}

/// Reconstruct the annotation timeline for a file+anchor across commits.
///
/// 1. Get commits that touched the file via `log_for_file`
/// 2. For each commit, fetch annotation and check relevance
/// 3. Sort chronologically (oldest first)
/// 4. Optionally follow related dependencies
/// 5. Apply limit
pub fn build_timeline(git: &dyn GitOps, query: &HistoryQuery) -> Result<HistoryOutput, GitError> {
    let shas = git.log_for_file(&query.file)?;
    let commits_in_log = shas.len() as u32;

    let mut entries: Vec<TimelineEntry> = Vec::new();
    let mut related_followed: u32 = 0;

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation = match schema::parse_annotation(&note) {
            Ok(a) => a,
            Err(_) => continue,
        };

        let commit_msg = git
            .commit_info(sha)
            .map(|ci| ci.message.clone())
            .unwrap_or_default();

        // Check if this annotation is relevant to the queried file
        let file_in_files_changed = annotation
            .narrative
            .files_changed
            .iter()
            .any(|f| file_matches(f, &query.file));
        let file_in_markers = annotation
            .markers
            .iter()
            .any(|m| file_matches(&m.file, &query.file));

        if !file_in_files_changed && !file_in_markers {
            continue;
        }

        // If anchor is specified, check if any marker has matching anchor
        if let Some(ref anchor_name) = query.anchor {
            let has_matching_anchor = annotation.markers.iter().any(|m| {
                file_matches(&m.file, &query.file)
                    && m.anchor
                        .as_ref()
                        .map(|a| anchor_matches(&a.name, anchor_name))
                        .unwrap_or(false)
            });
            if !has_matching_anchor && !file_in_files_changed {
                continue;
            }
        }

        // Extract constraints from Contract markers matching the file
        let constraints: Vec<String> = annotation
            .markers
            .iter()
            .filter(|m| file_matches(&m.file, &query.file))
            .filter(|m| {
                query.anchor.as_ref().is_none_or(|qa| {
                    m.anchor
                        .as_ref()
                        .is_some_and(|a| anchor_matches(&a.name, qa))
                })
            })
            .filter_map(|m| {
                if let v2::MarkerKind::Contract { description, .. } = &m.kind {
                    Some(description.clone())
                } else {
                    None
                }
            })
            .collect();

        // Extract risk notes from Hazard markers matching the file
        let risk_notes: Option<String> = {
            let hazards: Vec<String> = annotation
                .markers
                .iter()
                .filter(|m| file_matches(&m.file, &query.file))
                .filter(|m| {
                    query.anchor.as_ref().is_none_or(|qa| {
                        m.anchor
                            .as_ref()
                            .is_some_and(|a| anchor_matches(&a.name, qa))
                    })
                })
                .filter_map(|m| {
                    if let v2::MarkerKind::Hazard { description } = &m.kind {
                        Some(description.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if hazards.is_empty() {
                None
            } else {
                Some(hazards.join("; "))
            }
        };

        // Follow related: derive from Dependency markers
        let mut related_context = Vec::new();
        if query.follow_related {
            for marker in &annotation.markers {
                if let v2::MarkerKind::Dependency {
                    target_file,
                    target_anchor,
                    assumption,
                } = &marker.kind
                {
                    // Try to read the target commit's annotation for intent
                    let rel_intent = if let Ok(Some(rel_note)) = git.note_read(sha) {
                        if let Ok(rel_ann) = schema::parse_annotation(&rel_note) {
                            Some(rel_ann.narrative.summary.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    related_context.push(RelatedContext {
                        commit: sha.clone(),
                        anchor: format!("{}:{}", target_file, target_anchor),
                        relationship: assumption.clone(),
                        intent: rel_intent,
                    });
                    related_followed += 1;
                }
            }
        }

        let context_level = format!("{:?}", annotation.provenance.source).to_lowercase();

        entries.push(TimelineEntry {
            commit: sha.clone(),
            timestamp: annotation.timestamp.clone(),
            commit_message: commit_msg,
            context_level: context_level.clone(),
            provenance: context_level,
            intent: annotation.narrative.summary.clone(),
            reasoning: annotation.narrative.motivation.clone(),
            constraints,
            risk_notes,
            related_context,
        });
    }

    // Sort chronologically (oldest first). git log returns newest first, so reverse.
    entries.reverse();

    let annotations_found = entries.len() as u32;

    // Apply limit — keep the N most recent (from the end)
    if entries.len() > query.limit as usize {
        let start = entries.len() - query.limit as usize;
        entries = entries.split_off(start);
    }

    Ok(HistoryOutput {
        schema: "chronicle-history/v1".to_string(),
        query: QueryEcho {
            file: query.file.clone(),
            anchor: query.anchor.clone(),
        },
        timeline: entries,
        stats: HistoryStats {
            commits_in_log,
            annotations_found,
            related_followed,
        },
    })
}

fn file_matches(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> &str {
        s.strip_prefix("./").unwrap_or(s)
    }
    norm(a) == norm(b)
}

fn anchor_matches(region_anchor: &str, query_anchor: &str) -> bool {
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
    use crate::schema::common::AstAnchor;

    struct MockGitOps {
        file_log: Vec<String>,
        notes: std::collections::HashMap<String, String>,
        commit_messages: std::collections::HashMap<String, String>,
    }

    impl GitOps for MockGitOps {
        fn diff(&self, _commit: &str) -> Result<Vec<crate::git::FileDiff>, GitError> {
            Ok(vec![])
        }
        fn note_read(&self, commit: &str) -> Result<Option<String>, GitError> {
            Ok(self.notes.get(commit).cloned())
        }
        fn note_write(&self, _commit: &str, _content: &str) -> Result<(), GitError> {
            Ok(())
        }
        fn note_exists(&self, commit: &str) -> Result<bool, GitError> {
            Ok(self.notes.contains_key(commit))
        }
        fn file_at_commit(
            &self,
            _path: &std::path::Path,
            _commit: &str,
        ) -> Result<String, GitError> {
            Ok(String::new())
        }
        fn commit_info(&self, commit: &str) -> Result<crate::git::CommitInfo, GitError> {
            Ok(crate::git::CommitInfo {
                sha: commit.to_string(),
                message: self
                    .commit_messages
                    .get(commit)
                    .cloned()
                    .unwrap_or_default(),
                author_name: "test".to_string(),
                author_email: "test@test.com".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                parent_shas: vec![],
            })
        }
        fn resolve_ref(&self, _refspec: &str) -> Result<String, GitError> {
            Ok("abc123".to_string())
        }
        fn config_get(&self, _key: &str) -> Result<Option<String>, GitError> {
            Ok(None)
        }
        fn config_set(&self, _key: &str, _value: &str) -> Result<(), GitError> {
            Ok(())
        }
        fn log_for_file(&self, _path: &str) -> Result<Vec<String>, GitError> {
            Ok(self.file_log.clone())
        }
        fn list_annotated_commits(&self, _limit: u32) -> Result<Vec<String>, GitError> {
            Ok(vec![])
        }
    }

    fn make_v2_annotation_with_intent(
        commit: &str,
        timestamp: &str,
        summary: &str,
        files_changed: Vec<&str>,
        markers: Vec<v2::CodeMarker>,
    ) -> String {
        let ann = v2::Annotation {
            schema: "chronicle/v2".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            narrative: v2::Narrative {
                summary: summary.to_string(),
                motivation: None,
                rejected_alternatives: vec![],
                follow_up: None,
                files_changed: files_changed.into_iter().map(|s| s.to_string()).collect(),
            },
            decisions: vec![],
            markers,
            effort: None,
            provenance: v2::Provenance {
                source: v2::ProvenanceSource::Live,
                derived_from: vec![],
                notes: None,
            },
        };
        serde_json::to_string(&ann).unwrap()
    }

    fn make_contract_marker(file: &str, anchor: &str, description: &str) -> v2::CodeMarker {
        v2::CodeMarker {
            file: file.to_string(),
            anchor: Some(AstAnchor {
                unit_type: "fn".to_string(),
                name: anchor.to_string(),
                signature: None,
            }),
            lines: None,
            kind: v2::MarkerKind::Contract {
                description: description.to_string(),
                source: v2::ContractSource::Author,
            },
        }
    }

    fn make_dep_marker(
        file: &str,
        anchor: &str,
        target_file: &str,
        target_anchor: &str,
        assumption: &str,
    ) -> v2::CodeMarker {
        v2::CodeMarker {
            file: file.to_string(),
            anchor: Some(AstAnchor {
                unit_type: "fn".to_string(),
                name: anchor.to_string(),
                signature: None,
            }),
            lines: None,
            kind: v2::MarkerKind::Dependency {
                target_file: target_file.to_string(),
                target_anchor: target_anchor.to_string(),
                assumption: assumption.to_string(),
            },
        }
    }

    #[test]
    fn test_single_commit_history() {
        let note = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-01T00:00:00Z",
            "entry point",
            vec!["src/main.rs"],
            vec![make_contract_marker("src/main.rs", "main", "must not panic")],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);
        let mut msgs = std::collections::HashMap::new();
        msgs.insert("commit1".to_string(), "initial commit".to_string());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
            commit_messages: msgs,
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
            limit: 10,
            follow_related: true,
        };

        let result = build_timeline(&git, &query).unwrap();
        assert_eq!(result.timeline.len(), 1);
        assert_eq!(result.timeline[0].intent, "entry point");
        assert_eq!(result.timeline[0].commit_message, "initial commit");
    }

    #[test]
    fn test_multi_commit_chronological_order() {
        let note1 = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-01T00:00:00Z",
            "v1 entry",
            vec!["src/main.rs"],
            vec![],
        );
        let note2 = make_v2_annotation_with_intent(
            "commit2",
            "2025-01-02T00:00:00Z",
            "v2 entry",
            vec!["src/main.rs"],
            vec![],
        );
        let note3 = make_v2_annotation_with_intent(
            "commit3",
            "2025-01-03T00:00:00Z",
            "v3 entry",
            vec!["src/main.rs"],
            vec![],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);
        notes.insert("commit3".to_string(), note3);

        let git = MockGitOps {
            // git log returns newest first
            file_log: vec![
                "commit3".to_string(),
                "commit2".to_string(),
                "commit1".to_string(),
            ],
            notes,
            commit_messages: std::collections::HashMap::new(),
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            limit: 10,
            follow_related: false,
        };

        let result = build_timeline(&git, &query).unwrap();
        assert_eq!(result.timeline.len(), 3);
        // Oldest first
        assert_eq!(result.timeline[0].intent, "v1 entry");
        assert_eq!(result.timeline[1].intent, "v2 entry");
        assert_eq!(result.timeline[2].intent, "v3 entry");
    }

    #[test]
    fn test_limit_respected() {
        let note1 = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-01T00:00:00Z",
            "v1",
            vec!["src/main.rs"],
            vec![],
        );
        let note2 = make_v2_annotation_with_intent(
            "commit2",
            "2025-01-02T00:00:00Z",
            "v2",
            vec!["src/main.rs"],
            vec![],
        );
        let note3 = make_v2_annotation_with_intent(
            "commit3",
            "2025-01-03T00:00:00Z",
            "v3",
            vec!["src/main.rs"],
            vec![],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note1);
        notes.insert("commit2".to_string(), note2);
        notes.insert("commit3".to_string(), note3);

        let git = MockGitOps {
            file_log: vec![
                "commit3".to_string(),
                "commit2".to_string(),
                "commit1".to_string(),
            ],
            notes,
            commit_messages: std::collections::HashMap::new(),
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            limit: 2,
            follow_related: false,
        };

        let result = build_timeline(&git, &query).unwrap();
        // Should return 2 most recent
        assert_eq!(result.timeline.len(), 2);
        assert_eq!(result.timeline[0].intent, "v2");
        assert_eq!(result.timeline[1].intent, "v3");
        assert_eq!(result.stats.annotations_found, 3);
    }

    #[test]
    fn test_follow_related() {
        let note = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-02T00:00:00Z",
            "entry point",
            vec!["src/main.rs"],
            vec![make_dep_marker(
                "src/main.rs",
                "main",
                "src/tls.rs",
                "TlsSessionCache::new",
                "depends on session cache",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
            commit_messages: std::collections::HashMap::new(),
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
            limit: 10,
            follow_related: true,
        };

        let result = build_timeline(&git, &query).unwrap();
        assert_eq!(result.timeline.len(), 1);
        assert_eq!(result.timeline[0].related_context.len(), 1);
        assert_eq!(
            result.timeline[0].related_context[0].anchor,
            "src/tls.rs:TlsSessionCache::new"
        );
        assert_eq!(
            result.timeline[0].related_context[0].relationship,
            "depends on session cache"
        );
        assert_eq!(result.stats.related_followed, 1);
    }

    #[test]
    fn test_follow_related_disabled() {
        let note = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-02T00:00:00Z",
            "entry point",
            vec!["src/main.rs"],
            vec![make_dep_marker(
                "src/main.rs",
                "main",
                "src/tls.rs",
                "TlsSessionCache::new",
                "depends on session cache",
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
            commit_messages: std::collections::HashMap::new(),
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
            limit: 10,
            follow_related: false,
        };

        let result = build_timeline(&git, &query).unwrap();
        assert_eq!(result.timeline.len(), 1);
        assert!(result.timeline[0].related_context.is_empty());
        assert_eq!(result.stats.related_followed, 0);
    }

    #[test]
    fn test_commit_without_annotation_skipped() {
        let note = make_v2_annotation_with_intent(
            "commit1",
            "2025-01-01T00:00:00Z",
            "v1",
            vec!["src/main.rs"],
            vec![],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), note);
        // commit2 has no note

        let git = MockGitOps {
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
            commit_messages: std::collections::HashMap::new(),
        };

        let query = HistoryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
            limit: 10,
            follow_related: false,
        };

        let result = build_timeline(&git, &query).unwrap();
        assert_eq!(result.timeline.len(), 1);
        assert_eq!(result.stats.commits_in_log, 2);
    }
}
