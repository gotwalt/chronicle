pub mod init;
pub mod commit;
pub mod context;
pub mod annotate;
pub mod read;
pub mod deps;
pub mod history;
pub mod summary;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ultragit", version, about = "AI-powered commit annotation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize ultragit in the current repository
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

    /// Commit with annotation context (wraps git commit)
    Commit {
        /// Commit message
        #[arg(short, long)]
        message: Option<String>,

        /// Task identifier for the commit
        #[arg(long)]
        task: Option<String>,

        /// Reasoning behind the changes
        #[arg(long)]
        reasoning: Option<String>,

        /// Dependencies affected
        #[arg(long)]
        dependencies: Option<String>,

        /// Tags for the annotation
        #[arg(long)]
        tags: Vec<String>,

        /// Additional args passed through to git commit
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        git_args: Vec<String>,
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
