use crate::error::Result;
use crate::git::CliOps;
use crate::read::{execute, ReadQuery};
use crate::schema::LineRange;

pub fn run(path: String, anchor: Option<String>, lines: Option<String>) -> Result<()> {
    let line_range = match lines {
        Some(ref s) => Some(parse_line_range(s)?),
        None => None,
    };

    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let query = ReadQuery {
        file: path,
        anchor,
        lines: line_range,
    };

    let result = execute(&git_ops, &query)?;

    let json =
        serde_json::to_string_pretty(&result).map_err(|e| crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        })?;
    println!("{json}");

    Ok(())
}

/// Parse a "start:end" line range string.
fn parse_line_range(s: &str) -> Result<LineRange> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(crate::error::ChronicleError::Config {
            message: format!("invalid line range '{s}', expected format 'start:end'"),
            location: snafu::Location::default(),
        });
    }
    let start: u32 = parts[0]
        .parse()
        .map_err(|_| crate::error::ChronicleError::Config {
            message: format!("invalid start line number '{}'", parts[0]),
            location: snafu::Location::default(),
        })?;
    let end: u32 = parts[1]
        .parse()
        .map_err(|_| crate::error::ChronicleError::Config {
            message: format!("invalid end line number '{}'", parts[1]),
            location: snafu::Location::default(),
        })?;
    Ok(LineRange { start, end })
}
