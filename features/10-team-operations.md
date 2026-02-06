# Feature 10: Team Operations

## Overview

Team Operations is the feature set that takes Ultragit from a single-developer tool to a team-wide knowledge system. It provides five capabilities: notes sync (pushing and fetching annotations across clones), historical backfill (retroactively annotating existing commits), export/import (portable annotation format for migrations), a comprehensive diagnostic command (`ultragit doctor`), and skill installation (teaching agents to use Ultragit).

Without these, Ultragit annotations are local-only artifacts that can't be shared, historical code has no annotations, and agents don't know Ultragit exists. Team Operations is the difference between "I installed a hook" and "my team's codebase has a memory."

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 02 Git Operations Layer | Notes sync requires refspec configuration, notes read/write, remote interaction |
| 06 Hooks & Context Capture | Skill installation references hook behavior; backfill reuses the annotation pipeline |
| 04 LLM Provider Abstraction | Backfill makes LLM API calls for historical annotation |
| 05 Writing Agent | Backfill invokes the writing agent for each historical commit |

---

## Public API

### CLI Commands

#### `ultragit sync enable`

Configures the current repository to push and fetch Ultragit notes with the remote.

```
ultragit sync enable [--remote <REMOTE>]
```

**Arguments:**
- `--remote <REMOTE>` — remote name. Default: `origin`.

**Effect:** Adds push and fetch refspecs to `.git/config`:

```ini
[remote "origin"]
    push = refs/notes/ultragit
    fetch = +refs/notes/ultragit:refs/notes/ultragit
```

#### `ultragit sync status`

Shows the sync state between local and remote notes.

```
ultragit sync status [--remote <REMOTE>]
```

**Output:**
```
Notes sync: enabled
  Push refspec:  refs/notes/ultragit -> origin
  Fetch refspec: +refs/notes/ultragit:refs/notes/ultragit
  Local notes:   1,247 annotated commits
  Remote notes:  1,130 annotated commits (117 not yet pushed)
```

#### `ultragit sync pull`

Fetches remote notes and merges them with local notes.

```
ultragit sync pull [--remote <REMOTE>] [--strategy <STRATEGY>]
```

**Arguments:**
- `--remote <REMOTE>` — remote name. Default: `origin`.
- `--strategy <ours|theirs|union>` — merge conflict strategy. Default: `union`.

#### `ultragit backfill`

Annotates historical commits that don't yet have Ultragit notes.

```
ultragit backfill [OPTIONS]
```

**Flags:**
- `--limit <N>` — annotate at most N commits.
- `--since <DATE|SHA>` — only commits after this point.
- `--path <GLOB>` — only commits touching files matching this pattern. Repeatable.
- `--concurrency <N>` — number of parallel LLM API calls. Default: `4`.
- `--model <MODEL>` — override the model for backfill (e.g., `claude-haiku-4-5-20251001` for cheaper bulk annotation).
- `--dry-run` — list which commits would be annotated without making API calls.
- `--resume` — pick up where a previously interrupted backfill left off.

#### `ultragit export`

Exports annotations to a portable JSON file.

```
ultragit export [OPTIONS] [--path <GLOB>]
```

**Flags:**
- `--path <GLOB>` — only export annotations for commits touching these paths.
- `--since <DATE|SHA>` — only export annotations from commits after this point.
- `--output <FILE>` — write to file instead of stdout.

**Output format:** One JSON object per line (JSONL), each containing:

```json
{
  "commit_sha": "abc1234...",
  "timestamp": "2025-12-15T10:30:00Z",
  "annotation": { ... }
}
```

#### `ultragit import`

Restores annotations from a previously exported JSON file.

```
ultragit import <FILE> [OPTIONS]
```

**Flags:**
- `--dry-run` — show which annotations would be imported without writing.
- `--force` — overwrite existing annotations for commits that already have notes.

**Behavior:** For each entry in the import file, checks if the commit SHA exists in the local repository. If it does and the commit has no existing annotation (or `--force` is set), writes the annotation as a git note.

#### `ultragit doctor`

Single diagnostic command that validates the entire Ultragit setup.

```
ultragit doctor [--json]
```

**Output:**
```
ultragit doctor
  [check] Binary version: 0.1.0 (up to date)
  [check] Hooks: post-commit, prepare-commit-msg, post-rewrite
  [check] Credentials: ANTHROPIC_API_KEY found, connection OK
  [check] Sync: configured, 0 unpushed annotations
  [check] Skill: installed in CLAUDE.md (current version)
  [check] Last annotation: 2 hours ago (commit abc1234)
  [warn]  Backfill: 342 commits unannotated in last 6 months
          Run: ultragit backfill --since 2025-06-01
```

**Exit codes:**
- `0` — all checks pass (warnings are OK).
- `1` — one or more checks failed.

**`--json` flag:** Output structured JSON for programmatic consumption.

#### `ultragit skill install`

Installs the Ultragit skill definition into an agent framework.

```
ultragit skill install --target <TARGET> [--global]
```

**Targets:**
- `claude-code` — appends skill definition to `CLAUDE.md` (repository root, or `~/.claude/CLAUDE.md` with `--global`).
- `mcp` — writes MCP server configuration to `.mcp.json` or the agent's MCP config file.

#### `ultragit skill export`

Writes the raw skill definition as Markdown.

```
ultragit skill export [--output <FILE>]
```

#### `ultragit skill check`

Verifies which skill installations exist and whether they're current.

```
ultragit skill check
```

**Output:**
```
Skill installations found:
  [check] Claude Code (CLAUDE.md in repository root)
          Last updated: 2025-12-15
          Skill version: ultragit-skill/v1
  [fail]  Claude Code global (~/.claude/CLAUDE.md)
          Not installed. Run: ultragit skill install --target claude-code --global
  [fail]  MCP
          Not installed. Run: ultragit skill install --target mcp
```

---

## Internal Design

### Notes Sync

#### Refspec Configuration

`ultragit sync enable` modifies `.git/config` by adding push and fetch refspecs to the specified remote. It uses `git/config.rs` to read the current remote configuration, checks for existing refspecs to avoid duplication, and appends the new ones.

```rust
/// Sync configuration state
pub struct SyncConfig {
    pub remote: String,
    pub push_refspec: Option<String>,
    pub fetch_refspec: Option<String>,
}

/// Check current sync configuration for a remote
pub fn get_sync_config(repo: &Repository, remote: &str) -> Result<SyncConfig>;

/// Enable sync by adding push/fetch refspecs
pub fn enable_sync(repo: &Repository, remote: &str) -> Result<()>;

/// Disable sync by removing ultragit refspecs
pub fn disable_sync(repo: &Repository, remote: &str) -> Result<()>;
```

#### Sync Status

`ultragit sync status` counts local and remote notes by listing annotated commits on each side. Local count comes from iterating the local notes ref. Remote count requires a `git ls-remote` or fetching the remote notes ref and counting.

```rust
pub struct SyncStatus {
    pub enabled: bool,
    pub local_count: usize,
    pub remote_count: Option<usize>,  // None if remote unreachable
    pub unpushed_count: usize,
}
```

The unpushed count is computed by diffing local notes against the last-fetched remote notes ref.

#### Notes Merge (sync pull)

Git notes have their own merge mechanics. When two clones annotate different commits, merging is trivial — the notes are on different objects. Conflict occurs when two clones annotate the *same* commit with different content.

**Merge strategy: JSON-level union.**

When both local and remote have a note on the same commit SHA, Ultragit performs a JSON-level merge:

1. Parse both annotations as JSON.
2. Merge the `regions` arrays: for each region identified by `file` + `ast_anchor.name`, keep the version with the more recent `timestamp`. If a region exists only on one side, include it.
3. Merge `cross_cutting` arrays by deduplication on `description`.
4. Take the `provenance` from the more recent annotation.
5. Write the merged result.

This is more intelligent than git's built-in `cat_sort_uniq` notes merge strategy and avoids losing data from either side.

```rust
pub enum NotesMergeStrategy {
    /// Keep local version on conflict
    Ours,
    /// Keep remote version on conflict
    Theirs,
    /// JSON-level merge of annotation content
    Union,
}

pub fn merge_annotations(
    local: &Annotation,
    remote: &Annotation,
    strategy: NotesMergeStrategy,
) -> Result<Annotation>;
```

### Auto-Sync Detection

During `ultragit init`, after creating the notes ref, check if the remote already has `refs/notes/ultragit`:

```rust
pub fn detect_remote_notes(repo: &Repository, remote: &str) -> Result<bool> {
    // git ls-remote <remote> refs/notes/ultragit
    // Returns true if the ref exists on the remote
}
```

If remote notes exist, automatically call `enable_sync()` and fetch the existing notes. Report this to the user in the init output.

### Backfill

#### Commit Discovery

Walk the commit log on the current branch, filtering by `--since`, `--path`, and `--limit`. For each commit, check if a note already exists under `refs/notes/ultragit`. Collect the unannotated commits into a work queue, ordered oldest-first.

```rust
pub struct BackfillConfig {
    pub limit: Option<usize>,
    pub since: Option<BackfillSince>,
    pub paths: Vec<String>,
    pub concurrency: usize,
    pub model: Option<String>,
    pub dry_run: bool,
    pub resume: bool,
}

pub enum BackfillSince {
    Date(chrono::NaiveDate),
    Sha(String),
}

pub struct BackfillProgress {
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_sha: String,
    pub current_message: String,
    pub estimated_remaining: Duration,
}
```

#### Resume Support

On start, if `--resume` is set, read `.git/ultragit/backfill-state.json`:

```json
{
  "started_at": "2025-12-15T10:00:00Z",
  "last_completed_sha": "abc1234",
  "total_discovered": 1247,
  "completed": 423,
  "failed_shas": ["def456", "ghi789"]
}
```

Resume from `last_completed_sha`, skipping already-annotated commits.

Without `--resume`, start fresh. Overwrite any existing backfill state file.

#### Concurrency

Use a `tokio::sync::Semaphore` to limit concurrent LLM API calls. Each commit annotation is a separate task. The semaphore bounds concurrency to `--concurrency` (default 4).

```rust
let semaphore = Arc::new(Semaphore::new(config.concurrency));
let tasks: Vec<_> = commits.into_iter().map(|commit| {
    let permit = semaphore.clone().acquire_owned().await?;
    tokio::spawn(async move {
        let result = annotate_commit(&commit, &provider).await;
        drop(permit);
        result
    })
}).collect();
```

#### Progress Display

Use a terminal progress bar (stderr) that updates in place. Show: progress fraction, percentage, ETA, and the current commit being processed. When output is not a TTY (piped), fall back to periodic line-based progress.

#### Cost Estimation

After commit discovery (before starting annotation), estimate the total cost:

```
Backfill plan:
  Commits to annotate: 1,247
  Model: claude-haiku-4-5-20251001
  Estimated API calls: 1,247
  Estimated cost: ~$3.42
  Estimated time: ~90 minutes (at concurrency 4)

  Proceed? [Y/n]
```

Cost estimation uses rough per-model token pricing and an average token count per annotation (estimated from diff size). This is a best-effort estimate, not a guarantee.

#### Oldest-First Processing

Commits are processed oldest-first so that when an annotation references `related_annotations` from a prior commit, that prior annotation already exists. This allows the annotation agent to use `get_recent_annotations()` effectively even during backfill.

### Export/Import

#### Export Format

JSONL (one JSON object per line). Each line is self-contained:

```json
{"commit_sha":"abc1234...","timestamp":"2025-12-15T10:30:00Z","annotation":{...}}
```

JSONL is chosen over a single JSON array because:
- Streaming: can export arbitrarily large annotation sets without holding everything in memory.
- Appending: can concatenate multiple exports.
- Line-level processing: `wc -l`, `head`, `tail`, `grep` all work.

```rust
pub struct ExportEntry {
    pub commit_sha: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub annotation: Annotation,
}
```

#### Path-Scoped Export

When `--path` is specified, only export annotations from commits that touch files matching the glob. The annotation itself is exported in full (not filtered to specific regions) because partial annotations would lose cross-cutting concerns.

#### Import Matching

Import iterates each entry in the file:

1. Check if `commit_sha` exists in the local repository (`git cat-file -t <sha>`).
2. If the commit doesn't exist, skip (log a warning).
3. If the commit exists and has no existing note (or `--force`), write the annotation as a note.
4. If the commit exists and already has a note, skip (unless `--force`).

Report summary at end: imported, skipped (already annotated), skipped (commit not found).

### Doctor

`ultragit doctor` runs a series of diagnostic checks and reports pass/fail/warn for each:

```rust
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: DoctorStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}
```

**Check list:**

| Check | Pass | Warn | Fail |
|-------|------|------|------|
| Binary version | Version matches latest release | — | — (always passes, just reports version) |
| Hooks status | All three hooks installed and contain ultragit invocations | Some hooks missing | No hooks installed |
| Credential check | API key found and connection test succeeds | API key found but connection test fails | No API key found |
| Sync status | Sync configured, 0 unpushed | Sync configured, >0 unpushed | Sync not configured (warn only if remote has notes) |
| Skill installation | At least one skill installed and current version | Skill installed but outdated version | No skill installed |
| Last annotation time | Within last 24 hours | Within last 7 days | No annotations found or older than 7 days |
| Backfill coverage | >90% of recent commits annotated | 50-90% annotated | <50% annotated |

**Exit code:** `0` if no Fail results. `1` if any Fail result. Warns don't affect exit code.

**JSON output (`--json`):**

```json
{
  "version": "0.1.0",
  "checks": [
    {
      "name": "hooks",
      "status": "pass",
      "message": "post-commit, prepare-commit-msg, post-rewrite",
      "fix_hint": null
    }
  ],
  "overall": "pass"
}
```

### Skill Installation

#### Skill Definition Content

The skill definition is a Markdown document embedded in the Ultragit binary as a `const &str` or `include_str!`. It contains:

- When to use each Ultragit command.
- Command syntax and common invocations.
- How to read the output.
- How to provide context when committing.

The skill definition includes a version marker comment:

```markdown
<!-- ultragit-skill/v1 -->
```

#### Claude Code Installation

For `--target claude-code`:

1. Locate `CLAUDE.md`: repository root (default) or `~/.claude/CLAUDE.md` (with `--global`).
2. If the file doesn't exist, create it with the skill definition.
3. If the file exists, check for the version marker `<!-- ultragit-skill/v1 -->`.
   - If the marker exists with the current version, do nothing (idempotent).
   - If the marker exists with an older version, replace the section between markers.
   - If no marker exists, append the skill definition to the end.

Section delimiters:

```markdown
<!-- ultragit-skill-start -->
<!-- ultragit-skill/v1 -->
[skill content here]
<!-- ultragit-skill-end -->
```

#### MCP Installation

For `--target mcp`:

1. Locate MCP config: `.mcp.json` in the repository root, or `claude_desktop_config.json` in the user's config directory.
2. Add or update the Ultragit server entry:

```json
{
  "mcpServers": {
    "ultragit": {
      "command": "ultragit",
      "args": ["mcp", "start"],
      "cwd": "<repo-root>"
    }
  }
}
```

3. If the entry already exists with the same command, do nothing (idempotent).

#### Skill Check

`ultragit skill check` scans known installation locations:

```rust
pub struct SkillInstallation {
    pub target: SkillTarget,
    pub location: PathBuf,
    pub version: Option<String>,
    pub installed: bool,
}

pub enum SkillTarget {
    ClaudeCodeLocal,
    ClaudeCodeGlobal,
    Mcp,
}
```

For each target, check if the file exists and contains the skill markers. Report version if found.

---

## Error Handling

| Failure Mode | Handling |
|---|---|
| Remote unreachable during `sync status` | Report `remote_count: unknown`, continue with local count |
| Remote unreachable during `sync pull` | Return error with message suggesting checking network/remote URL |
| Notes merge conflict during `sync pull` | Apply configured merge strategy; if JSON parse fails on either side, fall back to `theirs` strategy and warn |
| LLM API failure during backfill | Log the failed SHA, continue with next commit, report failures at end |
| All LLM calls failing during backfill | After 5 consecutive failures, pause and prompt user (rate limit? credential issue?) |
| Backfill interrupted (Ctrl+C) | Write current progress to backfill state file; next `--resume` picks up where it left off |
| Export of empty repository | Write empty JSONL file (no lines), report "0 annotations exported" |
| Import of file with unknown SHAs | Skip unknown SHAs, log warnings, report count at end |
| Import of malformed JSON | Skip malformed lines, log errors, report count at end |
| Doctor check fails to connect to API | Report as credential check failure, don't crash |
| Skill install into read-only file | Report error with the path, suggest checking file permissions |
| CLAUDE.md has unexpected format | Append skill definition at end rather than trying to parse existing content |

---

## Configuration

All configuration lives in `.git/config` under `[ultragit]` or in `.ultragit-config.toml` for shared settings.

| Key | Default | Description |
|-----|---------|-------------|
| `ultragit.sync.remote` | `origin` | Default remote for sync operations |
| `ultragit.sync.mergeStrategy` | `union` | Default merge strategy for notes conflicts |
| `ultragit.backfill.concurrency` | `4` | Default concurrency for backfill |
| `ultragit.backfill.model` | (same as main model) | Model to use for backfill annotations |
| `ultragit.skill.version` | `v1` | Current skill definition version |

---

## Implementation Steps

### Step 1: Notes Sync — Enable and Status
**Scope:** `src/sync/push_fetch.rs`, `src/cli/sync.rs`

- Implement `enable_sync()` to add push/fetch refspecs.
- Implement `get_sync_config()` to read current refspecs.
- Implement `sync status` with local/remote note counting.
- Tests: verify refspec addition is idempotent, verify counting.

### Step 2: Notes Sync — Pull with Merge
**Scope:** `src/sync/merge.rs`, `src/cli/sync.rs`

- Implement `git fetch` of remote notes ref.
- Implement JSON-level merge for conflicting notes on the same commit.
- Implement `ours`, `theirs`, `union` strategies.
- Tests: merge two annotations with non-overlapping regions, merge with overlapping regions, merge with conflicting content.

### Step 3: Auto-Sync Detection
**Scope:** `src/cli/init.rs`, `src/sync/push_fetch.rs`

- Add `detect_remote_notes()` using `git ls-remote`.
- Integrate into `ultragit init`: if remote has notes, auto-enable sync and fetch.
- Tests: init with remote notes, init without remote notes.

### Step 4: Backfill — Commit Discovery and Dry Run
**Scope:** `src/backfill.rs`, `src/cli/backfill.rs`

- Walk commit log with filters (`--since`, `--path`, `--limit`).
- Check each commit for existing notes.
- Implement `--dry-run` output.
- Cost estimation logic.
- Tests: discovery with various filters, dry-run output format.

### Step 5: Backfill — Execution with Concurrency
**Scope:** `src/backfill.rs`

- Implement concurrent annotation pipeline using tokio semaphore.
- Progress bar output (TTY-aware).
- Oldest-first ordering.
- Error handling: log failures, continue processing.
- Tests: backfill of a small test repo (with mocked LLM), concurrency behavior.

### Step 6: Backfill — Resume Support
**Scope:** `src/backfill.rs`

- Write backfill state to `.git/ultragit/backfill-state.json` on progress and on interrupt.
- Read state on `--resume` and skip completed commits.
- Handle SIGINT gracefully (write state before exit).
- Tests: interrupt and resume simulation.

### Step 7: Export
**Scope:** `src/export.rs`, `src/cli/export.rs`

- Iterate all notes under `refs/notes/ultragit`.
- Filter by `--path` and `--since`.
- Serialize to JSONL format.
- Tests: export full repo, export with path filter, export empty repo.

### Step 8: Import
**Scope:** `src/import.rs`, `src/cli/import.rs`

- Parse JSONL input.
- Match SHAs to local repository.
- Write notes (skip existing unless `--force`).
- Report summary.
- Tests: import into empty repo, import with existing annotations, import with unknown SHAs.

### Step 9: Doctor
**Scope:** `src/doctor.rs`, `src/cli/doctor.rs`

- Implement each diagnostic check as a separate function.
- Aggregate results.
- Format output (text and `--json`).
- Exit code logic.
- Tests: doctor with fully configured repo, doctor with missing hooks, doctor with missing credentials.

### Step 10: Skill Installation
**Scope:** `src/skill.rs`, `src/cli/skill.rs`

- Embed skill definition as `include_str!`.
- Implement Claude Code installation (create or update CLAUDE.md).
- Implement MCP installation (create or update .mcp.json).
- Implement `skill check` scanner.
- Implement `skill export`.
- Tests: install into new file, install into existing file, idempotent reinstall, version upgrade.

---

## Test Plan

### Unit Tests

- **Sync config parsing:** verify refspec detection in various `.git/config` formats.
- **Merge strategies:** test `ours`, `theirs`, `union` with identical, non-overlapping, and conflicting annotations.
- **JSON merge:** merge two annotations with shared regions, different timestamps, cross-cutting deduplication.
- **Backfill commit discovery:** test with `--since` date, `--since` SHA, `--path` glob, `--limit`, combinations.
- **Cost estimation:** verify cost calculation for various models and commit counts.
- **Export serialization:** verify JSONL format, verify path filtering logic.
- **Import parsing:** verify JSONL parsing, handle malformed lines, handle missing commits.
- **Doctor checks:** test each check independently with mock filesystem and git state.
- **Skill marker parsing:** test version detection in CLAUDE.md, test section replacement.

### Integration Tests

- **Full sync round-trip:** Create two clones, annotate different commits in each, sync, verify both have all annotations.
- **Sync conflict resolution:** Both clones annotate the same commit, sync, verify merged result.
- **Backfill end-to-end:** Create a repo with 20 commits, run backfill with mocked LLM, verify all 20 have notes.
- **Backfill resume:** Start a backfill, interrupt after 10/20, resume, verify all 20 annotated.
- **Export/import round-trip:** Export from repo A, import into repo B (shared history), verify annotations match.
- **Doctor on fresh install:** Run doctor immediately after init, verify appropriate warnings.
- **Skill install into CLAUDE.md:** Install, verify content, reinstall (idempotent), upgrade version.

### Edge Cases

- Backfill on a repo with zero commits.
- Backfill with `--since` pointing to a non-existent SHA.
- Export from a repo with no annotations.
- Import a file with zero valid entries.
- Sync when remote is unreachable.
- Sync when local has no notes ref yet.
- Doctor when ultragit is not initialized in the repo.
- Skill install when CLAUDE.md contains the old version marker and unrelated content.
- Backfill with `--concurrency 1` (sequential).
- Backfill with all commits already annotated (no-op).

---

## Acceptance Criteria

1. `ultragit sync enable` adds correct refspecs; `git push` includes notes; `git fetch` retrieves remote notes.
2. `ultragit sync status` correctly reports local count, remote count, and unpushed count.
3. `ultragit sync pull` merges notes from remote, handling conflicts with the configured strategy without data loss under the `union` strategy.
4. `ultragit init` auto-detects remote notes and configures sync without manual intervention.
5. `ultragit backfill` annotates historical commits oldest-first, respects `--limit`, `--since`, `--path`, and `--concurrency`.
6. `ultragit backfill --dry-run` lists commits and cost estimate without making API calls.
7. `ultragit backfill --resume` resumes an interrupted backfill without re-annotating completed commits.
8. `ultragit export | ultragit import` round-trips annotations faithfully across repositories with shared history.
9. `ultragit export --path <glob>` exports only annotations from commits touching the specified paths.
10. `ultragit doctor` exits `0` on a healthy setup, exits `1` when critical checks fail, and provides actionable fix hints for every failure.
11. `ultragit doctor --json` produces valid JSON parseable by agents.
12. `ultragit skill install --target claude-code` produces a working CLAUDE.md skill definition; reinstalling is idempotent; upgrading replaces the old version.
13. `ultragit skill install --target mcp` produces a valid MCP server entry in configuration.
14. `ultragit skill check` accurately reports installation status and version for all targets.
15. All operations degrade gracefully when the network is unavailable (sync reports status as unknown, backfill retries, doctor reports connection failure).
