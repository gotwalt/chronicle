use chronicle::cli::{Cli, Commands, SyncAction};
use clap::Parser;

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Setup {
            force,
            dry_run,
            skip_skills,
            skip_hooks,
            skip_claude_md,
        } => chronicle::cli::setup::run(force, dry_run, skip_skills, skip_hooks, skip_claude_md),
        Commands::Reconfigure => chronicle::cli::reconfigure::run(),
        Commands::Backfill { limit, dry_run } => chronicle::cli::backfill::run(limit, dry_run),
        Commands::Init {
            no_sync,
            no_hooks,
            provider,
            model,
            backfill,
        } => chronicle::cli::init::run(no_sync, no_hooks, provider, model, backfill),
        Commands::Context { action } => chronicle::cli::context::run(action),
        Commands::Annotate {
            commit,
            sync,
            live,
            squash_sources,
            amend_source,
        } => chronicle::cli::annotate::run(commit, sync, live, squash_sources, amend_source),
        Commands::Read {
            path,
            anchor,
            lines,
        } => chronicle::cli::read::run(path, anchor, lines),
        Commands::Flag {
            path,
            anchor,
            reason,
        } => chronicle::cli::flag::run(path, anchor, reason),
        Commands::Correct {
            sha,
            region,
            field,
            remove,
            amend,
        } => chronicle::cli::correct::run(sha, region, field, remove, amend),
        Commands::Sync { action } => match action {
            SyncAction::Enable { remote } => chronicle::cli::sync::run_enable(&remote),
            SyncAction::Status { remote } => chronicle::cli::sync::run_status(&remote),
            SyncAction::Pull { remote } => chronicle::cli::sync::run_pull(&remote),
        },
        Commands::Export { output } => chronicle::cli::export::run(output),
        Commands::Import {
            file,
            force,
            dry_run,
        } => chronicle::cli::import::run(file, force, dry_run),
        Commands::Doctor { json } => chronicle::cli::doctor::run(json),
        Commands::Deps {
            path,
            anchor,
            format,
            max_results,
            scan_limit,
        } => chronicle::cli::deps::run(path, anchor, max_results, scan_limit, format),
        Commands::History {
            path,
            anchor,
            limit,
            format,
            follow_related,
        } => chronicle::cli::history::run(path, anchor, limit, follow_related, format),
        Commands::Show {
            path,
            anchor,
            commit,
            no_tui,
        } => chronicle::cli::show::run(path, anchor, commit, no_tui),
        Commands::Schema { name } => chronicle::cli::schema::run(&name),
        Commands::Contracts {
            path,
            anchor,
            format,
        } => chronicle::cli::contracts::run(path, anchor, format),
        Commands::Decisions { path, format } => chronicle::cli::decisions::run(path, format),
        Commands::Summary {
            path,
            anchor,
            format,
        } => chronicle::cli::summary::run(path, anchor, format),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
