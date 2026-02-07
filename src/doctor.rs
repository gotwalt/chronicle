use std::path::{Path, PathBuf};

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
pub fn run_doctor(git_ops: &dyn GitOps, git_dir: &Path) -> Result<DoctorReport> {
    let mut checks = vec![
        check_version(),
        check_notes_ref(git_ops),
        check_hooks(git_dir),
        check_credentials(),
        check_config(git_ops),
    ];
    checks.extend(check_global_setup());

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
        message: format!("chronicle {}", env!("CARGO_PKG_VERSION")),
        fix_hint: None,
    }
}

/// Check: notes ref exists.
fn check_notes_ref(git_ops: &dyn GitOps) -> DoctorCheck {
    match git_ops.resolve_ref("refs/notes/chronicle") {
        Ok(_) => DoctorCheck {
            name: "notes_ref".to_string(),
            status: DoctorStatus::Pass,
            message: "refs/notes/chronicle exists".to_string(),
            fix_hint: None,
        },
        Err(_) => DoctorCheck {
            name: "notes_ref".to_string(),
            status: DoctorStatus::Warn,
            message: "refs/notes/chronicle not found (no annotations yet)".to_string(),
            fix_hint: Some(
                "Run `git chronicle annotate --commit HEAD` to create the first annotation."
                    .to_string(),
            ),
        },
    }
}

/// Check: hooks installed.
fn check_hooks(git_dir: &Path) -> DoctorCheck {
    let hooks_dir = git_dir.join("hooks");
    let post_commit = hooks_dir.join("post-commit");

    if post_commit.exists() {
        let content = std::fs::read_to_string(&post_commit).unwrap_or_default();
        if content.contains("chronicle") {
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
                message: "post-commit hook exists but does not reference chronicle".to_string(),
                fix_hint: Some("Run `git chronicle init` to reinstall hooks.".to_string()),
            }
        }
    } else {
        DoctorCheck {
            name: "hooks".to_string(),
            status: DoctorStatus::Fail,
            message: "post-commit hook not installed".to_string(),
            fix_hint: Some("Run `git chronicle init` to install hooks.".to_string()),
        }
    }
}

/// Check: API credentials available.
fn check_credentials() -> DoctorCheck {
    use crate::config::user_config::{ProviderType, UserConfig};

    // Check user config first
    if let Ok(Some(config)) = UserConfig::load() {
        match config.provider.provider_type {
            ProviderType::ClaudeCode => {
                return DoctorCheck {
                    name: "credentials".to_string(),
                    status: DoctorStatus::Pass,
                    message: "provider: claude-code (uses Claude Code auth)".to_string(),
                    fix_hint: None,
                };
            }
            ProviderType::Anthropic => {
                if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    return DoctorCheck {
                        name: "credentials".to_string(),
                        status: DoctorStatus::Pass,
                        message: "provider: anthropic, ANTHROPIC_API_KEY found".to_string(),
                        fix_hint: None,
                    };
                } else {
                    return DoctorCheck {
                        name: "credentials".to_string(),
                        status: DoctorStatus::Fail,
                        message: "provider: anthropic, but ANTHROPIC_API_KEY not set".to_string(),
                        fix_hint: Some(
                            "Set the ANTHROPIC_API_KEY environment variable.".to_string(),
                        ),
                    };
                }
            }
            ProviderType::None => {}
        }
    }

    // Fall back to env var check
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
            message: "no LLM provider configured".to_string(),
            fix_hint: Some("Run `git chronicle setup` to configure a provider.".to_string()),
        }
    }
}

/// Check: global setup (user config, skills, hooks).
fn check_global_setup() -> Vec<DoctorCheck> {
    use crate::config::user_config::UserConfig;

    let mut checks = Vec::new();

    // Check user config exists
    match UserConfig::load() {
        Ok(Some(config)) => {
            checks.push(DoctorCheck {
                name: "global_config".to_string(),
                status: DoctorStatus::Pass,
                message: format!(
                    "~/.git-chronicle.toml exists (provider: {})",
                    config.provider.provider_type
                ),
                fix_hint: None,
            });
        }
        Ok(None) => {
            checks.push(DoctorCheck {
                name: "global_config".to_string(),
                status: DoctorStatus::Warn,
                message: "~/.git-chronicle.toml not found".to_string(),
                fix_hint: Some(
                    "Run `git chronicle setup` to configure Chronicle globally.".to_string(),
                ),
            });
        }
        Err(e) => {
            checks.push(DoctorCheck {
                name: "global_config".to_string(),
                status: DoctorStatus::Fail,
                message: format!("~/.git-chronicle.toml parse error: {e}"),
                fix_hint: Some(
                    "Run `git chronicle setup --force` to recreate the config file.".to_string(),
                ),
            });
        }
    }

    // Check skills directory
    if let Ok(home) = std::env::var("HOME") {
        let skills_dir = PathBuf::from(&home)
            .join(".claude")
            .join("skills")
            .join("chronicle");
        if skills_dir.exists() {
            checks.push(DoctorCheck {
                name: "global_skills".to_string(),
                status: DoctorStatus::Pass,
                message: "Claude Code skills installed".to_string(),
                fix_hint: None,
            });
        } else {
            checks.push(DoctorCheck {
                name: "global_skills".to_string(),
                status: DoctorStatus::Warn,
                message: "Claude Code skills not installed".to_string(),
                fix_hint: Some("Run `git chronicle setup` to install skills.".to_string()),
            });
        }
    }

    checks
}

/// Check: chronicle config is valid.
fn check_config(git_ops: &dyn GitOps) -> DoctorCheck {
    match git_ops.config_get("chronicle.enabled") {
        Ok(Some(val)) if val == "true" || val == "1" => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Pass,
            message: "chronicle is enabled".to_string(),
            fix_hint: None,
        },
        Ok(_) => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Fail,
            message: "chronicle is not enabled in git config".to_string(),
            fix_hint: Some("Run `git chronicle init` to initialize.".to_string()),
        },
        Err(_) => DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Fail,
            message: "could not read git config".to_string(),
            fix_hint: Some("Run `git chronicle init` to initialize.".to_string()),
        },
    }
}
