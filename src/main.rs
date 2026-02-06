use clap::Parser;
use ultragit::cli::{Cli, Commands};

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
        Commands::Annotate { commit, sync, live } => {
            ultragit::cli::annotate::run(commit, sync, live).await
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
