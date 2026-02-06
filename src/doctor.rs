use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::git::GitOps;

/// Status of a single doctor check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

/// Result of a single doctor check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

/// Full doctor report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub checks: Vec<DoctorCheck>,
    pub overall: DoctorStatus,
}

impl DoctorReport {
    pub fn has_failures(&self) -> bool {
        self.overall == DoctorStatus::Fail
    }
}

/// Run all doctor checks and produce a report.
pub fn run_doctor(git_ops: &dyn GitOps, git_dir: &PathBuf) -> Result<DoctorReport> {
    let mut checks = Vec::new();

    checks.push(check_version());
    checks.push(check_notes_ref(git_ops));
    checks.push(check_hooks(git_dir));
    checks.push(check_credentials());
    checks.push(check_config(git_ops));

    let overall = if checks.iter().any(|c| c.status == DoctorStatus::Fail) {
        DoctorStatus::Fail
    } else if checks.iter().any(|c| c.status == DoctorStatus::Warn) {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Pass
    };

    Ok(DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks,
        overall,
    })
}

/// Check: report binary version (always passes).
fn check_version() -> DoctorCheck {
    DoctorCheck {
        name: "version".to_string(),
        status: DoctorStatus::Pass,
        message: format!("ultragit {}", env!("CARGO_PKG_VERSION")),
        fix_hint: None,
    }
}

/// Check: notes ref exists.
fn check_notes_ref(git_ops: &dyn GitOps) -> DoctorCheck {
    match git_ops.resolve_ref("refs/notes/ultragit") {
        Ok(_) => DoctorCheck {
            name: "notes_ref".to_string(),
            status: DoctorStatus::Pass,
            message: "refs/notes/ultragit exists".to_string(),
            fix_hint: None,
        },
        Err(_) => DoctorCheck {
            name: "notes_ref".to_string(),
            status: DoctorStatus::Warn,
            message: "refs/notes/ultragit not found (no annotations yet)".to_string(),
            fix_hint: Some("Run `ultragit annotate --commit HEAD` to create the first annotation.".to_string()),
        },
    }
}

/// Check: hooks installed.
fn check_hooks(git_dir: &PathBuf) -> DoctorCheck {
    let hooks_dir = git_dir.join("hooks");
    let post_commit = hooks_dir.join("post-commit");

    if post_commit.exists() {
        let content = std::fs::read_to_string(&post_commit).unwrap_or_default();
        if content.contains("ultragit") {
            DoctorCheck {
                name: "hooks".to_string(),
                status: DoctorStatus::Pass,
                message: "post-commit hook installed".to_string(),
                fix_hint: None,
            }
        } else {
            DoctorCheck {
                name: "hooks".to_string(),
                status: DoctorStatus::Warn,
                message: "post-commit hook exists but does not reference ultragit".to_string(),
                fix_hint: Some("Run `ultragit init` to reinstall hooks.".to_string()),
            }
        }
    } else {
        DoctorCheck {
            name: "hooks".to_string(),
            status: DoctorStatus::Fail,
            message: "post-commit hook not installed".to_string(),
            fix_hint: Some("Run `ultragit init` to install hooks.".to_string()),
        }
    }
}

/// Check: API credentials available.
fn check_credentials() -> DoctorCheck {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        DoctorCheck {
            name: "credentials".to_string(),
            status: DoctorStatus::Pass,
            message: "ANTHROPIC_API_KEY found".to_string(),
            fix_hint: None,
        }
    } else {
        DoctorCheck {
            name: "credentials".to_string(),
            status: DoctorStatus::Fail,
            message: "ANTHROPIC_API_KEY not set".to_string(),
            fix_hint: Some("Set the ANTHROPIC_API_KEY environment variable.".to_string()),
        }
    }
}

/// Check: ultragit config is valid.
fn check_config(git_ops: &dyn GitOps) -> DoctorCheck {
    match git_ops.config_get("ultragit.enabled") {
        Ok(Some(val)) if val == "true" || val == "1" => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Pass,
            message: "ultragit is enabled".to_string(),
            fix_hint: None,
        },
        Ok(_) => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Fail,
            message: "ultragit is not enabled in git config".to_string(),
            fix_hint: Some("Run `ultragit init` to initialize.".to_string()),
        },
        Err(_) => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Fail,
            message: "could not read git config".to_string(),
            fix_hint: Some("Run `ultragit init` to initialize.".to_string()),
        },
    }
}
