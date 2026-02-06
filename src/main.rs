use clap::Parser;
use chronicle::cli::{Cli, Commands, SyncAction};

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { sync, no_hooks, provider, model } => {
            chronicle::cli::init::run(sync, no_hooks, provider, model)
        }
        Commands::Context { action } => {
            chronicle::cli::context::run(action)
        }
        Commands::Annotate { commit, sync, live, squash_sources, amend_source } => {
            chronicle::cli::annotate::run(commit, sync, live, squash_sources, amend_source)
        }
        Commands::Read { path, anchor, lines } => {
            chronicle::cli::read::run(path, anchor, lines)
        }
        Commands::Flag { path, anchor, reason } => {
            chronicle::cli::flag::run(path, anchor, reason)
        }
        Commands::Correct { sha, region, field, remove, amend } => {
            chronicle::cli::correct::run(sha, region, field, remove, amend)
        }
        Commands::Sync { action } => {
            match action {
                SyncAction::Enable { remote } => {
                    chronicle::cli::sync::run_enable(&remote)
                }
                SyncAction::Status { remote } => {
                    chronicle::cli::sync::run_status(&remote)
                }
                SyncAction::Pull { remote } => {
                    chronicle::cli::sync::run_pull(&remote)
                }
            }
        }
        Commands::Export { output } => {
            chronicle::cli::export::run(output)
        }
        Commands::Import { file, force, dry_run } => {
            chronicle::cli::import::run(file, force, dry_run)
        }
        Commands::Doctor { json } => {
            chronicle::cli::doctor::run(json)
        }
        Commands::Deps { path, anchor, format, max_results, scan_limit } => {
            chronicle::cli::deps::run(path, anchor, max_results, scan_limit, format)
        }
        Commands::History { path, anchor, limit, format, follow_related } => {
            chronicle::cli::history::run(path, anchor, limit, follow_related, format)
        }
        Commands::Summary { path, anchor, format } => {
            chronicle::cli::summary::run(path, anchor, format)
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
