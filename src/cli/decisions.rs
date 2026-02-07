use crate::error::Result;
use crate::git::CliOps;

/// Run the `git chronicle decisions` command.
pub fn run(path: Option<String>, format: String) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let query = crate::read::decisions::DecisionsQuery { file: path };

    let output = crate::read::decisions::query_decisions(&git_ops, &query).map_err(|e| {
        crate::error::ChronicleError::Git {
            source: e,
            location: snafu::Location::default(),
        }
    })?;

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&output).map_err(|e| {
                crate::error::ChronicleError::Json {
                    source: e,
                    location: snafu::Location::default(),
                }
            })?;
            println!("{json}");
        }
        _ => {
            if output.decisions.is_empty() && output.rejected_alternatives.is_empty() {
                println!("No decisions or rejected alternatives found.");
                return Ok(());
            }
            if !output.decisions.is_empty() {
                println!("Decisions:");
                for d in &output.decisions {
                    println!("  [{}] {}: {}", d.stability, d.what, d.why);
                    if let Some(ref rw) = d.revisit_when {
                        println!("    Revisit when: {rw}");
                    }
                }
            }
            if !output.rejected_alternatives.is_empty() {
                println!("Rejected alternatives:");
                for ra in &output.rejected_alternatives {
                    println!("  - {}: {}", ra.approach, ra.reason);
                }
            }
        }
    }

    Ok(())
}
