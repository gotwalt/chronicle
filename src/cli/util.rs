use std::path::PathBuf;

use crate::error::chronicle_error::{IoSnafu, NotARepositorySnafu};
use crate::error::Result;
use snafu::ResultExt;

/// Find the .git directory by running `git rev-parse --git-dir`.
pub(crate) fn find_git_dir() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .context(IoSnafu)?;

    if output.status.success() {
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = PathBuf::from(&dir);
        if path.is_relative() {
            let cwd = std::env::current_dir().context(IoSnafu)?;
            Ok(cwd.join(path))
        } else {
            Ok(path)
        }
    } else {
        let cwd = std::env::current_dir().context(IoSnafu)?;
        Err(NotARepositorySnafu { path: cwd }.build())
    }
}
