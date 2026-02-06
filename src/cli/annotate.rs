use crate::error::Result;
use crate::git::CliOps;

pub async fn run(commit: String, sync: bool) -> Result<()> {
    let provider = crate::provider::discover_provider()
        .map_err(|e| crate::error::UltragitError::Provider {
            source: e,
            location: snafu::Location::default(),
        })?;

    let repo_dir = std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

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
