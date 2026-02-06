use std::path::PathBuf;

use crate::error::Result;
use crate::sync::{enable_sync, get_sync_config, get_sync_status, pull_notes};

/// Run `ultragit sync enable`.
pub fn run_enable(remote: &str) -> Result<()> {
    let repo_dir = repo_dir()?;
    enable_sync(&repo_dir, remote)?;
    eprintln!("sync enabled for remote '{remote}'");
    eprintln!("  push refspec:  refs/notes/ultragit -> {remote}");
    eprintln!("  fetch refspec: +refs/notes/ultragit:refs/notes/ultragit");
    Ok(())
}

/// Run `ultragit sync status`.
pub fn run_status(remote: &str) -> Result<()> {
    let repo_dir = repo_dir()?;
    let config = get_sync_config(&repo_dir, remote)?;
    let status = get_sync_status(&repo_dir, remote)?;

    if config.is_enabled() {
        println!("Notes sync: enabled");
        if let Some(ref push) = config.push_refspec {
            println!("  Push refspec:  {push} -> {remote}");
        }
        if let Some(ref fetch) = config.fetch_refspec {
            println!("  Fetch refspec: {fetch}");
        }
    } else {
        println!("Notes sync: not configured");
        println!("  Run `ultragit sync enable` to set up sync.");
    }

    println!("  Local notes:   {} annotated commits", status.local_count);
    if let Some(rc) = status.remote_count {
        println!("  Remote notes:  {} annotated commits ({} not yet pushed)", rc, status.unpushed_count);
    } else {
        println!("  Remote notes:  unknown (remote unreachable)");
    }

    Ok(())
}

/// Run `ultragit sync pull`.
pub fn run_pull(remote: &str) -> Result<()> {
    let repo_dir = repo_dir()?;
    pull_notes(&repo_dir, remote)?;
    eprintln!("fetched notes from '{remote}'");
    Ok(())
}

fn repo_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
        source: e,
        location: snafu::Location::default(),
    })
}
