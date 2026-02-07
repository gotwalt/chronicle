use crate::error::Result;
use crate::git::CliOps;
use crate::read::deps::{find_dependents, DepsQuery};

pub fn run(
    path: String,
    anchor: Option<String>,
    max_results: u32,
    scan_limit: u32,
    format: String,
) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let query = DepsQuery {
        file: path,
        anchor,
        max_results,
        scan_limit,
    };

    let result =
        find_dependents(&git_ops, &query).map_err(|e| crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        })?;

    let json = if format == "pretty" {
        serde_json::to_string_pretty(&result)
    } else {
        serde_json::to_string(&result)
    }
    .map_err(|e| crate::error::ChronicleError::Json {
        source: e,
        location: snafu::Location::default(),
    })?;

    println!("{json}");
    Ok(())
}
