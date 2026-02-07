use crate::error::Result;
use crate::git::CliOps;
use crate::read::history::{build_timeline, HistoryQuery};

pub fn run(
    path: String,
    anchor: Option<String>,
    limit: u32,
    follow_related: bool,
    format: String,
) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let query = HistoryQuery {
        file: path,
        anchor,
        limit,
        follow_related,
    };

    let result =
        build_timeline(&git_ops, &query).map_err(|e| crate::error::ChronicleError::Git {
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
