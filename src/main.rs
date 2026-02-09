use chronicle::cli::{Cli, Commands, KnowledgeAction, SyncAction};
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
        Commands::Init { no_sync, no_hooks } => chronicle::cli::init::run(no_sync, no_hooks),
        Commands::Annotate {
            commit,
            live,
            squash_sources,
            amend_source,
            summary,
            json_input,
            auto,
        } => chronicle::cli::annotate::run(chronicle::cli::annotate::AnnotateArgs {
            commit,
            live,
            squash_sources,
            amend_source,
            summary,
            json_input,
            auto,
        }),
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
        Commands::Doctor { json, staleness } => chronicle::cli::doctor::run(json, staleness),
        Commands::Deps {
            path,
            anchor,
            format,
            max_results,
            scan_limit,
            compact,
        } => chronicle::cli::deps::run(path, anchor, max_results, scan_limit, format, compact),
        Commands::History {
            path,
            anchor,
            limit,
            format,
            compact,
        } => chronicle::cli::history::run(path, anchor, limit, format, compact),
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
            compact,
        } => chronicle::cli::contracts::run(path, anchor, format, compact),
        Commands::Decisions {
            path,
            format,
            compact,
        } => chronicle::cli::decisions::run(path, format, compact),
        Commands::Summary {
            path,
            anchor,
            format,
            compact,
        } => chronicle::cli::summary::run(path, anchor, format, compact),
        Commands::Lookup {
            path,
            anchor,
            format,
            compact,
        } => chronicle::cli::lookup::run(path, anchor, format, compact),
        Commands::Note { text, list, clear } => chronicle::cli::note::run(text, list, clear),
        Commands::Knowledge { action } => match action {
            KnowledgeAction::List { json } => chronicle::cli::knowledge::run_list(json),
            KnowledgeAction::Add {
                entry_type,
                id,
                scope,
                rule,
                module,
                owns,
                boundary,
                pattern,
                instead,
                stability,
                decided_in,
                learned_from,
            } => chronicle::cli::knowledge::run_add(chronicle::cli::knowledge::KnowledgeAddArgs {
                entry_type,
                id,
                scope,
                rule,
                module,
                owns,
                boundary,
                pattern,
                instead,
                stability,
                decided_in,
                learned_from,
            }),
            KnowledgeAction::Remove { id } => chronicle::cli::knowledge::run_remove(id),
        },
        Commands::Status { format } => chronicle::cli::status::run(format),
        #[cfg(feature = "web")]
        Commands::Web { port, open } => chronicle::cli::web::run(port, open),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
