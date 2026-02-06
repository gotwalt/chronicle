use crate::error::GitError;
use crate::git::GitOps;
use crate::schema::annotation::{Annotation, LineRange};

/// Query parameters for a condensed summary.
#[derive(Debug, Clone)]
pub struct SummaryQuery {
    pub file: String,
    pub anchor: Option<String>,
}

/// A summary unit for one AST element.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryUnit {
    pub anchor: SummaryAnchor,
    pub lines: LineRange,
    pub intent: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
    pub last_modified: String,
}

/// Anchor information in a summary unit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryAnchor {
    #[serde(rename = "type")]
    pub unit_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Statistics about the summary query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryStats {
    pub regions_found: u32,
    pub commits_examined: u32,
}

/// Output of a summary query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryOutput {
    pub schema: String,
    pub query: QueryEcho,
    pub units: Vec<SummaryUnit>,
    pub stats: SummaryStats,
}

/// Echo of the query parameters in the output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryEcho {
    pub file: String,
    pub anchor: Option<String>,
}

/// Build a condensed summary for a file (or file+anchor).
///
/// 1. Get commits that touched the file via `log_for_file`
/// 2. For each commit, fetch annotation and filter to matching regions
/// 3. For each unique anchor, keep the most recent annotation
/// 4. Extract only intent, constraints, risk_notes
pub fn build_summary(git: &dyn GitOps, query: &SummaryQuery) -> Result<SummaryOutput, GitError> {
    let shas = git.log_for_file(&query.file)?;
    let commits_examined = shas.len() as u32;

    // Collect all matching regions with their timestamps.
    // Key: anchor name, Value: (timestamp, SummaryUnit)
    let mut best: std::collections::HashMap<String, (String, SummaryUnit)> =
        std::collections::HashMap::new();

    for sha in &shas {
        let note = match git.note_read(sha)? {
            Some(n) => n,
            None => continue,
        };

        let annotation: Annotation = match serde_json::from_str(&note) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for region in &annotation.regions {
            if !file_matches(&region.file, &query.file) {
                continue;
            }
            if let Some(ref anchor_name) = query.anchor {
                if !anchor_matches(&region.ast_anchor.name, anchor_name) {
                    continue;
                }
            }

            let key = region.ast_anchor.name.clone();
            let constraints: Vec<String> =
                region.constraints.iter().map(|c| c.text.clone()).collect();

            let unit = SummaryUnit {
                anchor: SummaryAnchor {
                    unit_type: region.ast_anchor.unit_type.clone(),
                    name: region.ast_anchor.name.clone(),
                    signature: region.ast_anchor.signature.clone(),
                },
                lines: region.lines,
                intent: region.intent.clone(),
                constraints,
                risk_notes: region.risk_notes.clone(),
                last_modified: annotation.timestamp.clone(),
            };

            // Keep the entry with the most recent (lexicographically largest) timestamp.
            // Since git log returns newest first, the first match per anchor wins.
            best.entry(key).or_insert((annotation.timestamp.clone(), unit));
        }
    }

    let mut units: Vec<SummaryUnit> = best.into_values().map(|(_, unit)| unit).collect();
    // Sort by line start for deterministic output
    units.sort_by_key(|u| u.lines.start);

    let regions_found = units.len() as u32;

    Ok(SummaryOutput {
        schema: "ultragit-summary/v1".to_string(),
        query: QueryEcho {
            file: query.file.clone(),
            anchor: query.anchor.clone(),
        },
        units,
        stats: SummaryStats {
            regions_found,
            commits_examined,
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
        fn commit_info(&self, _commit: &str) -> Result<crate::git::CommitInfo, GitError> {
            Ok(crate::git::CommitInfo {
                sha: "abc123".to_string(),
                message: "test".to_string(),
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
        unit_type: &str,
        lines: LineRange,
        intent: &str,
        constraints: Vec<Constraint>,
        risk_notes: Option<&str>,
    ) -> RegionAnnotation {
        RegionAnnotation {
            file: file.to_string(),
            ast_anchor: AstAnchor {
                unit_type: unit_type.to_string(),
                name: anchor.to_string(),
                signature: None,
            },
            lines,
            intent: intent.to_string(),
            reasoning: Some("detailed reasoning".to_string()),
            constraints,
            semantic_dependencies: vec![],
            related_annotations: vec![],
            tags: vec!["tag1".to_string()],
            risk_notes: risk_notes.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_summary_single_file() {
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                make_region(
                    "src/main.rs",
                    "main",
                    "fn",
                    LineRange { start: 1, end: 10 },
                    "entry point",
                    vec![Constraint {
                        text: "must not panic".to_string(),
                        source: ConstraintSource::Author,
                    }],
                    Some("error handling is fragile"),
                ),
                make_region(
                    "src/main.rs",
                    "helper",
                    "fn",
                    LineRange { start: 12, end: 20 },
                    "helper fn",
                    vec![],
                    None,
                ),
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert_eq!(result.units.len(), 2);

        // Sorted by line start
        assert_eq!(result.units[0].anchor.name, "main");
        assert_eq!(result.units[0].intent, "entry point");
        assert_eq!(result.units[0].constraints, vec!["must not panic"]);
        assert_eq!(
            result.units[0].risk_notes,
            Some("error handling is fragile".to_string())
        );

        assert_eq!(result.units[1].anchor.name, "helper");
        assert_eq!(result.units[1].intent, "helper fn");
        assert!(result.units[1].constraints.is_empty());
        assert!(result.units[1].risk_notes.is_none());
    }

    #[test]
    fn test_summary_keeps_most_recent() {
        let ann1 = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "old intent",
                vec![],
                None,
            )],
        );
        let ann2 = make_annotation(
            "commit2",
            "2025-01-02T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "new intent",
                vec![],
                None,
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann1).unwrap());
        notes.insert("commit2".to_string(), serde_json::to_string(&ann2).unwrap());

        let git = MockGitOps {
            // newest first (as git log returns)
            file_log: vec!["commit2".to_string(), "commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].intent, "new intent");
    }

    #[test]
    fn test_summary_only_intent_constraints_risk() {
        // Verify that reasoning and tags don't appear in the output
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![make_region(
                "src/main.rs",
                "main",
                "fn",
                LineRange { start: 1, end: 10 },
                "entry point",
                vec![],
                None,
            )],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        let json = serde_json::to_string(&result).unwrap();
        // Should not contain "reasoning" or "tags" fields
        assert!(!json.contains("\"reasoning\""));
        assert!(!json.contains("\"tags\""));
    }

    #[test]
    fn test_summary_empty_when_no_annotations() {
        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes: std::collections::HashMap::new(),
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: None,
        };

        let result = build_summary(&git, &query).unwrap();
        assert!(result.units.is_empty());
        assert_eq!(result.stats.regions_found, 0);
    }

    #[test]
    fn test_summary_with_anchor_filter() {
        let ann = make_annotation(
            "commit1",
            "2025-01-01T00:00:00Z",
            vec![
                make_region(
                    "src/main.rs",
                    "main",
                    "fn",
                    LineRange { start: 1, end: 10 },
                    "entry point",
                    vec![],
                    None,
                ),
                make_region(
                    "src/main.rs",
                    "helper",
                    "fn",
                    LineRange { start: 12, end: 20 },
                    "helper fn",
                    vec![],
                    None,
                ),
            ],
        );

        let mut notes = std::collections::HashMap::new();
        notes.insert("commit1".to_string(), serde_json::to_string(&ann).unwrap());

        let git = MockGitOps {
            file_log: vec!["commit1".to_string()],
            notes,
        };

        let query = SummaryQuery {
            file: "src/main.rs".to_string(),
            anchor: Some("main".to_string()),
        };

        let result = build_summary(&git, &query).unwrap();
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].anchor.name, "main");
    }
}
