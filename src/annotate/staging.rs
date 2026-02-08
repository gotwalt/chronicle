use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::chronicle_error;
use crate::error::Result;
use snafu::ResultExt;

/// A single staged note captured during work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedNote {
    pub timestamp: String,
    pub text: String,
}

const STAGED_NOTES_FILE: &str = "chronicle/staged-notes.json";

/// Resolve the staged notes file path from a .git directory.
fn staged_notes_path(git_dir: &Path) -> PathBuf {
    git_dir.join(STAGED_NOTES_FILE)
}

/// Read all staged notes. Returns empty vec if no staged notes exist.
pub fn read_staged(git_dir: &Path) -> Result<Vec<StagedNote>> {
    let path = staged_notes_path(git_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path).context(chronicle_error::IoSnafu)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let notes: Vec<StagedNote> =
        serde_json::from_str(&content).context(chronicle_error::JsonSnafu)?;
    Ok(notes)
}

/// Append a new note to the staging area.
pub fn append_staged(git_dir: &Path, text: &str) -> Result<()> {
    let path = staged_notes_path(git_dir);

    // Ensure the chronicle directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context(chronicle_error::IoSnafu)?;
    }

    let mut notes = read_staged(git_dir)?;
    notes.push(StagedNote {
        timestamp: chrono::Utc::now().to_rfc3339(),
        text: text.to_string(),
    });

    let json = serde_json::to_string_pretty(&notes).context(chronicle_error::JsonSnafu)?;
    std::fs::write(&path, json).context(chronicle_error::IoSnafu)?;

    Ok(())
}

/// Clear all staged notes.
pub fn clear_staged(git_dir: &Path) -> Result<()> {
    let path = staged_notes_path(git_dir);
    if path.exists() {
        std::fs::remove_file(&path).context(chronicle_error::IoSnafu)?;
    }
    Ok(())
}

/// Format staged notes as a provenance notes string.
pub fn format_for_provenance(notes: &[StagedNote]) -> String {
    notes
        .iter()
        .map(|n| format!("[{}] {}", n.timestamp, n.text))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_git_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("chronicle")).unwrap();
        tmp
    }

    #[test]
    fn test_read_empty_staging() {
        let tmp = setup_git_dir();
        let notes = read_staged(tmp.path()).unwrap();
        assert!(notes.is_empty());
    }

    #[test]
    fn test_append_and_read() {
        let tmp = setup_git_dir();

        append_staged(tmp.path(), "Tried approach X, didn't work").unwrap();
        append_staged(tmp.path(), "Approach Y works better").unwrap();

        let notes = read_staged(tmp.path()).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].text, "Tried approach X, didn't work");
        assert_eq!(notes[1].text, "Approach Y works better");
        assert!(!notes[0].timestamp.is_empty());
    }

    #[test]
    fn test_clear_staged() {
        let tmp = setup_git_dir();

        append_staged(tmp.path(), "Some note").unwrap();
        assert!(!read_staged(tmp.path()).unwrap().is_empty());

        clear_staged(tmp.path()).unwrap();
        assert!(read_staged(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn test_clear_nonexistent_is_ok() {
        let tmp = setup_git_dir();
        // Should not error when no file exists
        clear_staged(tmp.path()).unwrap();
    }

    #[test]
    fn test_format_for_provenance() {
        let notes = vec![
            StagedNote {
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                text: "Tried X".to_string(),
            },
            StagedNote {
                timestamp: "2025-01-01T00:01:00Z".to_string(),
                text: "Y worked".to_string(),
            },
        ];

        let formatted = format_for_provenance(&notes);
        assert!(formatted.contains("[2025-01-01T00:00:00Z] Tried X"));
        assert!(formatted.contains("[2025-01-01T00:01:00Z] Y worked"));
        assert!(formatted.contains('\n'));
    }
}
