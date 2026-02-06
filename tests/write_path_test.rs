use std::path::Path;
use std::process::Command;

use chronicle::git::{CliOps, GitOps};
use chronicle::mcp::annotate_handler::{
    AnchorInput, AnchorResolutionKind, AnnotateInput, ConstraintInput, RegionInput,
    handle_annotate,
};
use chronicle::schema::{
    Annotation, ConstraintSource, ContextLevel, CrossCuttingConcern, CrossCuttingRegionRef,
    LineRange, ProvenanceOperation, SemanticDependency,
};

const SAMPLE_RUST_FILE: &str = r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

pub struct Greeter {
    prefix: String,
}

impl Greeter {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    pub fn greet(&self, name: &str) -> String {
        format!("{} {}!", self.prefix, name)
    }
}
"#;

fn create_temp_repo() -> (tempfile::TempDir, CliOps) {
    let dir = tempfile::tempdir().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let ops = CliOps::new(dir.path().to_path_buf());
    (dir, ops)
}

fn add_and_commit(dir: &Path, filename: &str, content: &str, message: &str) {
    if let Some(parent) = Path::new(filename).parent() {
        std::fs::create_dir_all(dir.join(parent)).unwrap();
    }
    std::fs::write(dir.join(filename), content).unwrap();
    Command::new("git")
        .args(["add", filename])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(dir)
        .output()
        .unwrap();
}

fn make_basic_input(commit: &str) -> AnnotateInput {
    AnnotateInput {
        commit: commit.to_string(),
        summary: "Add greet function and Greeter struct for customizable greetings".to_string(),
        task: Some("TASK-42".to_string()),
        regions: vec![
            RegionInput {
                file: "src/lib.rs".to_string(),
                anchor: Some(AnchorInput {
                    unit_type: "function".to_string(),
                    name: "greet".to_string(),
                }),
                lines: LineRange { start: 1, end: 3 },
                intent: "Add a standalone greet function for simple greeting use cases".to_string(),
                reasoning: Some("Needed a simple entry point before the full Greeter struct".to_string()),
                constraints: vec![ConstraintInput {
                    text: "Must return an owned String, not a reference".to_string(),
                }],
                semantic_dependencies: vec![],
                tags: vec!["greeting".to_string()],
                risk_notes: None,
            },
            RegionInput {
                file: "src/lib.rs".to_string(),
                anchor: Some(AnchorInput {
                    unit_type: "method".to_string(),
                    name: "Greeter::new".to_string(),
                }),
                lines: LineRange { start: 10, end: 12 },
                intent: "Constructor for Greeter with configurable prefix".to_string(),
                reasoning: None,
                constraints: vec![],
                semantic_dependencies: vec![SemanticDependency {
                    file: "src/lib.rs".to_string(),
                    anchor: "Greeter".to_string(),
                    nature: "Constructs Greeter instances".to_string(),
                }],
                tags: vec!["greeting".to_string(), "constructor".to_string()],
                risk_notes: None,
            },
        ],
        cross_cutting: vec![CrossCuttingConcern {
            description: "Greeting formatting is consistent across standalone function and struct method".to_string(),
            regions: vec![
                CrossCuttingRegionRef {
                    file: "src/lib.rs".to_string(),
                    anchor: "greet".to_string(),
                },
                CrossCuttingRegionRef {
                    file: "src/lib.rs".to_string(),
                    anchor: "Greeter::greet".to_string(),
                },
            ],
            tags: vec!["consistency".to_string()],
        }],
    }
}

#[test]
fn full_roundtrip_annotate_and_read_note() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "src/lib.rs", SAMPLE_RUST_FILE, "Add greeting module");

    let input = make_basic_input("HEAD");
    let result = handle_annotate(&ops, input).unwrap();

    assert!(result.success);
    assert_eq!(result.regions_written, 2);
    assert_eq!(result.commit.len(), 40);

    let note_json = ops.note_read(&result.commit).unwrap().unwrap();
    let annotation: Annotation = serde_json::from_str(&note_json).unwrap();

    assert_eq!(annotation.schema, "chronicle/v1");
    assert_eq!(annotation.commit, result.commit);
    assert_eq!(annotation.context_level, ContextLevel::Enhanced);
    assert_eq!(annotation.task, Some("TASK-42".to_string()));
    assert!(annotation.summary.contains("greet"));
    assert_eq!(annotation.regions.len(), 2);

    let region0 = &annotation.regions[0];
    assert_eq!(region0.file, "src/lib.rs");
    assert_eq!(region0.ast_anchor.name, "greet");
    assert_eq!(region0.ast_anchor.unit_type, "function");
    assert!(region0.ast_anchor.signature.is_some());
    assert!(region0.intent.contains("standalone greet"));
    assert_eq!(region0.tags, vec!["greeting"]);

    let region1 = &annotation.regions[1];
    assert_eq!(region1.ast_anchor.name, "Greeter::new");
    assert_eq!(region1.ast_anchor.unit_type, "method");

    assert_eq!(region0.constraints.len(), 1);
    assert_eq!(region0.constraints[0].source, ConstraintSource::Author);

    assert_eq!(annotation.cross_cutting.len(), 1);
    assert_eq!(annotation.provenance.operation, ProvenanceOperation::Initial);
}

#[test]
fn anchor_resolution_corrects_line_ranges() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "src/lib.rs", SAMPLE_RUST_FILE, "Add greeting module");

    let input = AnnotateInput {
        commit: "HEAD".to_string(),
        summary: "Test that AST corrects line ranges from agent input".to_string(),
        task: None,
        regions: vec![RegionInput {
            file: "src/lib.rs".to_string(),
            anchor: Some(AnchorInput {
                unit_type: "function".to_string(),
                name: "greet".to_string(),
            }),
            lines: LineRange { start: 99, end: 100 },
            intent: "The AST should correct these line numbers".to_string(),
            reasoning: None,
            constraints: vec![],
            semantic_dependencies: vec![],
            tags: vec![],
            risk_notes: None,
        }],
        cross_cutting: vec![],
    };

    let result = handle_annotate(&ops, input).unwrap();
    assert!(matches!(result.anchor_resolutions[0].resolution, AnchorResolutionKind::Exact));

    let note_json = ops.note_read(&result.commit).unwrap().unwrap();
    let annotation: Annotation = serde_json::from_str(&note_json).unwrap();
    let region = &annotation.regions[0];

    assert!(region.lines.start < 10);
    assert!(region.lines.end < 10);
    assert!(region.lines.end >= region.lines.start);
    assert!(region.ast_anchor.signature.is_some());
}

#[test]
fn validation_rejects_empty_summary() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "src/lib.rs", SAMPLE_RUST_FILE, "Init");

    let input = AnnotateInput {
        commit: "HEAD".to_string(),
        summary: "".to_string(),
        task: None,
        regions: vec![],
        cross_cutting: vec![],
    };

    let result = handle_annotate(&ops, input);
    assert!(result.is_err());

    let sha = ops.resolve_ref("HEAD").unwrap();
    assert!(ops.note_read(&sha).unwrap().is_none());
}

#[test]
fn multiple_commits_have_independent_notes() {
    let (dir, ops) = create_temp_repo();

    add_and_commit(dir.path(), "src/lib.rs", SAMPLE_RUST_FILE, "Add greeting module");
    let sha1 = ops.resolve_ref("HEAD").unwrap();

    let input1 = AnnotateInput {
        commit: sha1.clone(),
        summary: "First commit: add greeting module".to_string(),
        task: None,
        regions: vec![RegionInput {
            file: "src/lib.rs".to_string(),
            anchor: Some(AnchorInput { unit_type: "function".to_string(), name: "greet".to_string() }),
            lines: LineRange { start: 1, end: 3 },
            intent: "Add standalone greet function".to_string(),
            reasoning: None, constraints: vec![], semantic_dependencies: vec![], tags: vec![], risk_notes: None,
        }],
        cross_cutting: vec![],
    };
    handle_annotate(&ops, input1).unwrap();

    let updated = SAMPLE_RUST_FILE.to_string() + "\npub fn farewell() -> String {\n    \"Goodbye!\".to_string()\n}\n";
    add_and_commit(dir.path(), "src/lib.rs", &updated, "Add farewell function");
    let sha2 = ops.resolve_ref("HEAD").unwrap();

    let input2 = AnnotateInput {
        commit: sha2.clone(),
        summary: "Second commit: add farewell function".to_string(),
        task: None,
        regions: vec![RegionInput {
            file: "src/lib.rs".to_string(),
            anchor: Some(AnchorInput { unit_type: "function".to_string(), name: "farewell".to_string() }),
            lines: LineRange { start: 19, end: 21 },
            intent: "Add farewell function as counterpart to greet".to_string(),
            reasoning: None, constraints: vec![], semantic_dependencies: vec![], tags: vec![], risk_notes: None,
        }],
        cross_cutting: vec![],
    };
    handle_annotate(&ops, input2).unwrap();

    let note1_json = ops.note_read(&sha1).unwrap().unwrap();
    let note2_json = ops.note_read(&sha2).unwrap().unwrap();
    let ann1: Annotation = serde_json::from_str(&note1_json).unwrap();
    let ann2: Annotation = serde_json::from_str(&note2_json).unwrap();

    assert_eq!(ann1.commit, sha1);
    assert_eq!(ann2.commit, sha2);
    assert!(ann1.summary.contains("First"));
    assert!(ann2.summary.contains("Second"));
    assert_ne!(ann1.commit, ann2.commit);
}

#[test]
fn quality_warnings_do_not_block_write() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "src/lib.rs", SAMPLE_RUST_FILE, "Init");

    let input = AnnotateInput {
        commit: "HEAD".to_string(),
        summary: "short".to_string(),
        task: None,
        regions: vec![RegionInput {
            file: "src/lib.rs".to_string(),
            anchor: Some(AnchorInput { unit_type: "function".to_string(), name: "greet".to_string() }),
            lines: LineRange { start: 1, end: 3 },
            intent: "short".to_string(),
            reasoning: None, constraints: vec![], semantic_dependencies: vec![], tags: vec![], risk_notes: None,
        }],
        cross_cutting: vec![],
    };

    let result = handle_annotate(&ops, input).unwrap();
    assert!(result.success);
    assert!(!result.warnings.is_empty());

    let note = ops.note_read(&result.commit).unwrap();
    assert!(note.is_some());
}
