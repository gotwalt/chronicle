# Feature 06: Hooks & Context Capture

## Overview

This feature covers the mechanisms by which Ultragit integrates into the git commit workflow: hook installation, hook chaining with existing hooks, the `ultragit commit` wrapper command, the `ultragit context set` command, the `pending-context.json` lifecycle, environment variable fallbacks, pre-LLM filtering of trivial commits, async/sync execution models, and the `ultragit init` orchestration command.

Hooks are the entry point for the entire write path. Without them, annotations don't happen. The design priorities are: never break the user's existing git workflow, never block the terminal by default, degrade gracefully when anything goes wrong, and make the common case (agent commits with context) require zero configuration after `ultragit init`.

---

## Dependencies

| Feature | What it provides |
|---------|-----------------|
| 01 CLI & Config | CLI framework, `UltragitConfig`, clap subcommand structure |
| 02 Git Operations | Notes ref creation, git config read/write, diff extraction (for pre-LLM filtering) |
| 05 Writing Agent | `annotate_commit()` — the function hooks ultimately invoke |

---

## Public API

### CLI Commands

#### `ultragit init`

```
ultragit init [OPTIONS]

Initializes Ultragit in the current git repository.

Options:
  --provider <NAME>       Pin LLM provider (anthropic, openai, gemini, openrouter)
  --model <MODEL>         Pin model identifier
  --sync                  Use synchronous mode (block on commit until annotation completes)
  --include <GLOB>...     Only annotate files matching these patterns
  --exclude <GLOB>...     Exclude files matching these patterns
  --no-hooks              Skip hook installation (configure only)
  --dry-run               Show what would be installed without doing it

Actions:
  1. Install git hooks (post-commit, prepare-commit-msg, post-rewrite)
  2. Create refs/notes/ultragit if it doesn't exist
  3. Create .git/ultragit/ directory
  4. Write [ultragit] section to .git/config
  5. Check for .ultragit-config.toml and apply defaults
  6. Check for existing remote annotations and auto-configure sync
  7. Run credential check
  8. Optionally run a dry-run annotation test
```

#### `ultragit commit`

```
ultragit commit [GIT_COMMIT_OPTIONS] [ULTRAGIT_OPTIONS]

Wraps `git commit` with context capture. Writes context to
.git/ultragit/pending-context.json, then invokes `git commit`
with all pass-through flags.

Ultragit-specific options:
  --task <TEXT>            Task identifier or description
  --reasoning <TEXT>       Reasoning, rejected alternatives, tradeoffs
  --dependencies <TEXT>    Semantic dependencies the author is aware of
  --tags <TAG,...>         Comma-separated categorization tags

All other options are passed through to `git commit`:
  -m <MSG>                Commit message
  -a                      Stage all modified files
  --amend                 Amend the previous commit
  --allow-empty           Allow empty commit
  ... (any git commit flag)
```

#### `ultragit context set`

```
ultragit context set [OPTIONS]

Writes context to .git/ultragit/pending-context.json without committing.
The next `git commit` will pick up and consume the context.

Options:
  --task <TEXT>
  --reasoning <TEXT>
  --dependencies <TEXT>
  --tags <TAG,...>
  --clear                 Delete pending-context.json without committing
```

### Internal Types

```rust
/// Context provided by the author for the next commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingContext {
    pub task: Option<String>,
    pub reasoning: Option<String>,
    pub dependencies: Option<String>,
    pub tags: Option<Vec<String>>,
    pub timestamp: String,  // ISO 8601 — used for staleness detection
}

/// Result of pre-LLM filtering heuristics.
#[derive(Debug, PartialEq)]
pub enum FilterDecision {
    /// Proceed with LLM annotation.
    Annotate,
    /// Skip annotation entirely (commit is trivial).
    Skip(String),  // reason
    /// Produce a minimal annotation locally without an API call.
    MinimalLocal(String),  // reason
}

/// Hook type for installation.
#[derive(Debug, Clone, Copy)]
pub enum HookType {
    PostCommit,
    PrepareCommitMsg,
    PostRewrite,
}
```

---

## Internal Design

### Hook Installation

`ultragit init` installs three hooks into `.git/hooks/`:

#### `post-commit`

```bash
#!/bin/sh
# ultragit post-commit hook
# Installed by: ultragit init
# To remove: ultragit uninstall

# Run annotation in background by default.
# ultragit handles its own error logging.
if command -v ultragit >/dev/null 2>&1; then
  ultragit annotate --commit HEAD --async 2>/dev/null &
fi
```

When `--sync` mode is configured:

```bash
#!/bin/sh
if command -v ultragit >/dev/null 2>&1; then
  ultragit annotate --commit HEAD --sync
fi
```

#### `prepare-commit-msg`

```bash
#!/bin/sh
# ultragit prepare-commit-msg hook
# Detects squash merges and records source commits.

COMMIT_MSG_FILE="$1"
COMMIT_SOURCE="$2"

if command -v ultragit >/dev/null 2>&1; then
  ultragit hook prepare-commit-msg "$COMMIT_MSG_FILE" "$COMMIT_SOURCE" 2>/dev/null
fi
```

The `ultragit hook prepare-commit-msg` subcommand:
1. Checks if `$COMMIT_SOURCE` is `squash` or `merge`.
2. Checks for `.git/SQUASH_MSG` (present during `git merge --squash`).
3. If squash detected: resolves source commit SHAs, writes `.git/ultragit/pending-squash.json`.
4. Exits silently if not a squash.

#### `post-rewrite`

```bash
#!/bin/sh
# ultragit post-rewrite hook
# Migrates annotations when commits are amended.

if command -v ultragit >/dev/null 2>&1; then
  ultragit hook post-rewrite "$1" 2>/dev/null
fi
```

The `ultragit hook post-rewrite` subcommand:
1. Reads old-SHA → new-SHA pairs from stdin (git provides these).
2. For each pair: reads annotation from old SHA, passes it (along with new diff) to the annotation agent for migration.
3. Writes migrated annotation on new SHA.

### Chaining with Existing Hooks

If a hook file already exists when `ultragit init` runs:

1. **Check if it's already an Ultragit hook.** Look for the `# ultragit` marker comment. If found, replace the Ultragit section (upgrade).

2. **Check if it's a different hook.** Read the existing content.

3. **Append Ultragit invocation.** Add the Ultragit block at the end of the existing script, separated by a comment:

```bash
# --- existing hook content above ---

# --- ultragit hook (added by ultragit init) ---
if command -v ultragit >/dev/null 2>&1; then
  ultragit annotate --commit HEAD --async 2>/dev/null &
fi
# --- end ultragit hook ---
```

4. **Preserve the shebang.** If the existing hook uses `#!/bin/bash` or `#!/usr/bin/env python`, keep it. Only add a shebang if the file doesn't have one.

5. **Report what happened:**
```
✓ Appended to existing post-commit hook
✓ Installed prepare-commit-msg hook (new)
✓ Appended to existing post-rewrite hook
```

**Removal:** `ultragit uninstall` removes only the content between the `# --- ultragit hook` markers, preserving the rest of the hook.

**Hook manager detection:** If Husky (`.husky/` directory), Lefthook (`.lefthook.yml`), or pre-commit (`.pre-commit-config.yaml`) are detected, warn the user:

```
⚠ Detected Lefthook hook manager. Ultragit hooks may conflict.
  Consider using `ultragit init --no-hooks` and adding Ultragit
  to your Lefthook configuration instead:

  # .lefthook.yml
  post-commit:
    commands:
      ultragit:
        run: ultragit annotate --commit HEAD --async
```

### `ultragit commit` Implementation

1. Parse arguments: separate Ultragit-specific flags (`--task`, `--reasoning`, `--dependencies`, `--tags`) from git commit flags (everything else).

2. Write `PendingContext` to `.git/ultragit/pending-context.json`:
   ```json
   {
     "task": "PROJ-442: implement connection pooling",
     "reasoning": "Chose bounded pool with LRU eviction because...",
     "dependencies": "Assumes max_sessions in TlsSessionCache is 4",
     "tags": ["mqtt", "performance"],
     "timestamp": "2025-12-15T10:30:00Z"
   }
   ```

3. Execute `git commit` with the remaining (pass-through) arguments. Use `std::process::Command` and inherit stdin/stdout/stderr so interactive commit flows (editor, GPG signing) work correctly.

4. If `git commit` exits non-zero (commit aborted, pre-commit hook failed, etc.), delete `pending-context.json` to avoid stale context leaking to a future commit. Return the same exit code.

5. If `git commit` succeeds, the post-commit hook fires and invokes `ultragit annotate`. The annotate command reads and deletes `pending-context.json`.

**Flag collision handling:** If the user passes `--task` as a git commit flag (unlikely but possible), Ultragit consumes it. Document this. If this becomes a real issue, use `--ultragit-task` prefixed names.

### `ultragit context set` Implementation

1. Parse `--task`, `--reasoning`, `--dependencies`, `--tags`.
2. If `--clear` is passed, delete `.git/ultragit/pending-context.json` and exit.
3. If `pending-context.json` already exists, merge new fields into it (don't overwrite fields not specified in this call).
4. Write `.git/ultragit/pending-context.json`.
5. Print confirmation: `Context saved. Will be consumed by the next commit.`

### `pending-context.json` Lifecycle

```
ultragit commit --task "..." --reasoning "..."
  ├── Writes .git/ultragit/pending-context.json
  ├── Calls git commit -m "..."
  │   ├── pre-commit hooks run
  │   ├── Commit created (new SHA)
  │   └── post-commit hook fires
  │       └── ultragit annotate --commit HEAD
  │           ├── Reads .git/ultragit/pending-context.json
  │           ├── Deletes .git/ultragit/pending-context.json
  │           ├── Passes context to annotation agent
  │           └── Stores annotation as git note
  └── Returns exit code from git commit
```

**Staleness protection:** If `pending-context.json` has a `timestamp` older than 10 minutes, log a warning:

```
[ultragit] ⚠ pending-context.json is 47 minutes old. It may be from a previous
           failed commit. Using it anyway. Run `ultragit context set --clear`
           to discard.
```

This handles the case where `ultragit context set` was called but then no commit happened.

### ULTRAGIT_* Environment Variable Fallback

When `pending-context.json` doesn't exist, check environment variables:

```rust
fn read_author_context(ultragit_dir: &Path) -> Option<AuthorContext> {
    // Priority 1: pending-context.json
    let pending_path = ultragit_dir.join("pending-context.json");
    if pending_path.exists() {
        if let Ok(ctx) = read_and_delete_pending_context(&pending_path) {
            return Some(ctx);
        }
    }

    // Priority 2: ULTRAGIT_* environment variables
    let task = std::env::var("ULTRAGIT_TASK").ok();
    let reasoning = std::env::var("ULTRAGIT_REASONING").ok();
    let dependencies = std::env::var("ULTRAGIT_DEPENDENCIES").ok();
    let tags = std::env::var("ULTRAGIT_TAGS").ok()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    if task.is_some() || reasoning.is_some() || dependencies.is_some() {
        return Some(AuthorContext { task, reasoning, dependencies, tags, squash_sources: None });
    }

    None
}
```

Environment variables are not deleted after reading (they're per-process). This is fine for CI pipelines where each commit runs in its own process. For interactive use, `pending-context.json` is preferred because it's consumed atomically.

### Pre-LLM Filtering

Before making the LLM API call, apply heuristics to avoid wasting API calls on trivial commits:

```rust
pub fn filter_commit(
    diff: &CommitDiff,
    commit_message: &str,
    config: &UltragitConfig,
) -> FilterDecision {
    // 1. Check if diff only touches excluded paths
    if all_files_excluded(diff, config) {
        return FilterDecision::Skip("all changed files match exclude patterns".into());
    }

    // 2. Check for lockfile-only changes
    let lockfile_patterns = ["Cargo.lock", "package-lock.json", "yarn.lock",
                              "pnpm-lock.yaml", "Gemfile.lock", "poetry.lock",
                              "composer.lock", "go.sum"];
    if diff.files.iter().all(|f| lockfile_patterns.iter().any(|p| f.path.ends_with(p))) {
        return FilterDecision::Skip("lockfile update only".into());
    }

    // 3. Check commit message patterns
    let skip_patterns = ["Merge branch", "Merge pull request",
                         "WIP", "wip", "fixup!", "squash!"];
    if skip_patterns.iter().any(|p| commit_message.starts_with(p)) {
        return FilterDecision::Skip(format!("commit message matches skip pattern: {}", commit_message));
    }

    // 4. Check for tiny diffs (below trivial threshold)
    let meaningful_lines = count_meaningful_changed_lines(diff);
    let threshold = config.trivial_threshold.unwrap_or(3);
    if meaningful_lines <= threshold {
        return FilterDecision::MinimalLocal(
            format!("only {} meaningful lines changed (threshold: {})", meaningful_lines, threshold)
        );
    }

    // 5. Check for oversized diffs
    let total_lines = count_total_changed_lines(diff);
    let max_lines = config.max_diff_lines.unwrap_or(2000);
    if total_lines > max_lines {
        // Don't skip, but the agent will use chunking
        // Just log a note
        tracing::info!("large diff ({} lines), annotation will use chunking", total_lines);
    }

    FilterDecision::Annotate
}

/// Count changed lines excluding whitespace-only, comment-only, and import-only changes.
fn count_meaningful_changed_lines(diff: &CommitDiff) -> usize {
    // ... implementation
}
```

**Minimal local annotations:** For commits below the trivial threshold, produce a minimal annotation without an API call:

```json
{
  "$schema": "ultragit/v1",
  "commit": "<sha>",
  "timestamp": "<iso8601>",
  "summary": "Trivial change: version bump in Cargo.toml",
  "context_level": "inferred",
  "regions": [],
  "cross_cutting": [],
  "provenance": { "operation": "initial", "derived_from": [], "original_annotations_preserved": true }
}
```

### Async Execution Model

**Default: asynchronous.** The post-commit hook must not block the terminal.

#### Unix (macOS, Linux)

The hook script uses `&` to background the process:

```bash
ultragit annotate --commit HEAD --async 2>/dev/null &
```

Inside `ultragit annotate --async`:

1. Fork a background process using `daemonize` or `nohup` semantics:
   ```rust
   // Double-fork to detach from terminal
   match unsafe { libc::fork() } {
       0 => {
           // Child: create new session to detach from terminal
           unsafe { libc::setsid() };
           // Close stdin/stdout/stderr, redirect to log
           // Run annotation
           run_annotation(commit_sha, config).await;
           std::process::exit(0);
       }
       pid if pid > 0 => {
           // Parent: exit immediately so the hook returns
           std::process::exit(0);
       }
       _ => {
           // Fork failed: fall back to synchronous
           run_annotation(commit_sha, config).await;
       }
   }
   ```

2. Alternatively, use `std::process::Command` to spawn a detached child:
   ```rust
   std::process::Command::new("ultragit")
       .args(["annotate", "--commit", &sha, "--detached"])
       .stdin(Stdio::null())
       .stdout(Stdio::null())
       .stderr(Stdio::null())
       .spawn()?;
   ```

   The `--detached` flag tells the spawned process it's already detached and should run synchronously in its own process.

**Prefer the `Command::spawn` approach** — it's simpler, portable, and avoids `unsafe`. The hook script still backgrounds the initial `ultragit` invocation with `&`, and the `--async` flag within `ultragit annotate` spawns the long-running annotation in a fully detached child process.

#### Windows

Windows doesn't support `&` in shell hooks the same way. Use `CREATE_NEW_PROCESS_GROUP` and `DETACHED_PROCESS` flags:

```rust
#[cfg(target_os = "windows")]
fn spawn_detached(sha: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const DETACHED_PROCESS: u32 = 0x00000008;

    std::process::Command::new("ultragit")
        .args(["annotate", "--commit", sha, "--detached"])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
```

#### CI Detection

In CI environments, annotation should run synchronously (no terminal to return to, and the process may be killed after the pipeline step ends):

```rust
fn is_ci() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
        || std::env::var("CIRCLECI").is_ok()
        || std::env::var("BUILDKITE").is_ok()
}
```

When CI is detected and `--async` is specified, log a note and run synchronously:

```
[ultragit] CI environment detected. Running annotation synchronously.
```

### Race Condition Handling

Concurrent commits (e.g., multiple worktrees, or an agent committing rapidly) can create races on `pending-context.json` and the notes ref.

**File locking for `pending-context.json`:**

```rust
use std::fs::OpenOptions;

fn write_pending_context(ultragit_dir: &Path, ctx: &PendingContext) -> Result<()> {
    let lock_path = ultragit_dir.join("pending-context.lock");
    let _lock = FileLock::acquire(&lock_path, Duration::from_secs(5))?;

    let ctx_path = ultragit_dir.join("pending-context.json");
    let content = serde_json::to_string_pretty(ctx)?;
    std::fs::write(&ctx_path, content)?;
    Ok(())
}

fn read_and_delete_pending_context(path: &Path) -> Result<PendingContext> {
    let lock_path = path.with_extension("lock");
    let _lock = FileLock::acquire(&lock_path, Duration::from_secs(5))?;

    let content = std::fs::read_to_string(path)?;
    std::fs::remove_file(path)?;
    serde_json::from_str(&content).map_err(Into::into)
}
```

Use a simple lockfile mechanism (`flock` on Unix, `LockFileEx` on Windows) rather than pulling in a heavy dependency. A 5-second timeout is sufficient — if the lock is held longer than that, something is wrong.

**Notes ref locking:** Git handles notes ref locking internally. Concurrent `git notes add` commands will fail with a lock error. The retry wrapper in the storage layer (Feature 02) handles this with a short retry.

### `ultragit init` Orchestration

The init command performs a sequence of checks and installations:

```rust
pub fn run_init(opts: &InitOptions) -> Result<()> {
    let repo_root = find_git_repo_root()?;
    let git_dir = repo_root.join(".git");
    let ultragit_dir = git_dir.join("ultragit");

    // 1. Create .git/ultragit/ directory
    fs::create_dir_all(&ultragit_dir)?;

    // 2. Install hooks (unless --no-hooks)
    if !opts.no_hooks {
        install_hook(&git_dir, HookType::PostCommit, opts.sync)?;
        install_hook(&git_dir, HookType::PrepareCommitMsg, opts.sync)?;
        install_hook(&git_dir, HookType::PostRewrite, opts.sync)?;
    }

    // 3. Create notes ref if it doesn't exist
    create_notes_ref(&repo_root)?;

    // 4. Write [ultragit] config to .git/config
    write_git_config(&git_dir, opts)?;

    // 5. Check for .ultragit-config.toml
    if let Some(shared_config) = read_shared_config(&repo_root)? {
        // Prompt to apply shared config (in interactive mode)
        // Apply silently in non-interactive mode
    }

    // 6. Check for existing remote annotations
    if let Ok(true) = remote_has_ultragit_notes(&repo_root) {
        configure_notes_sync(&git_dir)?;
        println!("  ✓ Detected existing Ultragit annotations on origin");
        println!("  ✓ Sync configured automatically");
    }

    // 7. Credential check
    match discover_provider(&config).await {
        Ok(provider) => {
            match provider.check_auth().await {
                Ok(status) => println!("  ✓ Credentials: {} found", status.message),
                Err(e) => println!("  ✗ Credential check failed: {}", e),
            }
        }
        Err(ProviderError::NoCredentials) => {
            println!("  ✗ No LLM credentials found.");
            println!("    Run: ultragit auth check");
        }
        Err(e) => println!("  ✗ Credential error: {}", e),
    }

    // 8. First-commit confirmation
    println!("\n  Ultragit is ready. Your next commit will be annotated.");

    Ok(())
}
```

**Dry run mode:** When `--dry-run` is passed, print what would be done without actually doing it:

```
[dry run] Would install post-commit hook to .git/hooks/post-commit
[dry run] Would install prepare-commit-msg hook to .git/hooks/prepare-commit-msg
[dry run] Would install post-rewrite hook to .git/hooks/post-rewrite
[dry run] Would create refs/notes/ultragit
[dry run] Would write [ultragit] config to .git/config
```

---

## Error Handling

| Failure Mode | Handling |
|-------------|----------|
| Not in a git repository | `ultragit init` exits with error: "not a git repository" |
| Hook file exists but not writable | Error with guidance: "cannot write to .git/hooks/post-commit (permission denied)" |
| Hook file exists and is a symlink (e.g., Husky) | Warn and suggest `--no-hooks` |
| `ultragit` not on PATH when hook fires | Hook's `command -v` check fails silently. No annotation, no error. |
| `pending-context.json` is corrupt | Log warning, skip context (treat as inferred). Delete the corrupt file. |
| `pending-context.json` lock timeout | Log warning, skip context. Don't block the commit. |
| `git commit` fails after `pending-context.json` is written | `ultragit commit` deletes `pending-context.json` on non-zero exit. |
| Async annotation process crashes | Error is logged to `.git/ultragit/failed.log`. Commit is unaffected. |
| Pre-LLM filter incorrectly skips a non-trivial commit | Acceptable false negative. User can re-annotate with `ultragit annotate --commit <sha>`. |
| `ultragit init` in a bare repository | Error: "ultragit init requires a working tree" |
| Two `ultragit init` runs in sequence | Idempotent. Second run detects existing hooks and reports "already installed" or upgrades. |

---

## Configuration

### Git Config (`[ultragit]` section, written by `ultragit init`)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Master enable/disable switch |
| `async` | bool | `true` | Async annotation (background process) |
| `noteref` | string | `refs/notes/ultragit` | Notes reference namespace |
| `provider` | string | (auto) | Pinned LLM provider |
| `model` | string | (auto) | Pinned model |
| `include` | string | (none) | Comma-separated glob patterns for files to annotate |
| `exclude` | string | (none) | Comma-separated glob patterns for files to skip |
| `maxDiffLines` | integer | 2000 | Max diff lines before skipping annotation |
| `skipTrivial` | bool | `true` | Enable pre-LLM trivial commit filtering |
| `trivialThreshold` | integer | 3 | Min meaningful changed lines to annotate |

### `.ultragit-config.toml` (shared, checked into repo)

```toml
[ultragit]
enabled = true
async = true

[ultragit.model]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
backfill_model = "claude-haiku-4-5-20251001"

[ultragit.scope]
include = ["src/**", "lib/**", "config/**"]
exclude = ["*.generated.*", "vendor/**", "node_modules/**"]
max_diff_lines = 2000

[ultragit.sync]
auto_sync = true
```

When both `.git/config` and `.ultragit-config.toml` exist, `.git/config` takes precedence (local overrides shared).

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ULTRAGIT_TASK` | Task identifier for the next commit |
| `ULTRAGIT_REASONING` | Reasoning text for the next commit |
| `ULTRAGIT_DEPENDENCIES` | Dependency declarations for the next commit |
| `ULTRAGIT_TAGS` | Comma-separated tags for the next commit |
| `ULTRAGIT_SQUASH_SOURCES` | Source commit SHAs for squash annotation |
| `ULTRAGIT_DISABLED` | If set to `1` or `true`, skip annotation entirely |

---

## Implementation Steps

### Step 1: Hook installation and chaining
- Implement `install_hook(git_dir, hook_type, sync)` — writes hook script, chains with existing hooks.
- Implement existing hook detection and marker-based append/upgrade.
- Implement hook manager detection (Husky, Lefthook, pre-commit) with warnings.
- Unit tests: install to empty hooks dir, append to existing hook, upgrade existing Ultragit hook, detect hook managers.
- **PR scope:** `src/hooks/install.rs`.

### Step 2: `ultragit init` orchestration
- Implement the full init sequence: create dirs, install hooks, create notes ref, write config, check shared config, check remote annotations, check credentials.
- Implement `--dry-run`, `--no-hooks`, `--sync`, `--provider`, `--model`, `--include`, `--exclude`.
- Integration test: run init in a fresh test repository, verify all artifacts are created.
- **PR scope:** `src/cli/init.rs`.

### Step 3: `pending-context.json` lifecycle
- Implement `PendingContext` type with serialization.
- Implement write, read-and-delete, and staleness detection.
- Implement file locking (`flock` on Unix, `LockFileEx` on Windows).
- Unit tests: write/read/delete lifecycle, staleness warning, lock contention.
- **PR scope:** Part of `src/hooks/mod.rs` or `src/annotate/gather.rs`.

### Step 4: `ultragit commit` command
- Implement argument parsing: separate Ultragit flags from git commit flags.
- Write pending-context.json, execute `git commit`, handle exit codes.
- Delete pending-context.json on commit failure.
- Integration test: commit with context, verify pending-context.json is created and consumed.
- **PR scope:** `src/cli/commit.rs`.

### Step 5: `ultragit context set` command
- Implement context writing, merging, and `--clear`.
- Unit tests: set context, set partial context (merge), clear context.
- **PR scope:** `src/cli/context.rs`.

### Step 6: Environment variable fallback
- Implement `read_author_context()` — check pending-context.json first, then ULTRAGIT_* env vars.
- Unit tests: verify priority (file over env), verify env parsing.
- **PR scope:** Part of `src/annotate/gather.rs`.

### Step 7: Pre-LLM filtering
- Implement `filter_commit()` with all heuristics: excluded paths, lockfiles, commit message patterns, trivial threshold.
- Implement `count_meaningful_changed_lines()`.
- Implement minimal local annotation generation for trivial commits.
- Unit tests: each filter case (lockfile only, WIP message, below threshold, oversized).
- **PR scope:** `src/annotate/filter.rs`.

### Step 8: Async execution model
- Implement background process spawning for Unix (`Command::spawn` with detach).
- Implement background process spawning for Windows (`CREATE_NEW_PROCESS_GROUP`).
- Implement CI detection.
- Implement the `--async`, `--sync`, `--detached` flag handling.
- Test: verify hook returns immediately in async mode, verify annotation completes in background.
- **PR scope:** Part of `src/cli/annotate.rs` and `src/hooks/post_commit.rs`.

### Step 9: Post-commit hook logic
- Wire together: read config → check enabled → read pending-context.json → read pending-squash.json → filter commit → spawn annotation.
- Handle the squash path (defer full implementation to Feature 09, but detect and log here).
- Integration test: make a commit in a test repo with hooks installed, verify annotation is produced.
- **PR scope:** `src/hooks/post_commit.rs`.

### Step 10: Uninstall command
- Implement `ultragit uninstall`: remove hook markers, remove config section, remove `.git/ultragit/` directory.
- Verify existing non-Ultragit hook content is preserved.
- **PR scope:** `src/cli/init.rs` (or separate uninstall.rs).

---

## Test Plan

### Unit Tests

**Hook installation:**
- Install into empty `.git/hooks/` — hook file is created with correct content and is executable.
- Install when hook already exists — Ultragit section is appended between markers.
- Install when Ultragit hook already exists — section is replaced (upgrade).
- Uninstall removes only Ultragit markers, preserves other content.
- Hook scripts contain `command -v` guard.

**Pending context lifecycle:**
- Write, read, delete cycle works atomically.
- Staleness detection: context with timestamp >10 minutes old triggers warning.
- Corrupt JSON: read returns error, file is deleted.
- Concurrent access: two threads writing/reading don't corrupt data (lock test).

**Pre-LLM filtering:**
- Lockfile-only diff → `Skip`.
- Single-line version bump → `MinimalLocal`.
- "WIP" commit message → `Skip`.
- "fixup!" commit message → `Skip`.
- "Merge branch" message → `Skip`.
- 50-line code change → `Annotate`.
- 2-line code change (below threshold) → `MinimalLocal`.
- All files match exclude patterns → `Skip`.
- Mix of excluded and included files → `Annotate` (for included files).

**`ultragit commit` argument parsing:**
- `ultragit commit -m "msg" --task "T" --reasoning "R"` → context has task and reasoning, git gets `-m "msg"`.
- `ultragit commit -am "msg"` → no context, git gets `-am "msg"`.
- `ultragit commit --amend --task "T"` → context has task, git gets `--amend`.
- Unknown flags pass through to git.

**CI detection:**
- Returns true when `CI=true` is set.
- Returns true when `GITHUB_ACTIONS=true` is set.
- Returns false when no CI vars are set.

### Integration Tests

- **Full init cycle:** Create temp repo, run `ultragit init`, verify hooks exist and are executable, verify notes ref exists, verify config is written.
- **Init with existing hooks:** Create temp repo with pre-existing post-commit hook, run `ultragit init`, verify both hooks fire.
- **Init idempotency:** Run `ultragit init` twice, verify no duplicate hook content.
- **Commit with context:** Run `ultragit init`, use `ultragit commit -m "test" --task "T"`, verify pending-context.json is created then consumed.
- **Context set then commit:** Run `ultragit context set --task "T"`, then `git commit`, verify context is consumed.
- **Uninstall:** Run `ultragit init`, then `ultragit uninstall`, verify hooks are cleaned up, config is removed, existing hook content preserved.

### Edge Cases

- Repository with `core.hooksPath` set to a custom directory.
- Repository with `.git` as a file (worktree).
- Running `ultragit commit` outside a git repository.
- `pending-context.json` exists from a previous session (staleness).
- Very rapid sequential commits (context from commit N leaking to commit N+1 — prevented by read-and-delete).
- `ultragit commit --amend` with existing pending context.
- Commit with no changed files (`--allow-empty`).
- Hook script is not executable (chmod issue on some systems).

---

## Acceptance Criteria

1. `ultragit init` installs all three hooks, creates the notes ref, writes config, and reports status including credential check.
2. `ultragit init` correctly chains with existing hooks without overwriting them.
3. `ultragit init --no-hooks` skips hook installation.
4. `ultragit init --dry-run` shows what would happen without making changes.
5. `ultragit commit -m "msg" --task "T" --reasoning "R"` writes pending-context.json, runs `git commit`, and the post-commit hook consumes the context.
6. `ultragit context set --task "T"` writes pending-context.json, and the next `git commit` consumes it.
7. `pending-context.json` is deleted after being read by the post-commit hook, even if annotation fails.
8. ULTRAGIT_* environment variables are used as fallback when pending-context.json doesn't exist.
9. Pre-LLM filtering correctly skips lockfile-only, WIP, fixup, and below-threshold commits without making API calls.
10. Async mode returns control to the terminal immediately; annotation completes in the background.
11. In CI environments, annotation runs synchronously regardless of the `async` config setting.
12. File locking prevents race conditions on pending-context.json during concurrent operations.
13. `ultragit uninstall` cleanly removes Ultragit hooks and config while preserving existing hook content.
14. `ultragit init` detects hook managers (Husky, Lefthook) and warns the user.
