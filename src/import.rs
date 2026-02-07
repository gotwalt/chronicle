use std::io::BufRead;

use crate::error::chronicle_error::GitSnafu;
use crate::error::Result;
use crate::export::ExportEntry;
use crate::git::GitOps;
use crate::schema;
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
/// 1. Validate the annotation can be parsed (v1 or v2).
/// 2. Check if the commit SHA exists locally.
/// 3. If the commit has no existing note (or `force` is set), write the annotation.
/// 4. Otherwise skip.
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
        let line = line.map_err(|e| crate::error::ChronicleError::Io {
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

        // Validate annotation by trying to parse it (handles both v1 and v2)
        let annotation_json = serde_json::to_string(&entry.annotation).unwrap_or_default();
        if schema::parse_annotation(&annotation_json).is_err() {
            summary.skipped_invalid += 1;
            continue;
        }

        // Check if commit exists locally
        let commit_exists = git_ops.commit_info(&entry.commit_sha).is_ok();

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
            // Write the raw annotation JSON (preserving original format)
            git_ops
                .note_write(&entry.commit_sha, &annotation_json)
                .context(GitSnafu)?;
        }

        summary.imported += 1;
    }

    Ok(summary)
}
