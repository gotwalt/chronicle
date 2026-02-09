pub mod annotate;
pub mod contracts;
pub mod correct;
pub mod decisions;
pub mod deps;
pub mod doctor;
pub mod export;
pub mod flag;
pub mod history;
pub mod import;
pub mod init;
pub mod knowledge;
pub mod lookup;
pub mod note;
pub mod read;
pub mod schema;
pub mod setup;
pub mod show;
pub mod status;
pub mod summary;
pub mod sync;
pub(crate) mod util;
#[cfg(feature = "web")]
pub mod web;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "git-chronicle",
    version,
    about = "AI-powered commit annotation"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// One-time machine-wide setup (provider, skills, hooks, CLAUDE.md)
    Setup {
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
    },

    /// Initialize chronicle in the current repository
    Init {
        /// Disable notes sync (sync is enabled by default)
        #[arg(long)]
        no_sync: bool,

        /// Skip hook installation
        #[arg(long)]
        no_hooks: bool,
    },

    /// Read annotations for a file
    Read {
        /// File path to read annotations for
        path: String,

        /// Filter by AST anchor name
        #[arg(long)]
        anchor: Option<String>,

        /// Filter by line range (format: start:end)
        #[arg(long)]
        lines: Option<String>,
    },

    /// Annotate a specific commit
    Annotate {
        /// Commit to annotate (default: HEAD)
        #[arg(long, default_value = "HEAD")]
        commit: String,

        /// Read AnnotateInput JSON from stdin (live annotation path, zero LLM cost)
        #[arg(long)]
        live: bool,

        /// Comma-separated source commit SHAs for squash synthesis (CI usage)
        #[arg(long)]
        squash_sources: Option<String>,

        /// Old commit SHA to migrate annotation from (amend re-annotation)
        #[arg(long)]
        amend_source: Option<String>,

        /// Quick annotation: provide summary directly on command line
        #[arg(long, conflicts_with_all = ["live", "json_input", "squash_sources", "amend_source"])]
        summary: Option<String>,

        /// Provide full annotation JSON on command line
        #[arg(long = "json", conflicts_with_all = ["live", "summary", "squash_sources", "amend_source"])]
        json_input: Option<String>,

        /// Auto-annotate using the commit message as summary
        #[arg(long, conflicts_with_all = ["live", "summary", "json_input", "squash_sources", "amend_source"])]
        auto: bool,
    },

    /// Flag a region annotation as potentially inaccurate
    Flag {
        /// File path relative to repository root
        path: String,

        /// Optional AST anchor name to scope the flag to a specific region
        anchor: Option<String>,

        /// Reason for flagging this annotation
        #[arg(long)]
        reason: String,
    },

    /// Apply a precise correction to a specific annotation field
    Correct {
        /// Commit SHA of the annotation to correct
        sha: String,

        /// AST anchor name of the region within the annotation
        #[arg(long)]
        region: String,

        /// Annotation field to correct (intent, reasoning, constraints, risk_notes, semantic_dependencies, tags)
        #[arg(long)]
        field: String,

        /// Specific value to remove or mark as incorrect
        #[arg(long)]
        remove: Option<String>,

        /// Replacement or amendment text
        #[arg(long)]
        amend: Option<String>,
    },

    /// Manage notes sync with remotes
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },

    /// Export annotations as JSONL
    Export {
        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Import annotations from a JSONL file
    Import {
        /// JSONL file to import
        file: String,

        /// Overwrite existing annotations
        #[arg(long)]
        force: bool,

        /// Show what would be imported without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Run diagnostic checks on the chronicle setup
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Include annotation staleness check (scans recent annotations)
        #[arg(long)]
        staleness: bool,
    },

    /// Find code that depends on a given file/anchor (dependency inversion)
    Deps {
        /// File path to query
        path: String,

        /// AST anchor name to query
        anchor: Option<String>,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Maximum number of results to return
        #[arg(long, default_value = "50")]
        max_results: u32,

        /// Maximum number of commits to scan
        #[arg(long, default_value = "500")]
        scan_limit: u32,

        /// Omit metadata (schema, query echo, stats) from JSON output
        #[arg(long)]
        compact: bool,
    },

    /// Show annotation timeline for a file/anchor across commits
    History {
        /// File path to query
        path: String,

        /// AST anchor name to query
        anchor: Option<String>,

        /// Maximum number of timeline entries
        #[arg(long, default_value = "10")]
        limit: u32,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Omit metadata (schema, query echo, stats) from JSON output
        #[arg(long)]
        compact: bool,
    },

    /// Interactive TUI explorer for annotated source code
    Show {
        /// File path to show
        path: String,

        /// Focus on a specific AST anchor
        anchor: Option<String>,

        /// Commit to show file at
        #[arg(long, default_value = "HEAD")]
        commit: String,

        /// Force non-interactive plain-text output
        #[arg(long)]
        no_tui: bool,
    },

    /// Print JSON Schema for annotation types (self-documenting for AI agents)
    Schema {
        /// Schema name: annotate-input, annotation
        name: String,
    },

    /// Query contracts and dependencies for a file/anchor ("What must I not break?")
    Contracts {
        /// File path to query
        path: String,

        /// AST anchor name to query
        #[arg(long)]
        anchor: Option<String>,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Omit metadata (schema, query echo, stats) from JSON output
        #[arg(long)]
        compact: bool,
    },

    /// Query design decisions and rejected alternatives ("What was decided?")
    Decisions {
        /// File path to scope decisions to (omit for all)
        path: Option<String>,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Omit metadata (schema, query echo) from JSON output
        #[arg(long)]
        compact: bool,
    },

    /// Show condensed annotation summary for a file
    Summary {
        /// File path to query
        path: String,

        /// Filter to a specific AST anchor
        #[arg(long)]
        anchor: Option<String>,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Omit metadata (schema, query echo, stats) from JSON output
        #[arg(long)]
        compact: bool,
    },

    /// One-stop context lookup for a file (contracts + decisions + history)
    Lookup {
        /// File path to query
        path: String,

        /// AST anchor name
        #[arg(long)]
        anchor: Option<String>,

        /// Output format (json or pretty)
        #[arg(long, default_value = "json")]
        format: String,

        /// Compact output (payload only)
        #[arg(long)]
        compact: bool,
    },

    /// Show annotation status and coverage for the repository
    Status {
        /// Output format
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Stage a note for the next annotation (captured context during work)
    Note {
        /// The note text to stage (omit to list or clear)
        text: Option<String>,

        /// List current staged notes
        #[arg(long)]
        list: bool,

        /// Clear all staged notes
        #[arg(long)]
        clear: bool,
    },

    /// Launch web viewer for browsing annotations
    #[cfg(feature = "web")]
    Web {
        /// Port to listen on
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },

    /// Manage repo-level knowledge (conventions, boundaries, anti-patterns)
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeAction,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum KnowledgeAction {
    /// List all knowledge entries
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Add a new knowledge entry
    Add {
        /// Type of entry: convention, boundary, anti-pattern
        #[arg(long = "type")]
        entry_type: String,

        /// Stable ID (auto-generated if omitted)
        #[arg(long)]
        id: Option<String>,

        /// File/directory scope (for conventions)
        #[arg(long)]
        scope: Option<String>,

        /// The rule text (for conventions)
        #[arg(long)]
        rule: Option<String>,

        /// Module directory (for boundaries)
        #[arg(long)]
        module: Option<String>,

        /// What the module owns (for boundaries)
        #[arg(long)]
        owns: Option<String>,

        /// What must not cross the boundary (for boundaries)
        #[arg(long)]
        boundary: Option<String>,

        /// The anti-pattern to avoid
        #[arg(long)]
        pattern: Option<String>,

        /// What to do instead (for anti-patterns)
        #[arg(long)]
        instead: Option<String>,

        /// Stability level: permanent, provisional, experimental
        #[arg(long)]
        stability: Option<String>,

        /// Commit SHA where this was decided
        #[arg(long)]
        decided_in: Option<String>,

        /// Where this was learned (for anti-patterns)
        #[arg(long)]
        learned_from: Option<String>,
    },

    /// Remove a knowledge entry by ID
    Remove {
        /// ID of the entry to remove
        id: String,
    },
}

#[derive(Subcommand)]
pub enum SyncAction {
    /// Enable notes sync for a remote
    Enable {
        /// Remote name (default: origin)
        #[arg(long, default_value = "origin")]
        remote: String,
    },

    /// Show sync status
    Status {
        /// Remote name (default: origin)
        #[arg(long, default_value = "origin")]
        remote: String,
    },

    /// Fetch and merge remote notes
    Pull {
        /// Remote name (default: origin)
        #[arg(long, default_value = "origin")]
        remote: String,
    },
}
