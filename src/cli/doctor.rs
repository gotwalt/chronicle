use std::path::PathBuf;

use crate::doctor::{run_doctor, DoctorStatus};
use crate::error::Result;
use crate::git::CliOps;

/// Run `ultragit doctor`.
pub fn run(json: bool) -> Result<()> {
    let git_dir = find_git_dir()?;
    let repo_dir = git_dir.parent().unwrap_or(&git_dir).to_path_buf();
    let git_ops = CliOps::new(repo_dir);

    let report = run_doctor(&git_ops, &git_dir)?;

    if json {
        let output = serde_json::to_string_pretty(&report).map_err(|e| {
            crate::error::UltragitError::Json {
                source: e,
                location: snafu::Location::default(),
            }
        })?;
        println!("{output}");
    } else {
        println!("ultragit doctor");
        for check in &report.checks {
            let icon = match check.status {
                DoctorStatus::Pass => "pass",
                DoctorStatus::Warn => "warn",
                DoctorStatus::Fail => "FAIL",
            };
            println!("  [{icon}] {}: {}", check.name, check.message);
            if let Some(ref hint) = check.fix_hint {
                println!("         {hint}");
            }
        }
        println!();
        let overall = match report.overall {
            DoctorStatus::Pass => "all checks passed",
            DoctorStatus::Warn => "some warnings",
            DoctorStatus::Fail => "some checks failed",
        };
        println!("Overall: {overall}");
    }

    if report.has_failures() {
        std::process::exit(1);
    }

    Ok(())
}

/// Find the .git directory.
fn find_git_dir() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map_err(|e| crate::error::UltragitError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;

    if output.status.success() {
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let path = PathBuf::from(&dir);
        if path.is_relative() {
            let cwd = std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
                source: e,
                location: snafu::Location::default(),
            })?;
            Ok(cwd.join(path))
        } else {
            Ok(path)
        }
    } else {
        let cwd = std::env::current_dir().map_err(|e| crate::error::UltragitError::Io {
            source: e,
            location: snafu::Location::default(),
        })?;
        Err(crate::error::ultragit_error::NotARepositorySnafu { path: cwd }.build())
    }
}
