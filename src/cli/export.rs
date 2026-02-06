use std::fs::File;
use std::io::{self, BufWriter};

use crate::error::Result;
use crate::export::export_annotations;
use crate::git::CliOps;

/// Run `ultragit export`.
pub fn run(output: Option<String>) -> Result<()> {
    let repo_dir = std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
        source: e,
        location: snafu::Location::default(),
    })?;
    let git_ops = CliOps::new(repo_dir);

    let count = match output {
        Some(path) => {
            let file = File::create(&path).map_err(|e| crate::error::UltragitError::Io {
                source: e,
                location: snafu::Location::default(),
            })?;
            let mut writer = BufWriter::new(file);
            let c = export_annotations(&git_ops, &mut writer)?;
            eprintln!("exported {c} annotations to {path}");
            c
        }
        None => {
            let stdout = io::stdout();
            let mut writer = BufWriter::new(stdout.lock());
            let c = export_annotations(&git_ops, &mut writer)?;
            eprintln!("exported {c} annotations");
            c
        }
    };

    if count == 0 {
        eprintln!("no annotations found to export");
    }

    Ok(())
}
