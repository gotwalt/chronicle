use std::io::BufRead;

use crate::error::ultragit_error::GitSnafu;
use crate::error::Result;
use crate::export::ExportEntry;
use crate::git::GitOps;
use snafu::ResultExt;

/// Summary of an import operation.
#[derive(Debug, Clone)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped_existing: usize,
    pub skipped_not_found: usize,
    pub skipped_invalid: usize,
}

/// Import annotations from a JSONL reader.
///
/// Each line is an `ExportEntry` JSON object. For each entry:
/// 1. Check if the commit SHA exists locally.
/// 2. If the commit has no existing note (or `force` is set), write the annotation.
/// 3. Otherwise skip.
pub fn import_annotations<R: BufRead>(
    git_ops: &dyn GitOps,
    reader: R,
    force: bool,
    dry_run: bool,
) -> Result<ImportSummary> {
    let mut summary = ImportSummary {
        imported: 0,
        skipped_existing: 0,
        skipped_not_found: 0,
        skipped_invalid: 0,
    };

    for line in reader.lines() {
        let line = line.map_err(|e| crate::error::UltragitError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: ExportEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                summary.skipped_invalid += 1;
                continue;
            }
        };

        // Validate annotation
        if let Err(_) = entry.annotation.validate() {
            summary.skipped_invalid += 1;
            continue;
        }

        // Check if commit exists locally
        let commit_exists = match git_ops.commit_info(&entry.commit_sha) {
            Ok(_) => true,
            Err(_) => false,
        };

        if !commit_exists {
            summary.skipped_not_found += 1;
            continue;
        }

        // Check if note already exists
        if !force {
            let has_note = git_ops.note_exists(&entry.commit_sha).context(GitSnafu)?;
            if has_note {
                summary.skipped_existing += 1;
                continue;
            }
        }

        if !dry_run {
            let content = serde_json::to_string(&entry.annotation).map_err(|e| {
                crate::error::UltragitError::Json {
                    source: e,
                    location: snafu::Location::default(),
                }
            })?;
            git_ops.note_write(&entry.commit_sha, &content).context(GitSnafu)?;
        }

        summary.imported += 1;
    }

    Ok(summary)
}
