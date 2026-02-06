pub mod init;
pub mod context;
pub mod annotate;
pub mod read;
pub mod flag;
pub mod correct;
pub mod sync;
pub mod export;
pub mod import;
pub mod doctor;
pub mod deps;
pub mod history;
pub mod summary;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-chronicle", version, about = "AI-powered commit annotation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize chronicle in the current repository
    Init {
        /// Run annotations synchronously (default: async)
        #[arg(long)]
        sync: bool,

        /// Skip hook installation
        #[arg(long)]
        no_hooks: bool,

        /// LLM provider to use
        #[arg(long)]
        provider: Option<String>,

        /// LLM model to use
        #[arg(long)]
        model: Option<String>,
    },

    /// Manage annotation context
    Context {
        #[command(subcommand)]
        action: ContextAction,
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

        /// Run synchronously
        #[arg(long)]
        sync: bool,

        /// Read AnnotateInput JSON from stdin (live annotation path, zero LLM cost)
        #[arg(long)]
        live: bool,

        /// Comma-separated source commit SHAs for squash synthesis (CI usage)
        #[arg(long)]
        squash_sources: Option<String>,

        /// Old commit SHA to migrate annotation from (amend re-annotation)
        #[arg(long)]
        amend_source: Option<String>,
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

        /// Follow related annotation links
        #[arg(long, default_value = "true")]
        follow_related: bool,
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

#[derive(Subcommand)]
pub enum ContextAction {
    /// Set pending context for the next commit
    Set {
        #[arg(long)]
        task: Option<String>,

        #[arg(long)]
        reasoning: Option<String>,

        #[arg(long)]
        dependencies: Option<String>,

        #[arg(long)]
        tags: Vec<String>,
    },

    /// Show current pending context
    Show,

    /// Clear pending context
    Clear,
}
