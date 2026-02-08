use std::path::PathBuf;

use crate::annotate::staging;
use crate::error::Result;

/// Run `git chronicle note` command.
pub fn run(text: Option<String>, list: bool, clear: bool) -> Result<()> {
    let git_dir = find_git_dir()?;

    if clear {
        staging::clear_staged(&git_dir)?;
        println!("Staged notes cleared.");
        return Ok(());
    }

    if list || text.is_none() {
        let notes = staging::read_staged(&git_dir)?;
        if notes.is_empty() {
            println!("No staged notes.");
        } else {
            println!("Staged notes ({}):", notes.len());
            for note in &notes {
                println!("  [{}] {}", note.timestamp, note.text);
            }
        }
        return Ok(());
    }

    if let Some(note_text) = text {
        staging::append_staged(&git_dir, &note_text)?;
        let count = staging::read_staged(&git_dir)?.len();
        println!("Note staged ({count} total). Will be included in next annotation.");
    }

    Ok(())
}

/// Find the .git directory.
fn find_git_dir() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map_err(|e| crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

    if output.status.success() {
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = PathBuf::from(&dir);
        if path.is_relative() {
            let cwd = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
                source: e,
                location: snafu::Location::default(),
            })?;
            Ok(cwd.join(path))
        } else {
            Ok(path)
        }
    } else {
        let cwd = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;
        Err(crate::error::chronicle_error::NotARepositorySnafu { path: cwd }.build())
    }
}
