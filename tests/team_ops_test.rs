use std::io::{BufReader, Cursor};
use std::path::Path;
use std::process::Command;

use chronicle::doctor::{run_doctor, DoctorStatus};
use chronicle::export::export_annotations;
use chronicle::git::{CliOps, GitOps};
use chronicle::import::import_annotations;
use chronicle::schema::annotation::{
    Annotation, AstAnchor, ContextLevel, CrossCuttingConcern, CrossCuttingRegionRef, LineRange,
    Provenance, ProvenanceOperation, RegionAnnotation,
};
use chronicle::sync::{enable_sync, get_sync_config, get_sync_status};

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

fn make_test_annotation(commit: &str) -> Annotation {
    Annotation {
        schema: "chronicle/v1".to_string(),
        commit: commit.to_string(),
        timestamp: "2025-12-15T10:30:00Z".to_string(),
        task: Some("TEST-1".to_string()),
        summary: "Test annotation".to_string(),
        context_level: ContextLevel::Inferred,
        regions: vec![RegionAnnotation {
            file: "hello.txt".to_string(),
            ast_anchor: AstAnchor {
                unit_type: "file".to_string(),
                name: "hello.txt".to_string(),
                signature: None,
            },
            lines: LineRange { start: 1, end: 1 },
            intent: "Test change".to_string(),
            reasoning: None,
            constraints: Vec::new(),
            semantic_dependencies: Vec::new(),
            related_annotations: Vec::new(),
            tags: Vec::new(),
            risk_notes: None,
            corrections: vec![],
        }],
        cross_cutting: vec![CrossCuttingConcern {
            description: "test concern".to_string(),
            regions: vec![CrossCuttingRegionRef {
                file: "hello.txt".to_string(),
                anchor: "hello.txt".to_string(),
            }],
            tags: Vec::new(),
        }],
        provenance: Provenance {
            operation: ProvenanceOperation::Initial,
            derived_from: Vec::new(),
            original_annotations_preserved: false,
            synthesis_notes: None,
        },
    }
}

// ---- Sync tests ----

#[test]
fn sync_enable_adds_refspecs() {
    let (dir, _ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    // Add a remote first
    Command::new("git")
        .args(["remote", "add", "origin", "https://example.com/repo.git"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let repo_dir = dir.path().to_path_buf();

    // Before enable
    let config = get_sync_config(&repo_dir, "origin").unwrap();
    assert!(!config.is_enabled());

    // Enable sync
    enable_sync(&repo_dir, "origin").unwrap();

    // After enable
    let config = get_sync_config(&repo_dir, "origin").unwrap();
    assert!(config.is_enabled());
    assert!(config
        .push_refspec
        .unwrap()
        .contains("refs/notes/chronicle"));
    assert!(config
        .fetch_refspec
        .unwrap()
        .contains("refs/notes/chronicle"));
}

#[test]
fn sync_enable_is_idempotent() {
    let (dir, _ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    Command::new("git")
        .args(["remote", "add", "origin", "https://example.com/repo.git"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let repo_dir = dir.path().to_path_buf();

    enable_sync(&repo_dir, "origin").unwrap();
    enable_sync(&repo_dir, "origin").unwrap(); // should not error

    let config = get_sync_config(&repo_dir, "origin").unwrap();
    assert!(config.is_enabled());
}

#[test]
fn sync_status_reports_zero_notes() {
    let (dir, _ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    Command::new("git")
        .args(["remote", "add", "origin", "https://example.com/repo.git"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let repo_dir = dir.path().to_path_buf();
    let status = get_sync_status(&repo_dir, "origin").unwrap();
    assert_eq!(status.local_count, 0);
}

// ---- Export/Import roundtrip tests ----

#[test]
fn export_empty_repo_produces_no_output() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let mut output = Vec::new();
    let count = export_annotations(&ops, &mut output).unwrap();
    assert_eq!(count, 0);
    assert!(output.is_empty());
}

#[test]
fn export_import_roundtrip() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    // Write a test annotation as a note
    let annotation = make_test_annotation(&sha);
    let content = serde_json::to_string(&annotation).unwrap();
    ops.note_write(&sha, &content).unwrap();

    // Export
    // We need to construct the JSONL manually since list_annotated_commits
    // shells out to git directly from the cwd (not the test repo).
    let entry = chronicle::export::ExportEntry {
        commit_sha: sha.clone(),
        timestamp: annotation.timestamp.clone(),
        annotation: annotation.clone(),
    };
    let line = serde_json::to_string(&entry).unwrap();
    let export_buf = format!("{line}\n").into_bytes();

    // Import into a second repo that shares the same commit
    // (For simplicity, use the same repo and remove the note first)
    // Remove existing note
    Command::new("git")
        .args(["notes", "--ref", "refs/notes/chronicle", "remove", &sha])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!ops.note_exists(&sha).unwrap());

    // Import
    let reader = BufReader::new(Cursor::new(&export_buf));
    let summary = import_annotations(&ops, reader, false, false).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.skipped_existing, 0);
    assert_eq!(summary.skipped_not_found, 0);
    assert_eq!(summary.skipped_invalid, 0);

    // Verify the note was written back
    let note = ops.note_read(&sha).unwrap().unwrap();
    let reimported: Annotation = serde_json::from_str(&note).unwrap();
    assert_eq!(reimported.commit, sha);
    assert_eq!(reimported.summary, "Test annotation");
    assert_eq!(reimported.regions.len(), 1);
    assert_eq!(reimported.cross_cutting.len(), 1);
}

#[test]
fn import_skips_existing_notes_without_force() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    // Write a note
    let annotation = make_test_annotation(&sha);
    let content = serde_json::to_string(&annotation).unwrap();
    ops.note_write(&sha, &content).unwrap();

    // Build JSONL
    let entry = chronicle::export::ExportEntry {
        commit_sha: sha.clone(),
        timestamp: annotation.timestamp.clone(),
        annotation,
    };
    let line = serde_json::to_string(&entry).unwrap();
    let data = format!("{line}\n");

    // Import without force — should skip
    let reader = BufReader::new(Cursor::new(data.as_bytes()));
    let summary = import_annotations(&ops, reader, false, false).unwrap();
    assert_eq!(summary.imported, 0);
    assert_eq!(summary.skipped_existing, 1);
}

#[test]
fn import_with_force_overwrites() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    // Write an initial note
    ops.note_write(&sha, r#"{"old":"data"}"#).unwrap();

    // Build JSONL with a proper annotation
    let annotation = make_test_annotation(&sha);
    let entry = chronicle::export::ExportEntry {
        commit_sha: sha.clone(),
        timestamp: annotation.timestamp.clone(),
        annotation,
    };
    let line = serde_json::to_string(&entry).unwrap();
    let data = format!("{line}\n");

    // Import with force
    let reader = BufReader::new(Cursor::new(data.as_bytes()));
    let summary = import_annotations(&ops, reader, true, false).unwrap();
    assert_eq!(summary.imported, 1);

    // Verify overwritten
    let note = ops.note_read(&sha).unwrap().unwrap();
    let ann: Annotation = serde_json::from_str(&note).unwrap();
    assert_eq!(ann.summary, "Test annotation");
}

#[test]
fn import_skips_unknown_commits() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");

    // Build JSONL with a non-existent SHA
    let annotation = make_test_annotation("0000000000000000000000000000000000000000");
    let entry = chronicle::export::ExportEntry {
        commit_sha: "0000000000000000000000000000000000000000".to_string(),
        timestamp: annotation.timestamp.clone(),
        annotation,
    };
    let line = serde_json::to_string(&entry).unwrap();
    let data = format!("{line}\n");

    let reader = BufReader::new(Cursor::new(data.as_bytes()));
    let summary = import_annotations(&ops, reader, false, false).unwrap();
    assert_eq!(summary.imported, 0);
    assert_eq!(summary.skipped_not_found, 1);
}

#[test]
fn import_skips_invalid_json() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");

    let data = "this is not json\n{\"also\":\"invalid\"}\n";

    let reader = BufReader::new(Cursor::new(data.as_bytes()));
    let summary = import_annotations(&ops, reader, false, false).unwrap();
    assert_eq!(summary.imported, 0);
    assert_eq!(summary.skipped_invalid, 2);
}

#[test]
fn import_dry_run_does_not_write() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    let annotation = make_test_annotation(&sha);
    let entry = chronicle::export::ExportEntry {
        commit_sha: sha.clone(),
        timestamp: annotation.timestamp.clone(),
        annotation,
    };
    let line = serde_json::to_string(&entry).unwrap();
    let data = format!("{line}\n");

    let reader = BufReader::new(Cursor::new(data.as_bytes()));
    let summary = import_annotations(&ops, reader, false, true).unwrap();
    assert_eq!(summary.imported, 1); // counted as "would import"

    // But no actual note was written
    assert!(!ops.note_exists(&sha).unwrap());
}

// ---- Doctor tests ----

#[test]
fn doctor_on_fresh_repo() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    // Should have checks
    assert!(!report.checks.is_empty());

    // Version check should always pass
    let version_check = report.checks.iter().find(|c| c.name == "version").unwrap();
    assert_eq!(version_check.status, DoctorStatus::Pass);

    // Config check should fail (not initialized)
    let config_check = report.checks.iter().find(|c| c.name == "config").unwrap();
    assert_eq!(config_check.status, DoctorStatus::Fail);
}

#[test]
fn doctor_on_initialized_repo() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    // Initialize chronicle
    ops.config_set("chronicle.enabled", "true").unwrap();

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    // Config check should now pass
    let config_check = report.checks.iter().find(|c| c.name == "config").unwrap();
    assert_eq!(config_check.status, DoctorStatus::Pass);
}

#[test]
fn doctor_hooks_check_detects_missing_hooks() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    let hooks_check = report.checks.iter().find(|c| c.name == "hooks").unwrap();
    assert_eq!(hooks_check.status, DoctorStatus::Fail);
    assert!(hooks_check.fix_hint.is_some());
}

#[test]
fn doctor_hooks_check_detects_installed_hooks() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    // Install a post-commit hook that references chronicle
    let hooks_dir = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::write(
        hooks_dir.join("post-commit"),
        "#!/bin/sh\ngit-chronicle annotate --commit HEAD\n",
    )
    .unwrap();

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    let hooks_check = report.checks.iter().find(|c| c.name == "hooks").unwrap();
    assert_eq!(hooks_check.status, DoctorStatus::Pass);
}

#[test]
fn doctor_json_output_is_valid() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    // Serialize to JSON and back
    let json = serde_json::to_string(&report).unwrap();
    let parsed: chronicle::doctor::DoctorReport = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.version, report.version);
    assert_eq!(parsed.checks.len(), report.checks.len());
}

#[test]
fn doctor_has_failures_reports_correctly() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let git_dir = dir.path().join(".git");
    let report = run_doctor(&ops, &git_dir).unwrap();

    // A fresh repo should have failures (hooks not installed, config not set)
    assert!(report.has_failures());
}
