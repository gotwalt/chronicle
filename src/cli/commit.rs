use std::path::PathBuf;

use crate::error::ultragit_error::IoSnafu;
use crate::error::Result;
use crate::hooks::{delete_pending_context, write_pending_context, PendingContext};
use snafu::ResultExt;

pub fn run(
    message: Option<String>,
    task: Option<String>,
    reasoning: Option<String>,
    dependencies: Option<String>,
    tags: Vec<String>,
    git_args: Vec<String>,
) -> Result<()> {
    let git_dir = find_git_dir()?;

    // Write pending context if any annotation context was provided
    let has_context = task.is_some() || reasoning.is_some() || dependencies.is_some() || !tags.is_empty();

    if has_context {
        let ctx = PendingContext {
            task,
            reasoning,
            dependencies,
            tags,
        };
        write_pending_context(&git_dir, &ctx)?;
    }

    // Build the git commit command
    let mut cmd = std::process::Command::new("git");
    cmd.arg("commit");

    if let Some(ref msg) = message {
        cmd.args(["-m", msg]);
    }

    // Pass through any additional git args
    for arg in &git_args {
        cmd.arg(arg);
    }

    // Exec git commit, inheriting stdio so the user sees git's output
    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .context(IoSnafu)?;

    if !status.success() {
        // Clean up pending context on failure
        if has_context {
            let _ = delete_pending_context(&git_dir);
        }
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Find the .git directory by running `git rev-parse --git-dir`.
fn find_git_dir() -> Result<PathBuf> {
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
        Err(crate::error::ultragit_error::NotARepositorySnafu { path: cwd }.build())
    }
}
