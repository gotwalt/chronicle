use crate::error::Result;
use schemars::schema_for;

/// Run the `git chronicle schema <name>` subcommand.
///
/// Prints the JSON Schema for the requested type to stdout.
/// This makes the CLI self-documenting for AI agents.
pub fn run(name: &str) -> Result<()> {
    let schema = match name {
        "annotate-input" => {
            schema_for!(crate::annotate::live::LiveInput)
        }
        "annotation" => {
            schema_for!(crate::schema::v2::Annotation)
        }
        "knowledge" => {
            schema_for!(crate::schema::knowledge::KnowledgeStore)
        }
        _ => {
            return Err(crate::error::ChronicleError::Validation {
                message: format!(
                    "Unknown schema name: '{name}'. Available: annotate-input, annotation, knowledge"
                ),
                location: snafu::Location::default(),
            });
        }
    };

    let json = serde_json::to_string_pretty(&schema).map_err(|e| {
        crate::error::ChronicleError::Json {
            source: e,
            location: snafu::Location::default(),
        }
    })?;
    println!("{json}");

    Ok(())
}
