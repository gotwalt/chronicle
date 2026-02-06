use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::annotation::Annotation;

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
/// 2. For each commit, fetch annotation and filter to matching region
/// 3. Sort chronologically (oldest first)
/// 4. Optionally follow related_annotations
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

        let annotation: Annotation = match serde_json::from_str(&note) {
            Ok(a) => a,
            Err(_) => continue,
        };

        let commit_msg = git
            .commit_info(sha)
            .map(|ci| ci.message.clone())
            .unwrap_or_default();

        for region in &annotation.regions {
            if !file_matches(&region.file, &query.file) {
                continue;
            }
            if let Some(ref anchor_name) = query.anchor {
                if !anchor_matches(&region.ast_anchor.name, anchor_name) {
                    continue;
                }
            }

            let mut related_context = Vec::new();
            if query.follow_related {
                for rel in &region.related_annotations {
                    if let Ok(Some(rel_note)) = git.note_read(&rel.commit) {
                        if let Ok(rel_ann) = serde_json::from_str::<Annotation>(&rel_note) {
                            let rel_intent = rel_ann
                                .regions
                                .iter()
                                .find(|r| anchor_matches(&r.ast_anchor.name, &rel.anchor))
                                .map(|r| r.intent.clone());
                            related_context.push(RelatedContext {
                                commit: rel.commit.clone(),
                                anchor: rel.anchor.clone(),
                                relationship: rel.relationship.clone(),
                                intent: rel_intent,
                            });
                            related_followed += 1;
                        }
                    }
                }
            }

            let constraints: Vec<String> =
                region.constraints.iter().map(|c| c.text.clone()).collect();

            entries.push(TimelineEntry {
                commit: sha.clone(),
                timestamp: annotation.timestamp.clone(),
                commit_message: commit_msg.clone(),
                context_level: format!("{:?}", annotation.context_level).to_lowercase(),
                provenance: format!("{:?}", annotation.provenance.operation).to_lowercase(),
                intent: region.intent.clone(),
                reasoning: region.reasoning.clone(),
                constraints,
                risk_notes: region.risk_notes.clone(),
                related_context,
            });
        }
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
        schema: "ultragit-history/v1".to_string(),
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
    use crate::schema::annotation::*;

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
        fn file_at_commit(&self, _path: &std::path::Path, _commit: &str) -> Result<String, GitError> {
            Ok(String::new())
        }
        fn commit_info(&self, commit: &str) -> Result<crate::git::CommitInfo, GitError> {
            Ok(crate::git::CommitInfo {
                sha: commit.to_string(),
                message: self.commit_messages.get(commit).cloned().unwrap_or_default(),
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

    fn make_annotation(commit: &str, timestamp: &str, regions: Vec<RegionAnnotation>) -> Annotation {
        Annotation {
            schema: "ultragit/v1".to_string(),
            commit: commit.to_string(),
            timestamp: timestamp.to_string(),
            task: None,
            summary: "test".to_string(),
            context_level: ContextLevel::Enhanced,
            regions,
            cross_cutting: vec![],
            provenance: Provenance {
                operation: ProvenanceOperation::Initial,
                derived_from: vec![],
                original_annotations_preserved: false,
                synthesis_notes: None,
            },
        }
    }

    fn make_region(
        file: &str,
        anchor: &str,
        intent: &str,
        related: Vec<RelatedAnnotation>,
    ) -> RegionAnnotation {
        RegionAnnotation {
            file: file.to_string(),
            ast_anchor: AstAnchor {
                unit_type: "fn".to_string(),
                name: anchor.to_string(),
                signature: None,
            },
            lines: LineRange { start: 1, end: 10 },
            intent: intent.to_string(),
            reasoning: None,
            constraints: vec![],
            semantic_dependencies: vec![],
            related_annotations: related,
            tags: vec![],
            risk_notes: None,
        }
    }

    #[test]
    fn test_single_commit_history() {
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main", "entry point", vec![])],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());
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
        let ann1 = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v1 entry", vec![])],
        );
        let ann2 = make_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v2 entry", vec![])],
        );
        let ann3 = make_annotation(
            "commit3",
            "2025-01-03T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v3 entry", vec![])],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());
        notes.insert("commit3".to_string(), serde_json::to_string(&ann3).unwrap());

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
            anchor: Some("main".to_string()),
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
        let ann1 = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v1", vec![])],
        );
        let ann2 = make_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v2", vec![])],
        );
        let ann3 = make_annotation(
            "commit3",
            "2025-01-03T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v3", vec![])],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());
        notes.insert("commit3".to_string(), serde_json::to_string(&ann3).unwrap());

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
            anchor: Some("main".to_string()),
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
        let related_ann = make_annotation(
            "related_commit",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/tls.rs",
                "TlsSessionCache::new",
                "session cache init",
                vec![],
            )],
        );

        let main_ann = make_annotation(
            "commit1",
            "2025-01-02T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "entry point",
                vec![RelatedAnnotation {
                    commit: "related_commit".to_string(),
                    anchor: "TlsSessionCache::new".to_string(),
                    relationship: "depends on session cache".to_string(),
                }],
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&main_ann).unwrap());
        notes.insert(
            "related_commit".to_string(),
            serde_json::to_string(&related_ann).unwrap(),
        );

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
        assert_eq!(result.timeline[0].related_context[0].anchor, "TlsSessionCache::new");
        assert_eq!(
            result.timeline[0].related_context[0].intent,
            Some("session cache init".to_string())
        );
        assert_eq!(result.stats.related_followed, 1);
    }

    #[test]
    fn test_follow_related_disabled() {
        let main_ann = make_annotation(
            "commit1",
            "2025-01-02T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "entry point",
                vec![RelatedAnnotation {
                    commit: "related_commit".to_string(),
                    anchor: "TlsSessionCache::new".to_string(),
                    relationship: "depends on session cache".to_string(),
                }],
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&main_ann).unwrap());

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
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region("src/main.rs", "main", "v1", vec![])],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());
        // commit2 has no note

        let git = MockGitOps {
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
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
        assert_eq!(result.stats.commits_in_log, 2);
    }
}
