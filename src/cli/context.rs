use std::path::PathBuf;

use crate::cli::ContextAction;
use crate::error::chronicle_error::IoSnafu;
use crate::error::Result;
use crate::hooks::{
    delete_pending_context, read_pending_context, write_pending_context, PendingContext,
};
use snafu::ResultExt;

pub fn run(action: ContextAction) -> Result<()> {
    let git_dir = find_git_dir()?;

    match action {
        ContextAction::Set {
            task,
            reasoning,
            dependencies,
            tags,
        } => {
            let ctx = PendingContext {
                task,
                reasoning,
                dependencies,
                tags,
            };
            write_pending_context(&git_dir, &ctx)?;
            eprintln!("pending context saved");
        }
        ContextAction::Show => match read_pending_context(&git_dir)? {
            Some(ctx) => {
                println!("{}", serde_json::to_string_pretty(&ctx).unwrap_or_default());
            }
            None => {
                eprintln!("no pending context");
            }
        },
        ContextAction::Clear => {
            delete_pending_context(&git_dir)?;
            eprintln!("pending context cleared");
        }
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
        Err(crate::error::chronicle_error::NotARepositorySnafu { path: cwd }.build())
    }
}
