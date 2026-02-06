# Feature 01: CLI Framework & Configuration

## Overview

The CLI framework is the foundation of Chronicle. It defines the binary's entry point, all subcommand definitions, argument parsing, configuration loading with proper precedence, and error output formatting. Every other feature plugs into the structure defined here.

This feature produces a compilable `chronicle` binary with all subcommands wired up to stub handlers that return `unimplemented!()` or placeholder output. The configuration system is fully functional — later features consume it, they don't build it.

---

## Dependencies

None. This is the root of the dependency graph.

---

## Public API

### CLI Structure

The binary uses `clap` with derive macros. The top-level command:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "chronicle", version, about = "Semantic memory for codebases")]
pub struct Cli {
    /// Output format for machine-consumed commands
    #[arg(long, global = true, default_value = "auto")]
    pub format: OutputFormat,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all non-error output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// JSON for agent consumption, human-readable for interactive use
    Auto,
    /// Always JSON on stdout
    Json,
    /// Structured markdown — token-efficient for LLM consumption (default for read commands)
    Markdown,
    /// Always human-readable on stderr
    Pretty,
}
```

### Subcommand Definitions

```rust
#[derive(Subcommand)]
pub enum Command {
    /// Initialize Chronicle in the current repository
    Init(InitArgs),

    /// Commit with annotation context
    Commit(CommitArgs),

    /// Set context for the next commit
    Context(ContextArgs),

    /// Annotate a commit (called by hooks, can be run manually)
    Annotate(AnnotateArgs),

    /// Read annotations for a file or code region
    Read(ReadArgs),

    /// Query semantic dependencies on a code region
    Deps(DepsArgs),

    /// Show annotation timeline for a code region
    History(HistoryArgs),

    /// Condensed overview of annotations in a file or directory
    Summary(SummaryArgs),

    /// Show raw annotation for a commit
    Inspect(InspectArgs),

    /// Flag an annotation as inaccurate
    Flag(FlagArgs),

    /// Correct a specific annotation field
    Correct(CorrectArgs),

    /// Validate installation and configuration
    Doctor(DoctorArgs),

    /// Manage notes sync with remotes
    Sync(SyncArgs),

    /// Export annotations to portable JSON
    Export(ExportArgs),

    /// Import annotations from portable JSON
    Import(ImportArgs),

    /// Manage agent skill definitions
    Skill(SkillArgs),

    /// Manage LLM credentials
    Auth(AuthArgs),

    /// Read or write configuration
    Config(ConfigArgs),

    /// Manage MCP server
    Mcp(McpArgs),

    /// Annotate historical commits
    Backfill(BackfillArgs),
}
```

### Key Subcommand Args

Each subcommand struct defines its flags and arguments. The full set below; later feature specs reference these definitions.

```rust
#[derive(clap::Args)]
pub struct InitArgs {
    /// LLM provider to use
    #[arg(long)]
    pub provider: Option<String>,

    /// LLM model to use
    #[arg(long)]
    pub model: Option<String>,

    /// Run annotations synchronously (block on commit)
    #[arg(long)]
    pub sync: bool,

    /// File patterns to include
    #[arg(long, action = clap::ArgAction::Append)]
    pub include: Vec<String>,

    /// File patterns to exclude
    #[arg(long, action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Skip hook installation
    #[arg(long)]
    pub no_hooks: bool,

    /// Show what would be done without doing it
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(clap::Args)]
pub struct CommitArgs {
    /// Commit message (passed through to git commit)
    #[arg(short, long)]
    pub message: Option<String>,

    /// Task identifier or description
    #[arg(long)]
    pub task: Option<String>,

    /// Reasoning behind the changes
    #[arg(long)]
    pub reasoning: Option<String>,

    /// Semantic dependencies the author is aware of
    #[arg(long)]
    pub dependencies: Option<String>,

    /// Comma-separated tags
    #[arg(long)]
    pub tags: Option<String>,

    /// All remaining args passed through to `git commit`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

#[derive(clap::Args)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub action: ContextAction,
}

#[derive(Subcommand)]
pub enum ContextAction {
    /// Set context for the next commit
    Set(ContextSetArgs),
    /// Clear pending context
    Clear,
    /// Show pending context
    Show,
}

#[derive(clap::Args)]
pub struct ContextSetArgs {
    #[arg(long)]
    pub task: Option<String>,
    #[arg(long)]
    pub reasoning: Option<String>,
    #[arg(long)]
    pub dependencies: Option<String>,
    #[arg(long)]
    pub tags: Option<String>,
}

#[derive(clap::Args)]
pub struct AnnotateArgs {
    /// Commit SHA to annotate (default: HEAD)
    #[arg(long, default_value = "HEAD")]
    pub commit: String,

    /// Run asynchronously (default in hook mode)
    #[arg(long, name = "async")]
    pub async_mode: bool,

    /// Run synchronously (block until complete)
    #[arg(long)]
    pub sync: bool,

    /// Source commits for squash synthesis
    #[arg(long)]
    pub squash_sources: Option<String>,
}

#[derive(clap::Args)]
pub struct ReadArgs {
    /// File path(s) to read annotations for
    #[arg(required = true)]
    pub paths: Vec<String>,

    /// Named AST anchor (function, struct, etc.)
    #[arg()]
    pub anchor: Option<String>,

    /// Restrict to a line range (START:END)
    #[arg(long)]
    pub lines: Option<String>,

    /// Filter by date or commit SHA
    #[arg(long)]
    pub since: Option<String>,

    /// Filter by tags
    #[arg(long)]
    pub tags: Option<String>,

    /// Filter by context level
    #[arg(long)]
    pub context_level: Option<String>,

    /// Minimum confidence threshold (0.0-1.0)
    #[arg(long, default_value = "0.0")]
    pub min_confidence: f64,

    /// Hops of related annotations to follow
    #[arg(long, default_value = "1")]
    pub depth: u32,

    /// Max region annotations to return
    #[arg(long, default_value = "20")]
    pub max_regions: u32,

    /// Output format: markdown (default, token-efficient for LLMs), json (programmatic), pretty (human)
    #[arg(long, default_value = "markdown")]
    pub format: ReadOutputFormat,

    /// Target max token count for output
    #[arg(long)]
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReadOutputFormat {
    /// Structured markdown — token-efficient for LLM consumption (default)
    Markdown,
    /// JSON for programmatic access
    Json,
    /// Human-readable formatted output for debugging
    Pretty,
}

#[derive(clap::Args)]
pub struct DepsArgs {
    /// File path
    pub path: String,

    /// Named AST anchor
    pub anchor: Option<String>,
}

#[derive(clap::Args)]
pub struct HistoryArgs {
    /// File path
    pub path: String,

    /// Named AST anchor
    pub anchor: Option<String>,

    /// Max entries to return
    #[arg(long, default_value = "10")]
    pub limit: u32,
}

#[derive(clap::Args)]
pub struct SummaryArgs {
    /// File or directory path
    pub path: String,

    /// Named AST anchor
    pub anchor: Option<String>,
}

#[derive(clap::Args)]
pub struct InspectArgs {
    /// Commit SHA (default: HEAD)
    #[arg(default_value = "HEAD")]
    pub commit: String,

    /// Pretty-print the output
    #[arg(long)]
    pub pretty: bool,
}

#[derive(clap::Args)]
pub struct FlagArgs {
    /// File path
    pub path: String,

    /// Named AST anchor
    pub anchor: Option<String>,

    /// Reason for flagging
    #[arg(long, required = true)]
    pub reason: String,
}

#[derive(clap::Args)]
pub struct CorrectArgs {
    /// Commit SHA of the annotation to correct
    pub sha: String,

    /// Region anchor to correct
    #[arg(long)]
    pub region: String,

    /// Field to correct
    #[arg(long)]
    pub field: String,

    /// Value to remove
    #[arg(long)]
    pub remove: Option<String>,

    /// Value to add
    #[arg(long)]
    pub add: Option<String>,
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    /// Only check specific components
    #[arg(long)]
    pub check: Option<String>,
}

#[derive(clap::Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub action: SyncAction,
}

#[derive(Subcommand)]
pub enum SyncAction {
    /// Enable notes sync
    Enable,
    /// Disable notes sync
    Disable,
    /// Show sync status
    Status,
}

#[derive(clap::Args)]
pub struct ExportArgs {
    /// Restrict export to paths
    #[arg(long, action = clap::ArgAction::Append)]
    pub path: Vec<String>,
}

#[derive(clap::Args)]
pub struct ImportArgs {
    /// JSON file to import
    pub file: String,
}

#[derive(clap::Args)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub action: SkillAction,
}

#[derive(Subcommand)]
pub enum SkillAction {
    /// Install skill definition for a target
    Install(SkillInstallArgs),
    /// Export raw skill definition
    Export(SkillExportArgs),
    /// Check installed skills
    Check,
}

#[derive(clap::Args)]
pub struct SkillInstallArgs {
    /// Target agent framework
    #[arg(long)]
    pub target: String,

    /// Install globally
    #[arg(long)]
    pub global: bool,
}

#[derive(clap::Args)]
pub struct SkillExportArgs {
    /// Output file path
    #[arg(long)]
    pub output: Option<String>,
}

#[derive(clap::Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: AuthAction,
}

#[derive(Subcommand)]
pub enum AuthAction {
    /// Check credential status
    Check,
}

#[derive(clap::Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Get a config value
    Get(ConfigGetArgs),
    /// Set a config value
    Set(ConfigSetArgs),
    /// List all config values
    List,
}

#[derive(clap::Args)]
pub struct ConfigGetArgs {
    pub key: String,
}

#[derive(clap::Args)]
pub struct ConfigSetArgs {
    pub key: String,
    pub value: String,
}

#[derive(clap::Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub action: McpAction,
}

#[derive(Subcommand)]
pub enum McpAction {
    /// Install MCP server registration
    Install,
    /// Start MCP server (called by agent framework)
    Serve,
}

#[derive(clap::Args)]
pub struct BackfillArgs {
    /// Max commits to backfill
    #[arg(long)]
    pub limit: Option<u32>,

    /// Only commits since this date or SHA
    #[arg(long)]
    pub since: Option<String>,

    /// Only commits touching these paths
    #[arg(long, action = clap::ArgAction::Append)]
    pub path: Vec<String>,

    /// Concurrent API calls
    #[arg(long, default_value = "4")]
    pub concurrency: u32,

    /// Override model for backfill
    #[arg(long)]
    pub model: Option<String>,

    /// Show what would be annotated
    #[arg(long)]
    pub dry_run: bool,

    /// Resume a previously interrupted backfill
    #[arg(long)]
    pub resume: bool,
}
```

---

## Internal Design

### Configuration System

Configuration comes from three sources with strict precedence:

```
CLI flags > .git/config [chronicle] section > .chronicle-config.toml > compiled defaults
```

#### Resolved Config Type

All configuration resolves into a single struct:

```rust
#[derive(Debug, Clone)]
pub struct ChronicleConfig {
    pub enabled: bool,
    pub async_mode: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub backfill_model: Option<String>,
    pub note_ref: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub max_diff_lines: u32,
    pub skip_trivial: bool,
    pub trivial_threshold: u32,
    pub auto_sync: bool,
}

impl Default for ChronicleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            async_mode: true,
            provider: None,
            model: None,
            backfill_model: None,
            note_ref: "refs/notes/chronicle".into(),
            include: vec![],
            exclude: vec![],
            max_diff_lines: 2000,
            skip_trivial: true,
            trivial_threshold: 3,
            auto_sync: false,
        }
    }
}
```

#### `.git/config` Integration

The `[chronicle]` section in `.git/config`:

```ini
[chronicle]
    enabled = true
    async = true
    provider = anthropic
    model = claude-sonnet-4-5-20250929
    noteref = refs/notes/chronicle
    include = src/**,lib/**
    exclude = tests/**,*.generated.*
    maxDiffLines = 2000
    skipTrivial = true
    trivialThreshold = 3
```

Read via `gix` config parsing or `git config --get chronicle.<key>` fallback. Written via `gix` or `git config chronicle.<key> <value>`.

#### `.chronicle-config.toml` Parsing

Shared team config checked into the repository root:

```toml
[chronicle]
enabled = true
async = true

[chronicle.model]
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
backfill_model = "claude-haiku-4-5-20251001"

[chronicle.scope]
include = ["src/**", "lib/**", "config/**"]
exclude = ["*.generated.*", "vendor/**", "node_modules/**"]
max_diff_lines = 2000

[chronicle.sync]
auto_sync = true
```

Parsed with the `toml` crate into a `SharedConfig` struct, then merged into the base layer before git config overrides are applied.

#### Config Resolution Flow

```
fn load_config(cli: &Cli, repo_root: &Path) -> Result<ChronicleConfig> {
    let mut config = ChronicleConfig::default();

    // Layer 1: .chronicle-config.toml (if present)
    if let Some(shared) = load_shared_config(repo_root)? {
        config.merge_shared(shared);
    }

    // Layer 2: .git/config [chronicle] section
    if let Some(git) = load_git_config(repo_root)? {
        config.merge_git(git);
    }

    // Layer 3: CLI flags (applied per-command in handlers)
    // Each command handler merges its specific flags.

    Ok(config)
}
```

CLI flags are not merged generically because they are command-specific. Each subcommand handler takes the resolved config and overrides fields from its own args. For example, `AnnotateArgs.sync` overrides `config.async_mode`.

### Error Output Formatting

Chronicle outputs to two channels:

- **stdout** — structured data (markdown or JSON). Consumed by agents and scripts.
- **stderr** — human-readable status, progress, errors. Uses color when stderr is a TTY.

The `OutputFormat` enum controls behavior:

- `Auto` — detect if stdout is a TTY. If yes, use human-readable formatting on stdout. If no (piped), use markdown on stdout for read commands, JSON for other agent-facing commands. Errors always go to stderr.
- `Json` — always JSON on stdout. Errors as JSON on stderr.
- `Markdown` — structured markdown on stdout. Token-efficient for LLM consumption. Default for read commands (`read`, `deps`, `history`, `summary`).
- `Pretty` — always human-readable on stdout. Same as interactive.

Agent-facing read commands (`read`, `deps`, `history`, `summary`) default to markdown on stdout — this is more token-efficient for LLM consumption. Other agent-facing commands (`inspect`, `export`) default to JSON on stdout. Interactive commands (`init`, `doctor`, `status`, `config`) default to human-readable on stderr. Any command can override with `--format json` when programmatic parsing is needed.

```rust
pub struct Output {
    format: OutputFormat,
    is_tty: bool,
}

impl Output {
    /// Emit structured data (JSON) to stdout
    pub fn data<T: serde::Serialize>(&self, value: &T) -> Result<()>;

    /// Emit a human-readable status message to stderr
    pub fn status(&self, msg: &str);

    /// Emit a success indicator to stderr
    pub fn success(&self, msg: &str);

    /// Emit a warning to stderr
    pub fn warn(&self, msg: &str);

    /// Emit an error to stderr. Also returns the error as JSON on
    /// stdout when format is Json.
    pub fn error(&self, err: &dyn std::error::Error);
}
```

### Entry Point Flow

```
fn main() {
    // 1. Parse CLI args
    let cli = Cli::parse();

    // 2. Initialize tracing based on verbosity
    init_tracing(cli.verbose);

    // 3. Locate repository root (gix discover or git rev-parse)
    // Some commands (like `--version`) don't need a repo.
    let repo_root = discover_repo().ok();

    // 4. Load config (if in a repo)
    let config = if let Some(root) = &repo_root {
        load_config(&cli, root)?
    } else {
        ChronicleConfig::default()
    };

    // 5. Dispatch to subcommand handler
    match cli.command {
        Command::Init(args) => cmd::init::run(args, config)?,
        Command::Commit(args) => cmd::commit::run(args, config)?,
        // ... etc
    }
}
```

Commands that don't require a repository (`--version`, `--help`) work without one. Commands that require a repository (`init`, `annotate`, `read`, etc.) fail with a clear error if not run inside a git repository.

---

## Error Handling

### Error Types

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum ChronicleError {
    #[snafu(display("Not a git repository (run from a directory containing .git), at {location}"))]
    NotARepository {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Chronicle is not initialized in this repository (run `git chronicle init`), at {location}"))]
    NotInitialized {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Configuration error: {message}, at {location}"))]
    Config {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Invalid config key: {key}, at {location}"))]
    InvalidConfigKey {
        key: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Git operation failed, at {location}"))]
    Git {
        #[snafu(source)]
        source: GitError,
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

    #[snafu(display("TOML parse error, at {location}"))]
    TomlParse {
        #[snafu(source)]
        source: toml::de::Error,
        #[snafu(implicit)]
        location: Location,
    },
}
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | General error |
| 2    | Usage error (bad arguments) — handled by clap |
| 3    | Not a git repository |
| 4    | Not initialized |
| 5    | Configuration error |

### Failure Modes

- **Not in a git repository.** Print clear error to stderr, exit 3. Do not panic.
- **`.chronicle-config.toml` is malformed.** Print parse error with line number, exit 5. Do not silently ignore.
- **`.git/config` has invalid chronicle values.** Warn to stderr, fall back to defaults for the invalid keys, continue.
- **Unknown subcommand.** Handled by clap with usage help.
- **Missing required arguments.** Handled by clap with per-command help.

---

## Configuration

All config keys and their meanings:

| Key | Type | Default | Source | Description |
|-----|------|---------|--------|-------------|
| `enabled` | bool | `true` | git/toml | Master switch. When false, hooks exit immediately. |
| `async` | bool | `true` | git/toml | Run annotation in background (true) or block (false). |
| `provider` | string | auto-detect | git/toml/cli | LLM provider name. |
| `model` | string | provider default | git/toml/cli | LLM model identifier. |
| `backfill_model` | string | same as `model` | git/toml/cli | Model for backfill (can be cheaper). |
| `noteref` | string | `refs/notes/chronicle` | git/toml | Git notes ref namespace. |
| `include` | string[] | `[]` (all files) | git/toml/cli | Glob patterns for files to annotate. |
| `exclude` | string[] | `[]` | git/toml/cli | Glob patterns for files to skip. |
| `maxDiffLines` | u32 | `2000` | git/toml | Skip annotation for diffs larger than this. |
| `skipTrivial` | bool | `true` | git/toml | Skip trivial commits before LLM call. |
| `trivialThreshold` | u32 | `3` | git/toml | Lines changed below this are trivial. |
| `auto_sync` | bool | `false` | toml | Auto-configure notes sync on init. |

---

## Implementation Steps

### Step 1: Project Scaffold and Cargo.toml

Create `Cargo.toml` with all dependencies. Create `src/main.rs` with a minimal clap `Cli` struct that parses `--version` and `--help`. Verify `cargo build` and `cargo test` pass.

**Deliverable:** Compiling binary that prints version and help.

### Step 2: Subcommand Definitions

Define all subcommand arg structs in `src/cli/`. Wire the `Command` enum to dispatch to stub handlers that print "not yet implemented" and exit 0. Every subcommand should be reachable via `git chronicle <command> --help`.

**Deliverable:** All subcommands parse and show help. `git chronicle init --dry-run` prints a placeholder message.

### Step 3: Error Types and Output Formatting

Implement `ChronicleError` with snafu. Every error variant gets a `location: Location` field (auto-captured via `#[snafu(implicit)]`) and a display message ending in `, at {location}`. Source errors from other crates are linked with `#[snafu(source)]`. Implement `Output` struct with `data()`, `status()`, `success()`, `warn()`, `error()` methods. Wire into main so all subcommands use `Output` consistently. Implement exit code mapping.

**Deliverable:** Errors display properly in both TTY and piped modes. JSON error output works.

### Step 4: Repository Discovery

Implement `discover_repo()` using `gix::discover()` with fallback to `git rev-parse --show-toplevel`. Subcommands that need a repo fail cleanly when not in one.

**Deliverable:** `git chronicle config list` outside a repo prints error and exits 3.

### Step 5: `.chronicle-config.toml` Parser

Define `SharedConfig` serde struct matching the TOML schema. Implement `load_shared_config()` that reads from `{repo_root}/.chronicle-config.toml` if it exists.

**Deliverable:** Unit tests parsing valid, invalid, and missing TOML files.

### Step 6: `.git/config` Reader

Implement `load_git_config()` that reads `[chronicle]` keys from `.git/config` via `gix` config API, with fallback to `git config --get`. Handle missing section, missing keys, and invalid values.

**Deliverable:** Unit tests for config reading with various key combinations. Integration test with a real `.git/config`.

### Step 7: Config Merge and `ChronicleConfig`

Implement the merge logic: defaults -> shared config -> git config. Implement `config get`, `config set`, and `config list` subcommands that read/write the git config layer.

**Deliverable:** `git chronicle config set provider anthropic` writes to `.git/config`. `git chronicle config list` shows merged config with source annotations.

### Step 8: Tracing Setup

Configure `tracing-subscriber` based on `--verbose` flag levels. `-v` = info, `-vv` = debug, `-vvv` = trace. Default = warn. Integrate with `Output` so tracing events go to stderr.

**Deliverable:** `git chronicle -vvv config list` shows debug-level config loading trace.

---

## Test Plan

### Unit Tests

- **Config parsing:** Valid TOML, missing fields (defaults apply), invalid fields (error with context), empty file.
- **Config merge:** Verify precedence. Set a value in TOML and a different value in git config; git config wins. Set a value in git config; CLI flag overrides it.
- **Output formatting:** JSON output is valid JSON. Pretty output contains status markers. Error output includes exit code in JSON mode.
- **Arg parsing:** Every subcommand parses valid args. Invalid args produce useful errors. Trailing var args in `commit` pass through correctly.

### Integration Tests

- **Repository discovery:** Create a temp git repo, verify `discover_repo()` finds it from a subdirectory. Verify failure outside a repo.
- **Config round-trip:** `git chronicle init` writes config, `git chronicle config list` reads it back, values match.
- **TOML + git config interaction:** Place `.chronicle-config.toml` in repo root, set different value in `.git/config`, verify merged config respects precedence.

### Edge Cases

- Binary run outside any git repository.
- `.chronicle-config.toml` exists but is not valid TOML.
- `.git/config` has `[chronicle]` section with unknown keys (should be ignored, not error).
- Config values with unusual characters (paths with spaces, glob patterns with `**`).
- `git chronicle commit` with complex passthrough args (`-a`, `--amend`, `--no-edit`).

---

## Acceptance Criteria

1. `cargo install --path .` produces a working `chronicle` binary.
2. `git chronicle --version` prints version.
3. `git chronicle --help` lists all subcommands with descriptions.
4. `git chronicle <subcommand> --help` shows help for every subcommand.
5. `git chronicle config set <key> <value>` writes to `.git/config` `[chronicle]` section.
6. `git chronicle config get <key>` reads merged config and reports the value.
7. `git chronicle config list` shows all config keys, values, and their source (default/toml/git/cli).
8. Config precedence is verifiable: set conflicting values in `.chronicle-config.toml` and `.git/config`, confirm git config wins.
9. Running any subcommand outside a git repository produces a clear error and exit code 3.
10. `--format json` produces valid JSON on stdout for commands that emit data.
11. All subcommands are wired (even as stubs) — no `unimplemented!()` panics at the dispatch level; stubs return a structured "not yet implemented" response.
12. `tracing` is wired and `-v` flags increase output verbosity to stderr.
