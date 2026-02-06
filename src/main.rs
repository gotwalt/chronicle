use clap::Parser;
use ultragit::cli::{Cli, Commands, SyncAction};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { sync, no_hooks, provider, model } => {
            ultragit::cli::init::run(sync, no_hooks, provider, model)
        }
        Commands::Commit { message, task, reasoning, dependencies, tags, git_args } => {
            ultragit::cli::commit::run(message, task, reasoning, dependencies, tags, git_args)
        }
        Commands::Context { action } => {
            ultragit::cli::context::run(action)
        }
        Commands::Annotate { commit, sync, live, squash_sources, amend_source } => {
            ultragit::cli::annotate::run(commit, sync, live, squash_sources, amend_source).await
        }
        Commands::Read { path, anchor, lines } => {
            ultragit::cli::read::run(path, anchor, lines)
        }
        Commands::Flag { path, anchor, reason } => {
            ultragit::cli::flag::run(path, anchor, reason)
        }
        Commands::Correct { sha, region, field, remove, amend } => {
            ultragit::cli::correct::run(sha, region, field, remove, amend)
        }
        Commands::Sync { action } => {
            match action {
                SyncAction::Enable { remote } => {
                    ultragit::cli::sync::run_enable(&remote)
                }
                SyncAction::Status { remote } => {
                    ultragit::cli::sync::run_status(&remote)
                }
                SyncAction::Pull { remote } => {
                    ultragit::cli::sync::run_pull(&remote)
                }
            }
        }
        Commands::Export { output } => {
            ultragit::cli::export::run(output)
        }
        Commands::Import { file, force, dry_run } => {
            ultragit::cli::import::run(file, force, dry_run)
        }
        Commands::Doctor { json } => {
            ultragit::cli::doctor::run(json)
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
