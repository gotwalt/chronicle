# Feature 02: Git Operations Layer

## Overview

The Git operations layer is the abstraction between Ultragit's application logic and git itself. It provides a unified interface for diff extraction, blame queries, notes read/write, config access, and ref management. Every operation is implemented in `gix` (gitoxide) first, with an automatic fallback to the `git` CLI when gix doesn't support the operation or fails at runtime.

This layer is consumed by nearly every other feature: the writing agent needs diffs and notes storage, the read pipeline needs blame and notes retrieval, hooks need config access, and team operations need ref management. Getting this layer right is critical — it must be fast, reliable, and testable.

---

## Dependencies

- **Feature 01 (CLI & Config):** Uses `UltragitConfig` for the notes ref name, repo root, and configuration.

---

## Public API

### Core Trait

All git operations go through a `GitOps` trait. This enables testing with mock implementations and makes the gix-vs-CLI fallback transparent to callers.

```rust
use std::path::{Path, PathBuf};

/// A range of lines (1-indexed, inclusive)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// A single hunk in a diff
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub content: String,
}

/// Per-file diff information
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,  // for renames
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// A blame result: which commit produced which lines
#[derive(Debug, Clone)]
pub struct BlameEntry {
    pub commit_sha: String,
    pub original_path: PathBuf,
    pub original_start_line: u32,
    pub lines: u32,
    pub final_start_line: u32,
}

/// Result of a blame query
#[derive(Debug, Clone)]
pub struct BlameResult {
    pub path: PathBuf,
    pub entries: Vec<BlameEntry>,
}

/// A commit's basic metadata
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parents: Vec<String>,
}

pub trait GitOps: Send + Sync {
    // --- Diff ---

    /// Extract diff between a commit and its parent.
    /// For merge commits, diffs against the first parent.
    fn diff(&self, commit: &str) -> Result<Vec<FileDiff>>;

    /// Extract diff between two arbitrary commits.
    fn diff_range(&self, from: &str, to: &str) -> Result<Vec<FileDiff>>;

    // --- Blame ---

    /// Blame an entire file at HEAD.
    fn blame(&self, path: &Path) -> Result<BlameResult>;

    /// Blame a specific line range of a file at HEAD.
    fn blame_lines(&self, path: &Path, range: &LineRange) -> Result<BlameResult>;

    // --- Notes ---

    /// Read the note for a commit under the configured notes ref.
    /// Returns None if no note exists.
    fn note_read(&self, commit: &str) -> Result<Option<String>>;

    /// Write (or overwrite) the note for a commit.
    fn note_write(&self, commit: &str, content: &str) -> Result<()>;

    /// Check if a note exists for a commit.
    fn note_exists(&self, commit: &str) -> Result<bool>;

    /// List all commits that have notes under the configured ref.
    fn note_list(&self) -> Result<Vec<String>>;

    // --- Config ---

    /// Read a git config value under [ultragit].
    fn config_get(&self, key: &str) -> Result<Option<String>>;

    /// Write a git config value under [ultragit].
    fn config_set(&self, key: &str, value: &str) -> Result<()>;

    // --- Refs ---

    /// Resolve a ref (branch name, HEAD, SHA) to a full SHA.
    fn resolve_ref(&self, refspec: &str) -> Result<String>;

    /// Check if a ref exists.
    fn ref_exists(&self, refspec: &str) -> Result<bool>;

    /// Create a ref if it doesn't exist.
    fn ref_create(&self, refspec: &str, target: &str) -> Result<()>;

    // --- File content ---

    /// Read file content at a specific commit.
    fn file_at_commit(&self, path: &Path, commit: &str) -> Result<String>;

    // --- Commit info ---

    /// Get metadata for a commit.
    fn commit_info(&self, commit: &str) -> Result<CommitInfo>;

    /// Walk commits from a starting point, oldest first.
    fn walk_commits(&self, from: &str, limit: Option<u32>) -> Result<Vec<String>>;

    // --- Repository ---

    /// Get the repository root path.
    fn repo_root(&self) -> &Path;

    /// Get the notes ref namespace.
    fn notes_ref(&self) -> &str;
}
```

### Constructor

```rust
/// Create a GitOps implementation for the repository at the given root.
/// Tries gix first; falls back to CLI if gix initialization fails.
pub fn open(repo_root: &Path, notes_ref: &str) -> Result<Box<dyn GitOps>> {
    match GixOps::open(repo_root, notes_ref) {
        Ok(gix) => Ok(Box::new(gix)),
        Err(e) => {
            tracing::warn!("gix initialization failed, falling back to git CLI: {e}");
            Ok(Box::new(CliOps::new(repo_root, notes_ref)))
        }
    }
}
```

---

## Internal Design

### Dual Implementation Strategy

Every `GitOps` method has two implementations:

1. **`GixOps`** — uses the `gix` crate for pure-Rust git operations. Faster, no process spawning, no PATH dependency. Preferred.
2. **`CliOps`** — shells out to the `git` binary. Reliable, covers every edge case, handles any git version. Fallback.

The fallback is not just at initialization. Individual operations within `GixOps` can fall back to CLI:

```rust
impl GitOps for GixOps {
    fn blame_lines(&self, path: &Path, range: &LineRange) -> Result<BlameResult> {
        // gix blame with line range may not be supported
        match self.gix_blame_lines(path, range) {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::debug!("gix blame_lines failed, falling back to CLI: {e}");
                self.cli_fallback.blame_lines(path, range)
            }
        }
    }
}
```

This per-operation fallback is important because `gix` is actively developed and some operations may not be available or stable in the version Ultragit pins.

### Diff Extraction

The diff between a commit and its parent is the primary input to the writing agent.

**gix path:**
- Open the commit object via `gix::Repository::find_commit()`.
- Get the commit's tree and parent's tree.
- Compute a tree diff using `gix::diff::tree::Changes`.
- For each changed file, compute the blob diff to produce hunks.

**CLI fallback:**
```
git diff --unified=3 --no-color <parent>..<commit>
```

Parse the unified diff output into `FileDiff` structs. The parser handles:
- Standard unified diff format with `@@ -start,count +start,count @@` headers.
- Renamed files (`rename from`, `rename to`).
- Binary files (skipped with a note).
- New files (old side is empty).
- Deleted files (new side is empty).

**Edge cases:**
- Root commit (no parent): diff is the entire tree as additions.
- Merge commit: diff against first parent only (`HEAD^1..HEAD`).
- Large diffs: respect `max_diff_lines` from config. If exceeded, return a `FileDiff` with `status: TooLarge` and no hunks.

### Blame

Blame maps current lines to the commits that last modified them. This is the index for the read pipeline.

**gix path:**
- `gix::Repository::blame()` (if available in the pinned version).
- Returns blame entries per line.

**CLI fallback:**
```
git blame --porcelain -L <start>,<end> <path>
```

The `--porcelain` format is machine-parseable. Parse it into `BlameEntry` structs. The porcelain format outputs blocks like:

```
<sha> <orig_line> <final_line> <num_lines>
author ...
committer ...
filename <path>
	<line content>
```

**Line-range blame:** The `-L start,end` flag restricts blame to a range. This is critical for performance — blaming a 20-line function in a 5000-line file should not require blaming all 5000 lines.

**Caching:** Within a single Ultragit command invocation, blame results are cached by (path, range). The read pipeline may blame the same file multiple times with overlapping ranges (e.g., when resolving multiple anchors). The cache is a `HashMap<(PathBuf, Option<LineRange>), BlameResult>` wrapped in a `RefCell` or stored on the `GitOps` implementation.

```rust
pub struct CachedGitOps {
    inner: Box<dyn GitOps>,
    blame_cache: RefCell<HashMap<BlameCacheKey, BlameResult>>,
}

#[derive(Hash, Eq, PartialEq)]
struct BlameCacheKey {
    path: PathBuf,
    range: Option<LineRange>,
}
```

The cache is session-scoped — it lives for the duration of a single CLI invocation and is not persisted.

### Notes Storage

Notes are the core storage mechanism. Each annotated commit has exactly one JSON document stored as a git note under `refs/notes/ultragit`.

**Write path:**

```rust
fn note_write(&self, commit: &str, content: &str) -> Result<()>
```

**gix path:**
- Resolve the notes ref to a tree.
- Create a blob from the content.
- Update the notes tree to map `<commit-sha>` -> `<blob-sha>`.
- Create a new commit on the notes ref pointing to the updated tree.

**CLI fallback:**
```
git notes --ref=ultragit add -f -m '<content>' <commit>
```

The `-f` (force) flag is critical — it allows overwriting an existing note. Re-annotation of a commit must be idempotent. Without `-f`, `git notes add` fails if a note already exists.

**Read path:**

```rust
fn note_read(&self, commit: &str) -> Result<Option<String>>
```

**gix path:**
- Resolve the notes ref to a tree.
- Look up the tree entry for `<commit-sha>`.
- Read the blob content.

**CLI fallback:**
```
git notes --ref=ultragit show <commit>
```

Returns the raw content. The caller (schema layer) is responsible for parsing JSON.

**Existence check:**

```rust
fn note_exists(&self, commit: &str) -> Result<bool>
```

Used by backfill to skip already-annotated commits. Cheaper than a full read.

**List all notes:**

```rust
fn note_list(&self) -> Result<Vec<String>>
```

**CLI fallback:**
```
git notes --ref=ultragit list
```

Returns lines of `<blob-sha> <commit-sha>`. Parse the commit SHAs.

### Config Read/Write

Git config operations for the `[ultragit]` section.

**gix path:**
- `gix::Repository::config_snapshot()` for reads.
- Config file manipulation for writes (gix supports this).

**CLI fallback:**
```
git config --get ultragit.<key>
git config ultragit.<key> <value>
```

Note: `git config --get` exits with code 1 when the key doesn't exist. This must be handled as `Ok(None)`, not as an error.

### File Content at Commit

Read a file's content at a specific commit, used by the writing agent for context gathering.

**gix path:**
- Resolve commit to tree.
- Walk tree to find the blob at the given path.
- Read blob content.

**CLI fallback:**
```
git show <commit>:<path>
```

### Commit Walking

Walk the commit graph for backfill and history queries.

**gix path:**
- `gix::Repository::rev_walk()` from a starting ref.

**CLI fallback:**
```
git rev-list --reverse <from> [--max-count=N]
```

`--reverse` gives oldest-first ordering, which is what backfill needs (annotate chronologically so earlier annotations can be referenced by later ones).

---

## Error Handling

### Error Types

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum GitError {
    #[snafu(display("Git command failed: {command}: {stderr}, at {location}"))]
    CommandFailed {
        command: String,
        stderr: String,
        exit_code: i32,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("gix error: {message}, at {location}"))]
    Gix {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Commit not found: {sha}, at {location}"))]
    CommitNotFound {
        sha: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("File not found at commit: {path} @ {commit}, at {location}"))]
    FileNotFound {
        path: PathBuf,
        commit: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Notes ref does not exist: {refspec} (run `ultragit init`), at {location}"))]
    NotesRefMissing {
        refspec: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Diff parse error: {message}, at {location}"))]
    DiffParse {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Blame parse error: {message}, at {location}"))]
    BlameParse {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("IO error, at {location}"))]
    Io {
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: Location,
    },
}
```

### Failure Modes

- **`git` not on PATH.** `CliOps` constructor checks `which git` (or `git --version`). If git is not found and gix also fails, error with a clear message: "neither gix nor git CLI available."
- **Notes ref doesn't exist.** `note_read` returns `Ok(None)`. `note_write` creates the ref if it doesn't exist (auto-init).
- **Commit SHA doesn't exist.** `CommitNotFound` error. Callers handle this (backfill skips, read returns empty).
- **Corrupted note content.** `note_read` returns raw string. The caller (schema layer) handles JSON parse errors and surfaces them as annotation-level warnings.
- **gix operation fails at runtime.** Log the error, fall back to CLI, log that fallback was used. If CLI also fails, propagate the CLI error.
- **Large repositories with slow blame.** Blame can be slow on very large files. The `blame_lines` method with a range avoids full-file blame. Callers should scope blame to the narrowest range possible.

---

## Configuration

The git operations layer reads these config values:

| Key | Used For | Default |
|-----|----------|---------|
| `noteref` | Notes ref namespace for all note operations | `refs/notes/ultragit` |

The layer itself is mostly config-free — it receives the repo root and notes ref at construction time.

---

## Implementation Steps

### Step 1: `GitOps` Trait and `CliOps` Skeleton

Define the `GitOps` trait with all methods. Implement `CliOps` with the full CLI fallback for every method. This gives a working git operations layer immediately, using `git` commands.

**Deliverable:** `CliOps` passes all trait methods via `git` commands. Unit tests for diff parsing, blame parsing, and notes operations.

### Step 2: Diff Extraction

Implement `diff()` and `diff_range()` in `CliOps`. Write the unified diff parser that produces `Vec<FileDiff>`. Handle renames, additions, deletions, binary files.

**Deliverable:** `diff("HEAD")` returns correct `FileDiff` structs for a test commit. Parser handles all file status types.

### Step 3: Blame Wrapper

Implement `blame()` and `blame_lines()` in `CliOps`. Write the porcelain blame parser. Implement the blame cache.

**Deliverable:** `blame_lines(path, range)` returns correct `BlameEntry` structs. Cache prevents redundant calls within a session.

### Step 4: Notes Read/Write

Implement `note_read()`, `note_write()`, `note_exists()`, `note_list()` in `CliOps`. Handle the notes ref lifecycle (create if missing on first write).

**Deliverable:** Round-trip test: write a note, read it back, verify content. List notes after multiple writes.

### Step 5: Remaining CliOps Methods

Implement `config_get()`, `config_set()`, `resolve_ref()`, `ref_exists()`, `ref_create()`, `file_at_commit()`, `commit_info()`, `walk_commits()`.

**Deliverable:** All `GitOps` trait methods implemented and tested in `CliOps`.

### Step 6: `GixOps` Implementation

Implement `GixOps` using the `gix` crate. Start with the operations gix handles well: commit info, file content, config, ref resolution. Diff and blame may need partial or full fallback to CLI depending on gix version.

**Deliverable:** `GixOps` with per-method fallback to `CliOps`. Benchmarks comparing gix vs CLI for common operations.

### Step 7: `open()` Constructor and Auto-Selection

Implement the `open()` function that tries `GixOps` first and falls back to `CliOps`. Add the `CachedGitOps` wrapper.

**Deliverable:** `git::open(repo_root, notes_ref)` returns a working `GitOps` implementation regardless of gix availability.

### Step 8: Integration Tests with Real Repositories

Write integration tests that create temporary git repos, make commits, and exercise every `GitOps` method against real git state.

**Deliverable:** Test suite creating repos with merges, renames, and multi-file commits. All operations verified.

---

## Test Plan

### Unit Tests

- **Diff parser:** Parse sample unified diffs covering additions, deletions, modifications, renames, binary files, and multi-file diffs. Verify `FileDiff` structs.
- **Blame parser:** Parse sample porcelain blame output. Verify `BlameEntry` fields. Test boundary detection (where one commit's lines end and another's begin).
- **Notes round-trip:** Write JSON content as a note, read it back, verify byte-exact match.
- **Config round-trip:** Set a config key, read it back, verify value.
- **Commit walking:** Walk a linear history, verify oldest-first ordering. Walk with limit, verify count.
- **Blame caching:** Call `blame_lines` twice with the same args, verify the underlying operation runs only once (use a counting wrapper).

### Integration Tests

- **Fresh repository:** Create a repo, make 5 commits modifying different files. Test diff, blame, and notes against each commit.
- **Merge commit:** Create a branch, make changes on both sides, merge. Verify `diff()` on the merge commit shows only the first-parent diff.
- **Rename tracking:** Rename a file across commits. Verify blame tracks through the rename.
- **Root commit:** Verify `diff()` on the initial commit shows all files as additions.
- **No notes ref:** Read a note when the notes ref doesn't exist yet. Verify `Ok(None)`.
- **Force overwrite:** Write a note, write a different note to the same commit, verify the second note wins.
- **Large file blame range:** Create a 1000-line file, blame lines 500-510, verify only those entries are returned.

### Edge Cases

- Commit with no changes (empty commit).
- Commit with only binary file changes.
- File paths with spaces and special characters.
- Notes content containing special characters that need shell escaping (in CLI fallback).
- Git config keys with dots and dashes.
- Bare repositories (should error gracefully).
- Shallow clones (blame may not trace to the true origin — should not crash).
- Detached HEAD state.

---

## Acceptance Criteria

1. `git::open()` returns a working `GitOps` implementation in any standard git repository.
2. `diff("HEAD")` returns correct per-file, per-hunk diff information for the HEAD commit.
3. `blame_lines(path, range)` returns correct `(commit_sha, line_range)` pairs for the specified range.
4. `note_write()` followed by `note_read()` returns the exact content written.
5. `note_write()` with `-f` semantics: writing to an already-annotated commit succeeds and overwrites.
6. Notes are stored under the configured `refs/notes/ultragit` ref (or custom ref from config).
7. `note_list()` returns all annotated commit SHAs.
8. Blame caching eliminates redundant blame calls within a session (measurable via debug logging or test instrumentation).
9. When `gix` fails for a specific operation, the CLI fallback activates transparently (visible in debug logs, invisible to the caller).
10. All operations handle missing files, missing commits, and missing refs without panicking — they return appropriate `Err` variants.
11. Integration tests pass with real git repositories covering merges, renames, and multi-file commits.
12. `git` CLI not being on PATH is detected at `CliOps` construction time, not at first use.
