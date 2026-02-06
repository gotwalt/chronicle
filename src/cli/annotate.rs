use crate::error::Result;
use crate::git::CliOps;

pub async fn run(commit: String, sync: bool, live: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    if live {
        return run_live(&git_ops);
    }

    let provider = crate::provider::discover_provider()
        .map_err(|e| crate::error::UltragitError::Provider {
            source: e,
            location: snafu::Location::default(),
        })?;

    let annotation = crate::annotate::run(&git_ops, provider.as_ref(), &commit, sync)
        .await?;

    let json = serde_json::to_string_pretty(&annotation).map_err(|e| {
        crate::error::UltragitError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;
    println!("{json}");

    Ok(())
}

/// Live annotation path: read AnnotateInput JSON from stdin, call handle_annotate,
/// print AnnotateResult JSON to stdout. Zero LLM cost.
fn run_live(git_ops: &CliOps) -> Result<()> {
    let stdin = std::io::read_to_string(std::io::stdin()).map_err(|e| {
        crate::error::UltragitError::Io {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    let input: crate::mcp::annotate_handler::AnnotateInput =
        serde_json::from_str(&stdin).map_err(|e| crate::error::UltragitError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;

    let result = crate::mcp::annotate_handler::handle_annotate(git_ops, input)?;

    let json = serde_json::to_string_pretty(&result).map_err(|e| {
        crate::error::UltragitError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;
    println!("{json}");

    Ok(())
}
