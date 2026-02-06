use std::fs::File;
use std::io::BufReader;

use crate::error::Result;
use crate::git::CliOps;
use crate::import::import_annotations;

/// Run `git chronicle import`.
pub fn run(file: String, force: bool, dry_run: bool) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let f = File::open(&file).map_err(|e| crate::error::ChronicleError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let reader = BufReader::new(f);

    let summary = import_annotations(&git_ops, reader, force, dry_run)?;

    if dry_run {
        eprintln!("dry run: would import {} annotations", summary.imported);
    } else {
        eprintln!("imported {} annotations", summary.imported);
    }

    if summary.skipped_existing > 0 {
        eprintln!("  skipped {} (already annotated)", summary.skipped_existing);
    }
    if summary.skipped_not_found > 0 {
        eprintln!("  skipped {} (commit not found)", summary.skipped_not_found);
    }
    if summary.skipped_invalid > 0 {
        eprintln!("  skipped {} (invalid entry)", summary.skipped_invalid);
    }

    Ok(())
}
