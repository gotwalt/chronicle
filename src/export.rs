use std::io::Write;

use crate::error::chronicle_error::GitSnafu;
use crate::error::Result;
use crate::git::GitOps;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

/// A single export entry: commit SHA + timestamp + raw annotation JSON.
///
/// The annotation field is `serde_json::Value` so we can export both v1 and v2
/// annotations without needing to know the schema version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEntry {
    pub commit_sha: String,
    pub timestamp: String,
    pub annotation: serde_json::Value,
}

/// Export annotations as JSONL to a writer.
///
/// Iterates all notes under `refs/notes/chronicle`, and writes one JSON object
/// per line. Preserves the raw annotation format (v1 or v2).
pub fn export_annotations<W: Write>(git_ops: &dyn GitOps, writer: &mut W) -> Result<usize> {
    let note_list = list_annotated_commits(git_ops)?;
    let mut count = 0;

    for sha in &note_list {
        let note_content = match git_ops.note_read(sha).context(GitSnafu)? {
            Some(content) => content,
            None => continue,
        };

        let annotation: serde_json::Value = match serde_json::from_str(&note_content) {
            Ok(a) => a,
            Err(_) => continue, // skip malformed notes
        };

        let timestamp = annotation
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let entry = ExportEntry {
            commit_sha: sha.clone(),
            timestamp,
            annotation,
        };

        let line =
            serde_json::to_string(&entry).map_err(|e| crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            })?;

        writeln!(writer, "{line}").map_err(|e| crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

        count += 1;
    }

    Ok(count)
}

/// List all commit SHAs that have chronicle notes.
fn list_annotated_commits(_git_ops: &dyn GitOps) -> Result<Vec<String>> {
    // git notes --ref=refs/notes/chronicle list outputs: <note-sha> <commit-sha>
    // We use the CliOps internals indirectly — iterate by using a known set.
    // Since GitOps doesn't expose `notes list`, we shell out directly.
    let output = std::process::Command::new("git")
        .args(["notes", "--ref", "refs/notes/chronicle", "list"])
        .output()
        .map_err(|e| crate::error::ChronicleError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

    if !output.status.success() {
        // No notes ref yet — return empty
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let shas: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            // Format: <note-object-sha> <commit-sha>
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(shas)
}
