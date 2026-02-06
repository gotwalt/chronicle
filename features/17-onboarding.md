# Feature 17: Rapid Onboarding

## Overview

Chronicle has all the pieces (CLI, skills, hooks, annotation handler) but no streamlined path to get a new user from "binary installed" to "everything working." Skills and hooks live only in the Chronicle project's own `.claude/` directory — they're invisible to other repos. There's no `setup` command. The `init` command doesn't mention backfill or global setup. A new user has to manually copy files and read docs to get going.

This feature provides a **two-command onboarding flow**:

1. `git chronicle setup` — one-time machine-wide setup (LLM provider config, Claude Code skills, hooks, CLAUDE.md)
2. `git chronicle init` (enhanced) — per-repo, with optional backfill suggestion

Plus two supporting commands:

3. `git chronicle reconfigure` — rerun the LLM provider selection prompt
4. `git chronicle backfill` — CLI command to annotate historical commits

**Goal:** Under 60 seconds from binary install to first annotation (excluding backfill).

**How `git chronicle` works:** Git has a built-in extension convention — when you type `git chronicle setup`, git searches your PATH for an executable named `git-chronicle` and runs `git-chronicle setup`. Any binary named `git-<name>` on PATH automatically becomes available as `git <name>`. No registration or configuration is needed. So `cargo install chronicle` (which produces the `git-chronicle` binary in `~/.cargo/bin`) is sufficient — `git chronicle` works immediately, as long as `~/.cargo/bin` is on PATH.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 01 CLI & Config | CLI subcommand infrastructure |
| 06 Hooks & Context | Git hook installation patterns, marker-delimited chaining |
| 15 Claude Code Skills | Skill and hook content to distribute |

---

## Components

### 1. `git chronicle setup` Command

One-time, machine-wide setup. Installs everything a user needs to use Chronicle across all repos.

**What it does:**

1. **Verifies `git-chronicle` is on PATH** — runs `git-chronicle --version` and confirms the binary is accessible
2. **Prompts for LLM provider configuration** (interactive, with defaults):
   - `[1] Claude Code (recommended)` — spawns `claude -p` subprocess, uses existing Claude Code auth, no API key needed
   - `[2] Anthropic API key` — prompts for key or reads from `ANTHROPIC_API_KEY` env var
   - `[3] Skip` — configure later with `git chronicle reconfigure`
3. **Writes user-level config** to `~/.git-chronicle.toml`
4. **Creates Claude Code skills** at `~/.claude/skills/chronicle/{context,annotate,backfill}/SKILL.md`
5. **Creates Claude Code hooks** at `~/.claude/hooks/{post-tool-use/chronicle-annotate-reminder.sh, pre-tool-use/chronicle-read-context-hint.sh}`
6. **Appends a marker-delimited Chronicle section to `~/.claude/CLAUDE.md`** (idempotent — replaces on re-run)
7. **Prints summary** with next-step suggestion (`cd my-project && git chronicle init`)

**Flags:**

| Flag | Effect |
|------|--------|
| `--force` | Overwrite existing skills/hooks/config without prompting |
| `--dry-run` | Print what would be created/modified without writing |
| `--skip-skills` | Don't install Claude Code skills |
| `--skip-hooks` | Don't install Claude Code hooks |
| `--skip-claude-md` | Don't modify `~/.claude/CLAUDE.md` |

**Content source:** All skill, hook, and CLAUDE.md snippet files are embedded in the binary via `include_str!()` from an `embedded/` directory at project root. These are "distribution" versions — generic (no Chronicle-project-specific references), CLI-primary (no MCP dependency since Feature 12 isn't fully built).

**Hook naming:** Prefixed `chronicle-` (e.g., `chronicle-annotate-reminder.sh`) to avoid collision with other tools' hooks in the global directory.

**CLAUDE.md integration:** Uses `<!-- chronicle-setup-begin -->` / `<!-- chronicle-setup-end -->` markers. Creates file if absent, replaces section if markers exist, appends if no markers. Same idempotent marker pattern used by git hook chaining in `src/hooks/mod.rs`.

**Output example (normal run):**

```
Chronicle setup complete!

  Provider:    Claude Code (claude -p subprocess)
  Config:      ~/.git-chronicle.toml
  Skills:      ~/.claude/skills/chronicle/{context,annotate,backfill}/SKILL.md
  Hooks:       ~/.claude/hooks/post-tool-use/chronicle-annotate-reminder.sh
               ~/.claude/hooks/pre-tool-use/chronicle-read-context-hint.sh
  CLAUDE.md:   ~/.claude/CLAUDE.md (Chronicle section added)

Next: cd your-project && git chronicle init
```

**Output example (dry run):**

```
[dry-run] Would write ~/.git-chronicle.toml
[dry-run] Would create ~/.claude/skills/chronicle/context/SKILL.md
[dry-run] Would create ~/.claude/skills/chronicle/annotate/SKILL.md
[dry-run] Would create ~/.claude/skills/chronicle/backfill/SKILL.md
[dry-run] Would create ~/.claude/hooks/post-tool-use/chronicle-annotate-reminder.sh
[dry-run] Would create ~/.claude/hooks/pre-tool-use/chronicle-read-context-hint.sh
[dry-run] Would update ~/.claude/CLAUDE.md (add Chronicle section)
```

---

### 2. User-Level Config File (`~/.git-chronicle.toml`)

A new **user-level config file** separate from per-repo git config. Stores machine-wide preferences:

```toml
[provider]
type = "claude-code"   # "claude-code" | "anthropic" | "none"
model = "claude-sonnet-4-5-20250929"  # optional override

# Only populated if type = "anthropic"
# api_key_env = "ANTHROPIC_API_KEY"   # env var name to read key from
```

**Provider types:**

| Type | Description | Auth | Agent Loop |
|------|-------------|------|------------|
| `claude-code` | Spawns `claude -p` subprocess | Uses existing Claude Code auth, no API key | Full `LlmProvider` trait including multi-turn tool-use conversation loop via subprocess |
| `anthropic` | Direct Anthropic API calls | `ANTHROPIC_API_KEY` env var | Existing `AnthropicProvider` |
| `none` | No provider configured | N/A | Batch annotate errors; live path still works (no LLM needed) |

**`claude-code` provider details:**

Implements the full `LlmProvider` trait via `claude -p` subprocess:

- Spawns `claude` CLI with `--print` and `--output-format json` flags for structured JSON conversation
- Manages the agent loop's tool calls and responses through the subprocess
- Zero API key management — uses the user's existing Claude Code authentication
- Falls back gracefully if `claude` binary is not found

**Config precedence:** Per-repo git config (`chronicle.provider`, `chronicle.model`) overrides user-level config. This allows individual repos to use a different provider or model than the machine default.

**Validation during setup:**

- `claude-code`: Checks that `claude` binary is on PATH (runs `claude --version`)
- `anthropic`: Checks that `ANTHROPIC_API_KEY` env var is set (does not validate the key)
- `none`: No validation needed

---

### 3. `git chronicle reconfigure` Command

Reruns the LLM provider selection prompt from `setup`. Updates `~/.git-chronicle.toml`. Does **not** touch skills, hooks, or CLAUDE.md.

```
$ git chronicle reconfigure

Current provider: claude-code

Select LLM provider for batch annotation:
  [1] Claude Code (recommended) — uses existing Claude Code auth
  [2] Anthropic API key — uses ANTHROPIC_API_KEY env var
  [3] None — skip for now, live path still works

Choice [1]:
```

---

### 4. `git chronicle init` (Enhanced)

Current behavior is fully preserved. New additions run **after** existing init steps:

1. **Count unannotated commits** — walk last 100 commits on HEAD, check for notes under `refs/notes/chronicle`, report count
2. **Backfill suggestion** — if unannotated > 0:
   ```
   Found 47 unannotated commits. Run `git chronicle backfill --limit 20` to annotate recent history.
   ```
3. **Setup suggestion** — if `~/.claude/skills/chronicle/` directory is missing:
   ```
   TIP: Run `git chronicle setup` to install Claude Code skills globally.
   ```
4. **`--backfill` flag** — convenience flag that runs `backfill --limit 20` after init completes

**Enhanced output example:**

```
Chronicle initialized in /path/to/my-project

  Notes ref:    refs/notes/chronicle
  Post-commit:  .git/hooks/post-commit (chronicle section added)
  Config:       chronicle.initialized = true

  Found 47 unannotated commits (of last 100).
  Run `git chronicle backfill --limit 20` to annotate recent history.

  TIP: Run `git chronicle setup` to install Claude Code skills globally.
```

---

### 5. `git chronicle backfill` Command

Annotates historical commits that lack Chronicle annotations. Promotes the existing skill-based backfill workflow (Feature 15) to a first-class CLI command.

**Usage:**

```
git chronicle backfill [--limit N] [--dry-run]
```

| Flag | Default | Effect |
|------|---------|--------|
| `--limit N` | 20 | Maximum number of commits to annotate |
| `--dry-run` | false | List commits that would be annotated (with skip reasons) without calling the LLM |

**Workflow:**

1. Walk commits on HEAD from most recent to oldest, up to `--limit`
2. Check each commit for an existing note under `refs/notes/chronicle`
3. **Pre-filter** unannotated commits before sending to LLM:
   - Reuse `annotate::filter::pre_llm_filter()` to skip merge commits, WIP commits, lockfile-only changes, trivial changes
   - **New:** Skip commits that only touch binary files (images, compiled artifacts, etc.)
   - **New:** Skip commits that only touch generated/unparsable files (`.min.js`, vendored deps, lockfiles beyond the existing set)
4. For each commit passing the filter, run the batch annotate agent
5. Print progress: `[3/20] abc1234 Add error handling to provider`

**Pre-filter extensions to `annotate::filter`:**

```rust
/// Binary file extensions that indicate non-code content.
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg",
    "woff", "woff2", "ttf", "eot",
    "pdf", "zip", "tar", "gz", "bz2",
    "exe", "dll", "so", "dylib",
    "pyc", "class", "o", "obj",
];

/// Generated/vendored file patterns that aren't worth annotating.
const GENERATED_PATTERNS: &[&str] = &[
    ".min.js", ".min.css",
    "vendor/", "vendored/",
    "node_modules/",
    ".generated.", "_generated.",
    "dist/", "build/",
];
```

These extensions integrate with the existing `pre_llm_filter()` function as new `FilterDecision::Skip` branches.

**Provider requirement:** Uses the configured provider from `~/.git-chronicle.toml` (`claude-code` or `anthropic`). Errors if provider is `none`:

```
Error: No LLM provider configured. Run `git chronicle setup` or `git chronicle reconfigure` to select a provider.
```

**`init --backfill` sugar:** Runs `backfill --limit 20` after init completes. If the provider is `none`, prints a warning but doesn't fail the init.

**Dry-run output example:**

```
$ git chronicle backfill --limit 10 --dry-run

Scanning last 10 commits on HEAD...

  ANNOTATE  abc1234  Add error handling to provider (5 files, 120 lines)
  SKIP      def5678  Merge branch 'feature' into main (merge commit)
  SKIP      ghi9012  WIP checkpoint (WIP commit)
  ANNOTATE  jkl3456  Refactor AST anchor resolution (3 files, 85 lines)
  SKIP      mno7890  Update Cargo.lock (lockfile-only)
  ANNOTATE  pqr1234  Add backfill skill content (2 files, 45 lines)
  SKIP      stu5678  Fix typo in README (trivial: 1 line)
  SKIP      vwx9012  Add logo.png (binary-only)
  ANNOTATE  yza3456  Implement corrections schema (4 files, 200 lines)
  SKIP      bcd7890  Vendor updated JS deps (generated/vendored)

Would annotate 4 of 10 commits (6 skipped).
```

---

## End-to-End Flow

```
cargo install chronicle                     # binary on PATH
git chronicle setup                         # global: provider, skills, hooks, CLAUDE.md
cd my-project && git chronicle init         # per-repo: hooks, notes ref, config
git chronicle backfill --limit 20           # optional: annotate recent history
# ... start coding, annotations happen automatically via hooks
```

**Alternative: fast path with backfill:**

```
cargo install chronicle
git chronicle setup
cd my-project && git chronicle init --backfill
```

---

## Embedded Content (`embedded/` Directory)

All files installed by `setup` are embedded in the binary via `include_str!()`. This means:

- No network dependency during setup
- Content is versioned with the binary
- `setup` always installs content matching the binary version

**Directory structure:**

```
embedded/
├── skills/
│   ├── context/
│   │   └── SKILL.md
│   ├── annotate/
│   │   └── SKILL.md
│   └── backfill/
│       └── SKILL.md
├── hooks/
│   ├── chronicle-annotate-reminder.sh
│   └── chronicle-read-context-hint.sh
└── claude-md-snippet.md
```

**Content differences from project-local versions:**

The embedded (distribution) versions differ from the project-local versions in `.claude/`:

| Aspect | Project-local (`.claude/`) | Embedded (`embedded/`) |
|--------|--------------------------|----------------------|
| MCP tools | Primary method | CLI-primary (MCP as optional enhancement) |
| References | Chronicle-specific paths | Generic paths (`src/path/to/file.rs`) |
| Complexity | Full detail | Slightly simplified for general use |
| Hook prefix | `annotate-reminder.sh` | `chronicle-annotate-reminder.sh` |

The embedded content is derived from the project-local content but adapted for distribution to any repository.

---

## New Source Files

### `src/setup/mod.rs` — Setup Orchestration

Coordinates the full `setup` flow:

```rust
pub struct SetupOptions {
    pub force: bool,
    pub dry_run: bool,
    pub skip_skills: bool,
    pub skip_hooks: bool,
    pub skip_claude_md: bool,
}

pub fn run_setup(options: &SetupOptions) -> Result<SetupReport, SetupError> {
    verify_binary_on_path()?;
    let provider_config = prompt_provider_selection()?;
    write_user_config(&provider_config, options)?;
    if !options.skip_skills {
        install_skills(options)?;
    }
    if !options.skip_hooks {
        install_hooks(options)?;
    }
    if !options.skip_claude_md {
        update_claude_md(options)?;
    }
    Ok(SetupReport { /* ... */ })
}
```

Key functions:

- `verify_binary_on_path()` — runs `git-chronicle --version`, returns error if not found
- `prompt_provider_selection()` — interactive TTY prompt with 3 choices
- `write_user_config()` — serializes `UserConfig` to `~/.git-chronicle.toml`
- `install_skills()` — writes embedded skill files to `~/.claude/skills/chronicle/`
- `install_hooks()` — writes embedded hook files to `~/.claude/hooks/`, sets executable bit
- `update_claude_md()` — marker-delimited insert/replace in `~/.claude/CLAUDE.md`

### `src/setup/embedded.rs` — Embedded Content

```rust
pub const SKILL_CONTEXT: &str = include_str!("../../embedded/skills/context/SKILL.md");
pub const SKILL_ANNOTATE: &str = include_str!("../../embedded/skills/annotate/SKILL.md");
pub const SKILL_BACKFILL: &str = include_str!("../../embedded/skills/backfill/SKILL.md");
pub const HOOK_ANNOTATE_REMINDER: &str =
    include_str!("../../embedded/hooks/chronicle-annotate-reminder.sh");
pub const HOOK_READ_CONTEXT_HINT: &str =
    include_str!("../../embedded/hooks/chronicle-read-context-hint.sh");
pub const CLAUDE_MD_SNIPPET: &str = include_str!("../../embedded/claude-md-snippet.md");
```

### `src/config/user_config.rs` — User Config Load/Save

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct UserConfig {
    pub provider: ProviderConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderType {
    ClaudeCode,
    Anthropic,
    None,
}

impl UserConfig {
    pub fn load() -> Result<Option<Self>, UserConfigError> { /* ... */ }
    pub fn save(&self) -> Result<(), UserConfigError> { /* ... */ }
    pub fn path() -> Result<PathBuf, UserConfigError> { /* ... */ }
}
```

Uses `toml` crate for serialization. `load()` returns `Ok(None)` if file doesn't exist.

### `src/provider/claude_code.rs` — Claude Code Provider

Implements `LlmProvider` by spawning `claude -p` as a subprocess:

```rust
pub struct ClaudeCodeProvider {
    model: Option<String>,
}

impl LlmProvider for ClaudeCodeProvider {
    fn complete(&self, messages: &[Message], tools: &[ToolDef]) -> Result<Response, ProviderError> {
        // Spawn: claude --print --output-format json
        // Write messages as structured JSON to stdin
        // Parse structured JSON response from stdout
        // Map tool_use blocks to Response tool calls
    }
}
```

Key implementation details:

- Spawns `claude` CLI with `--print` and `--output-format json`
- Sends the full message history (system prompt + messages + tool definitions) as structured input
- Parses JSON output for assistant messages and tool-use blocks
- Manages multi-turn conversation by accumulating messages across subprocess invocations
- Falls back with clear error if `claude` binary not found: `"Claude CLI not found. Install Claude Code or run 'git chronicle reconfigure' to select a different provider."`

### `src/provider/mod.rs` — Updated Provider Discovery

Update `discover_provider()` to read user-level config:

```rust
pub fn discover_provider() -> Result<Box<dyn LlmProvider>, ProviderError> {
    // 1. Check per-repo git config (chronicle.provider, chronicle.model)
    // 2. Fall back to user-level config (~/.git-chronicle.toml)
    // 3. Fall back to env var detection (ANTHROPIC_API_KEY)
    // 4. Error: no provider configured
}
```

### `src/cli/setup.rs` — Setup CLI Subcommand

```rust
#[derive(Parser)]
pub struct SetupArgs {
    /// Overwrite existing files without prompting
    #[arg(long)]
    force: bool,

    /// Print what would be done without writing
    #[arg(long)]
    dry_run: bool,

    /// Skip installing Claude Code skills
    #[arg(long)]
    skip_skills: bool,

    /// Skip installing Claude Code hooks
    #[arg(long)]
    skip_hooks: bool,

    /// Skip modifying ~/.claude/CLAUDE.md
    #[arg(long)]
    skip_claude_md: bool,
}
```

### `src/cli/reconfigure.rs` — Reconfigure CLI Subcommand

```rust
#[derive(Parser)]
pub struct ReconfigureArgs {}
```

Reads existing `~/.git-chronicle.toml`, shows current provider, prompts for new selection, writes updated config.

### `src/cli/backfill.rs` — Backfill CLI Subcommand

```rust
#[derive(Parser)]
pub struct BackfillArgs {
    /// Maximum number of commits to annotate
    #[arg(long, default_value = "20")]
    limit: usize,

    /// List commits that would be annotated without calling the LLM
    #[arg(long)]
    dry_run: bool,
}
```

---

## Error Handling

New error variants in the error hierarchy:

```rust
#[derive(Debug, Snafu)]
#[snafu(module(setup_error))]
pub enum SetupError {
    #[snafu(display("Home directory not found, at {location}"))]
    NoHomeDirectory {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("git-chronicle binary not found on PATH, at {location}"))]
    BinaryNotFound {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to write {path}: {source}, at {location}"))]
    WriteFile {
        path: String,
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to read user config: {source}, at {location}"))]
    ReadConfig {
        #[snafu(source)]
        source: toml::de::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to write user config: {source}, at {location}"))]
    WriteConfig {
        #[snafu(source)]
        source: toml::ser::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Claude CLI not found — install Claude Code or select a different provider, at {location}"))]
    ClaudeCliNotFound {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("ANTHROPIC_API_KEY environment variable not set, at {location}"))]
    ApiKeyNotSet {
        #[snafu(implicit)]
        location: Location,
    },
}
```

```rust
#[derive(Debug, Snafu)]
#[snafu(module(backfill_error))]
pub enum BackfillError {
    #[snafu(display("No LLM provider configured — run `git chronicle setup` or `git chronicle reconfigure`, at {location}"))]
    NoProvider {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to annotate commit {sha}: {source}, at {location}"))]
    AnnotateCommit {
        sha: String,
        #[snafu(source)]
        source: Box<dyn std::error::Error + Send + Sync>,
        #[snafu(implicit)]
        location: Location,
    },
}
```

---

## CLAUDE.md Marker Content

The embedded `claude-md-snippet.md` content installed by `setup`:

```markdown
<!-- chronicle-setup-begin -->
## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

Use the CLI to check annotations on code you're about to modify:

- `git chronicle read src/foo.rs --anchor bar_function` — get intent,
  reasoning, constraints for a specific function
- `git chronicle deps src/foo.rs bar_function` — find code that depends
  on this function's behavior
- `git chronicle summary src/foo.rs` — overview of all annotated regions

**Respect constraints.** Annotations may include constraints like "must not
allocate" or "assumes sorted input." Violating these without updating the
annotation is a bug.

### Writing annotations (after committing)

After committing, annotate using the live path:

```bash
echo '<AnnotateInput JSON>' | git chronicle annotate --live
```

See the annotate skill (`~/.claude/skills/chronicle/annotate/SKILL.md`) for
the full annotation workflow.

### Backfilling annotations

To annotate historical commits that lack annotations:

```bash
git chronicle backfill --limit 20
```

See the backfill skill (`~/.claude/skills/chronicle/backfill/SKILL.md`) for
the full backfill workflow.
<!-- chronicle-setup-end -->
```

Note: This is the CLI-primary version (no MCP tool references). The project-local CLAUDE.md in the Chronicle repo itself references MCP tools because it has the full MCP integration.

---

## Doctor Integration

Add a `check_global_setup()` check to `src/doctor.rs`:

```
$ git chronicle doctor

  [OK] Git repository detected
  [OK] refs/notes/chronicle exists
  [OK] Post-commit hook installed
  ...
  [OK] Global setup: ~/.git-chronicle.toml exists
  [OK] Provider configured: claude-code
  [OK] Claude Code skills installed (~/.claude/skills/chronicle/)
  [WARN] Claude Code hooks not installed (~/.claude/hooks/) — run `git chronicle setup`
```

Checks:

1. `~/.git-chronicle.toml` exists and is valid TOML
2. Provider is configured (not `none`)
3. If provider is `claude-code`, verify `claude` binary is on PATH
4. If provider is `anthropic`, verify `ANTHROPIC_API_KEY` env var is set
5. `~/.claude/skills/chronicle/` directory exists with expected skill files
6. `~/.claude/hooks/` contains Chronicle hook files
7. `~/.claude/CLAUDE.md` contains Chronicle markers

---

## Implementation Steps

### Step 1: Embedded Content Directory

**Scope:** `embedded/` directory

Create distribution versions of all skill, hook, and CLAUDE.md snippet files. Derive from existing project-local versions in `.claude/` but adapt for distribution:

- Replace MCP-primary instructions with CLI-primary
- Remove Chronicle-project-specific file paths
- Prefix hook filenames with `chronicle-`

**Deliverables:** `embedded/skills/{context,annotate,backfill}/SKILL.md`, `embedded/hooks/chronicle-{annotate-reminder,read-context-hint}.sh`, `embedded/claude-md-snippet.md`

### Step 2: Setup Module

**Scope:** `src/setup/mod.rs`, `src/setup/embedded.rs`

Implement the `include_str!()` embedded content and the full `run_setup()` orchestration:

- Binary verification
- Interactive provider prompt (reads from stdin/TTY)
- File writing with dry-run and force support
- CLAUDE.md marker-delimited insertion (create/replace/append logic)
- Summary output

**Deliverables:** `run_setup()`, `verify_binary_on_path()`, `prompt_provider_selection()`, `install_skills()`, `install_hooks()`, `update_claude_md()`

### Step 3: User Config Module

**Scope:** `src/config/user_config.rs`

TOML-based load/save for `~/.git-chronicle.toml`:

- `UserConfig` struct with `ProviderConfig`
- `ProviderType` enum (`ClaudeCode`, `Anthropic`, `None`)
- `load()` / `save()` / `path()` methods
- Graceful handling of missing file (`Ok(None)`)

**Deliverables:** `UserConfig`, `ProviderConfig`, `ProviderType`, load/save/path functions

### Step 4: Claude Code Provider

**Scope:** `src/provider/claude_code.rs`

Implement `ClaudeCodeProvider` that wraps the `claude` CLI:

- Spawn `claude --print --output-format json` subprocess
- Send structured message history on stdin
- Parse JSON response from stdout
- Map tool-use blocks to `LlmProvider` response types
- Handle subprocess errors, timeouts, missing binary

**Deliverables:** `ClaudeCodeProvider` implementing `LlmProvider`

### Step 5: Provider Discovery Update

**Scope:** `src/provider/mod.rs`

Update `discover_provider()` to read user-level config:

- Check per-repo git config first (`chronicle.provider`)
- Fall back to `~/.git-chronicle.toml`
- Fall back to env var detection (`ANTHROPIC_API_KEY`)
- Return appropriate provider or error

**Deliverables:** Updated `discover_provider()` with config file support

### Step 6: Setup CLI Subcommand

**Scope:** `src/cli/setup.rs`, `src/cli/mod.rs`, `src/main.rs`

Wire `Setup` subcommand into clap:

- Parse flags (`--force`, `--dry-run`, `--skip-skills`, `--skip-hooks`, `--skip-claude-md`)
- Call `setup::run_setup()`
- Format and print the report

**Deliverables:** `SetupArgs`, subcommand dispatch

### Step 7: Reconfigure CLI Subcommand

**Scope:** `src/cli/reconfigure.rs`

Wire `Reconfigure` subcommand:

- Load existing `~/.git-chronicle.toml`
- Show current provider
- Rerun provider selection prompt
- Save updated config

**Deliverables:** `ReconfigureArgs`, subcommand dispatch

### Step 8: Backfill CLI Subcommand

**Scope:** `src/cli/backfill.rs`

Wire `Backfill` subcommand:

- Walk commits on HEAD up to `--limit`
- Check for existing notes
- Run pre-filter (extended with binary/generated file detection)
- In normal mode: run batch annotate agent on each passing commit, print progress
- In dry-run mode: list commits with annotate/skip decisions and reasons

**Deliverables:** `BackfillArgs`, commit iteration, filter integration, progress output

### Step 9: Extended Pre-LLM Filtering

**Scope:** `src/annotate/filter.rs`

Add binary file and generated/vendored file detection to existing `pre_llm_filter()`:

- `BINARY_EXTENSIONS` constant
- `GENERATED_PATTERNS` constant
- New `FilterDecision::Skip` branches for binary-only and generated-only commits

**Deliverables:** Extended `pre_llm_filter()`, new constants, new tests

### Step 10: Enhanced `init`

**Scope:** `src/cli/init.rs`

Add post-init enhancements:

- Count unannotated commits (walk last 100 on HEAD)
- Print backfill suggestion if unannotated > 0
- Print setup suggestion if `~/.claude/skills/chronicle/` missing
- `--backfill` flag that runs `backfill --limit 20` after init

**Deliverables:** `--backfill` flag, unannotated commit counting, suggestion messages

### Step 11: Error Variants

**Scope:** `src/setup/mod.rs`, `src/cli/backfill.rs`

Add `SetupError` and `BackfillError` types following snafu conventions:

- All variants include `location` field
- Messages end with `, at {location}`
- Leaf errors linked with `source`

**Deliverables:** `SetupError`, `BackfillError` with full snafu integration

### Step 12: Doctor Integration

**Scope:** `src/doctor.rs`

Add `check_global_setup()`:

- Verify `~/.git-chronicle.toml` exists and parses
- Verify provider is configured and reachable
- Verify skills directory exists
- Verify hooks exist
- Verify CLAUDE.md markers

**Deliverables:** `check_global_setup()` integrated into doctor output

---

## Test Plan

### Unit Tests

**CLAUDE.md marker logic** (`src/setup/mod.rs`):

- Insert into empty file → file contains markers with content
- Insert into file with no markers → markers appended at end
- Replace existing markers → content between markers replaced, surrounding content preserved
- Idempotent: run twice → same result
- Markers inside code blocks are not matched (false positive prevention)

**User config roundtrip** (`src/config/user_config.rs`):

- Serialize `UserConfig` → deserialize → equal
- `ProviderType::ClaudeCode` serializes as `"claude-code"`
- `ProviderType::Anthropic` serializes as `"anthropic"`
- `ProviderType::None` serializes as `"none"`
- Missing file → `load()` returns `Ok(None)`
- Invalid TOML → `load()` returns appropriate error

**Pre-filter extensions** (`src/annotate/filter.rs`):

- Commit with only `.png` files → `Skip("binary-only")`
- Commit with only `.min.js` files → `Skip("generated/vendored")`
- Commit with mix of binary and code files → `Annotate`
- Commit with only `vendor/` paths → `Skip("generated/vendored")`

**Backfill commit selection**:

- Already-annotated commits are skipped
- Merge commits are skipped
- WIP commits are skipped
- Lockfile-only commits are skipped
- Normal code commits are selected

### Integration Tests

**Full setup in temp HOME** (`tests/integration/setup_test.rs`):

- Set `HOME` to a temp directory
- Run `setup` with `--force` (skip interactive prompt by providing config directly)
- Verify all files created at expected paths
- Verify file contents match embedded content
- Verify CLAUDE.md has markers
- Run `setup` again → verify idempotent (files unchanged or correctly replaced)
- Run with `--dry-run` → verify no files written

**Backfill with mock provider**:

- Create a temp git repo with 5 commits (mix of code, lockfile, merge)
- Run `backfill --dry-run` → verify correct annotate/skip decisions
- Run `backfill` with a mock provider → verify annotations written for non-skipped commits
- Verify already-annotated commits are not re-annotated

**Init enhancements**:

- Create a temp repo with unannotated commits → run `init` → verify suggestion printed
- Create a temp repo with all annotated commits → run `init` → verify no suggestion
- Remove `~/.claude/skills/chronicle/` → run `init` → verify setup suggestion printed

---

## Acceptance Criteria

1. `git chronicle setup` installs skills, hooks, CLAUDE.md snippet, and provider config in a single command with no network dependency.
2. `git chronicle setup --dry-run` prints all actions without writing any files.
3. `git chronicle setup` is idempotent — running it twice produces the same result (CLAUDE.md markers replaced, not duplicated).
4. `git chronicle setup --force` overwrites existing files without prompting.
5. Installed skill files are CLI-primary (no MCP tool dependency) and work for any repository.
6. Installed hook files have the executable bit set and are prefixed with `chronicle-`.
7. `git chronicle reconfigure` updates the provider in `~/.git-chronicle.toml` without touching skills or hooks.
8. `git chronicle init` reports the count of unannotated commits and suggests `backfill`.
9. `git chronicle init` suggests `setup` when global skills are not installed.
10. `git chronicle init --backfill` runs `backfill --limit 20` after init completes.
11. `git chronicle backfill --dry-run` lists commits with annotate/skip decisions and reasons.
12. `git chronicle backfill` skips binary-only, generated-only, merge, WIP, lockfile-only, and trivial commits.
13. `git chronicle backfill` errors clearly when no LLM provider is configured.
14. `git chronicle doctor` checks for global setup (config, provider, skills, hooks, CLAUDE.md markers).
15. The `ClaudeCodeProvider` implements `LlmProvider` via `claude -p` subprocess with zero API key management.
16. Per-repo git config (`chronicle.provider`) overrides user-level config.
17. The full onboarding path (`setup` → `init` → first annotation) completes in under 60 seconds excluding backfill.
18. All embedded content is compiled into the binary via `include_str!()` — no runtime file dependencies.
