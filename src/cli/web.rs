use crate::error::Result;

pub fn run(port: Option<u16>, open_browser: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = crate::git::CliOps::new(repo_dir);

    crate::web::serve(git_ops, port, open_browser)
}
