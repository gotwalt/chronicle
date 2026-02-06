pub mod init;
pub mod commit;
pub mod context;
pub mod annotate;
pub mod read;
pub mod sync;
pub mod export;
pub mod import;
pub mod doctor;

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

    /// Run diagnostic checks on the ultragit setup
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
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
