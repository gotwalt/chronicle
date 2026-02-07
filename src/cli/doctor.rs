use std::path::PathBuf;

use crate::doctor::{run_doctor, DoctorCheck, DoctorStatus};
use crate::error::Result;
use crate::git::{CliOps, GitOps};

/// Run `git chronicle doctor`.
pub fn run(json: bool, staleness: bool) -> Result<()> {
    let git_dir = find_git_dir()?;
    let repo_dir = git_dir.parent().unwrap_or(&git_dir).to_path_buf();
    let git_ops = CliOps::new(repo_dir);

    let mut report = run_doctor(&git_ops, &git_dir)?;

    if staleness {
        let staleness_check = check_staleness(&git_ops);
        if staleness_check.status == DoctorStatus::Warn
            && report.overall == DoctorStatus::Pass
        {
            report.overall = DoctorStatus::Warn;
        }
        report.checks.push(staleness_check);
    }

    if json {
        let output = serde_json::to_string_pretty(&report).map_err(|e| {
            crate::error::ChronicleError::Json {
                source: e,
                location: snafu::Location::default(),
            }
        })?;
        println!("{output}");
    } else {
        println!("chronicle doctor");
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

/// Check annotation staleness across the repo.
fn check_staleness(git_ops: &dyn GitOps) -> DoctorCheck {
    match crate::read::staleness::scan_staleness(git_ops, 50) {
        Ok(report) => {
            if report.stale_count == 0 {
                DoctorCheck {
                    name: "staleness".to_string(),
                    status: DoctorStatus::Pass,
                    message: format!(
                        "{} annotations checked, none stale",
                        report.total_annotations
                    ),
                    fix_hint: None,
                }
            } else {
                DoctorCheck {
                    name: "staleness".to_string(),
                    status: DoctorStatus::Warn,
                    message: format!(
                        "{} stale annotation(s) out of {} checked",
                        report.stale_count, report.total_annotations
                    ),
                    fix_hint: Some(
                        "Run `git chronicle annotate` on stale files to refresh annotations."
                            .to_string(),
                    ),
                }
            }
        }
        Err(_) => DoctorCheck {
            name: "staleness".to_string(),
            status: DoctorStatus::Warn,
            message: "could not check staleness".to_string(),
            fix_hint: None,
        },
    }
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
