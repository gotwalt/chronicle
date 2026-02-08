use crate::error::Result;
use crate::git::CliOps;

/// Run the `git chronicle contracts` command.
pub fn run(path: String, anchor: Option<String>, format: String, compact: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let query = crate::read::contracts::ContractsQuery { file: path, anchor };

    let output = crate::read::contracts::query_contracts(&git_ops, &query).map_err(|e| {
        crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    match format.as_str() {
        "json" => {
            let json = if compact {
                let compact_out = serde_json::json!({
                    "contracts": output.contracts,
                    "dependencies": output.dependencies,
                });
                serde_json::to_string_pretty(&compact_out)
            } else {
                serde_json::to_string_pretty(&output)
            }
            .map_err(|e| crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            })?;
            println!("{json}");
        }
        _ => {
            if output.contracts.is_empty() && output.dependencies.is_empty() {
                println!("No contracts or dependencies found.");
                return Ok(());
            }
            if !output.contracts.is_empty() {
                println!("Contracts:");
                for c in &output.contracts {
                    let anchor_str = c
                        .anchor
                        .as_ref()
                        .map(|a| format!(":{}", a))
                        .unwrap_or_default();
                    println!(
                        "  [{}] {}{}: {}",
                        c.source, c.file, anchor_str, c.description
                    );
                }
            }
            if !output.dependencies.is_empty() {
                println!("Dependencies:");
                for d in &output.dependencies {
                    let anchor_str = d
                        .anchor
                        .as_ref()
                        .map(|a| format!(":{}", a))
                        .unwrap_or_default();
                    println!(
                        "  {}{} -> {}:{} ({})",
                        d.file, anchor_str, d.target_file, d.target_anchor, d.assumption
                    );
                }
            }
        }
    }

    Ok(())
}
