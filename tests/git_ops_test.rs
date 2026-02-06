use std::path::Path;
use std::process::Command;

use ultragit::git::{CliOps, GitOps};

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

#[test]
fn commit_info_returns_correct_metadata() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello world\n", "Initial commit");

    let info = ops.commit_info("HEAD").unwrap();
    assert_eq!(info.message, "Initial commit");
    assert_eq!(info.author_name, "Test");
    assert_eq!(info.author_email, "test@test.com");
    assert!(!info.sha.is_empty());
    assert_eq!(info.sha.len(), 40);
    assert!(info.parent_shas.is_empty());
}

#[test]
fn commit_info_has_parent_for_second_commit() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "a.txt", "a\n", "First");
    let first_sha = ops.resolve_ref("HEAD").unwrap();

    add_and_commit(dir.path(), "b.txt", "b\n", "Second");
    let info = ops.commit_info("HEAD").unwrap();
    assert_eq!(info.parent_shas.len(), 1);
    assert_eq!(info.parent_shas[0], first_sha);
}

#[test]
fn resolve_ref_returns_full_sha() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");

    let sha = ops.resolve_ref("HEAD").unwrap();
    assert_eq!(sha.len(), 40);
    assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn note_write_and_read_roundtrip() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");

    let sha = ops.resolve_ref("HEAD").unwrap();

    let note = ops.note_read(&sha).unwrap();
    assert!(note.is_none());
    assert!(!ops.note_exists(&sha).unwrap());

    let content = r#"{"schema":"ultragit/v1","test":true}"#;
    ops.note_write(&sha, content).unwrap();

    let note = ops.note_read(&sha).unwrap();
    assert!(note.is_some());
    assert_eq!(note.unwrap().trim(), content);
    assert!(ops.note_exists(&sha).unwrap());
}

#[test]
fn note_write_overwrites_existing() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    ops.note_write(&sha, "first").unwrap();
    ops.note_write(&sha, "second").unwrap();

    let note = ops.note_read(&sha).unwrap().unwrap();
    assert_eq!(note.trim(), "second");
}

#[test]
fn file_at_commit_returns_correct_content() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "version 1\n", "First");
    let sha1 = ops.resolve_ref("HEAD").unwrap();

    add_and_commit(dir.path(), "hello.txt", "version 2\n", "Second");

    let content = ops.file_at_commit(Path::new("hello.txt"), &sha1).unwrap();
    assert_eq!(content, "version 1\n");

    let content = ops.file_at_commit(Path::new("hello.txt"), "HEAD").unwrap();
    assert_eq!(content, "version 2\n");
}

#[test]
fn file_at_commit_errors_on_missing_file() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello\n", "Init");

    let result = ops.file_at_commit(Path::new("nonexistent.txt"), "HEAD");
    assert!(result.is_err());
}

#[test]
fn diff_returns_file_changes_for_root_commit() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "hello world\n", "Init");
    let sha = ops.resolve_ref("HEAD").unwrap();

    let diffs = ops.diff(&sha).unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].path, "hello.txt");
    assert_eq!(diffs[0].status, ultragit::git::DiffStatus::Added);
    assert!(diffs[0].added_line_count() > 0);
}

#[test]
fn diff_returns_modifications() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "hello.txt", "line 1\n", "Init");
    add_and_commit(dir.path(), "hello.txt", "line 1\nline 2\n", "Add line");
    let sha = ops.resolve_ref("HEAD").unwrap();

    let diffs = ops.diff(&sha).unwrap();
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].path, "hello.txt");
    assert_eq!(diffs[0].status, ultragit::git::DiffStatus::Modified);
    assert!(diffs[0].added_line_count() >= 1);
}

#[test]
fn config_get_set_roundtrip() {
    let (dir, ops) = create_temp_repo();
    add_and_commit(dir.path(), "init.txt", "init\n", "Init");

    let val = ops.config_get("ultragit.test-key").unwrap();
    assert!(val.is_none());

    ops.config_set("ultragit.test-key", "test-value").unwrap();
    let val = ops.config_get("ultragit.test-key").unwrap();
    assert_eq!(val, Some("test-value".to_string()));
}
