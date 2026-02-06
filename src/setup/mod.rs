pub mod embedded;

use std::path::{Path, PathBuf};

use crate::error::SetupError;
use crate::error::setup_error::{
    BinaryNotFoundSnafu, InteractiveInputSnafu, NoHomeDirectorySnafu,
    WriteFileSnafu, ReadFileSnafu,
};
use crate::config::user_config::{ProviderConfig, ProviderType, UserConfig};
use snafu::ResultExt;

const CLAUDE_MD_BEGIN: &str = "<!-- chronicle-setup-begin -->";
const CLAUDE_MD_END: &str = "<!-- chronicle-setup-end -->";

/// Options for the setup command.
#[derive(Debug)]
pub struct SetupOptions {
    pub force: bool,
    pub dry_run: bool,
    pub skip_skills: bool,
    pub skip_hooks: bool,
    pub skip_claude_md: bool,
}

/// Report of what setup did.
#[derive(Debug)]
pub struct SetupReport {
    pub provider_type: ProviderType,
    pub config_path: PathBuf,
    pub skills_installed: Vec<PathBuf>,
    pub hooks_installed: Vec<PathBuf>,
    pub claude_md_updated: bool,
}

/// Run the full setup process.
pub fn run_setup(options: &SetupOptions) -> Result<SetupReport, SetupError> {
    let home = home_dir()?;

    // 1. Verify binary on PATH
    verify_binary_on_path()?;

    // 2. Prompt for provider selection
    let provider_config = if options.dry_run {
        eprintln!("[dry-run] Would prompt for provider selection");
        ProviderConfig {
            provider_type: ProviderType::ClaudeCode,
            model: None,
            api_key_env: None,
        }
    } else {
        prompt_provider_selection()?
    };

    let provider_type = provider_config.provider_type.clone();

    // 3. Write user config
    let config_path = UserConfig::path()?;
    let user_config = UserConfig {
        provider: provider_config,
    };
    if options.dry_run {
        eprintln!("[dry-run] Would write {}", config_path.display());
    } else {
        user_config.save()?;
    }

    // 4. Install skills
    let mut skills_installed = Vec::new();
    if !options.skip_skills {
        skills_installed = install_skills(&home, options)?;
    }

    // 5. Install hooks
    let mut hooks_installed = Vec::new();
    if !options.skip_hooks {
        hooks_installed = install_hooks(&home, options)?;
    }

    // 6. Update CLAUDE.md
    let claude_md_updated = if !options.skip_claude_md {
        update_claude_md(&home, options)?
    } else {
        false
    };

    Ok(SetupReport {
        provider_type,
        config_path,
        skills_installed,
        hooks_installed,
        claude_md_updated,
    })
}

fn home_dir() -> Result<PathBuf, SetupError> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| NoHomeDirectorySnafu.build())
}

/// Verify that git-chronicle is accessible on PATH.
fn verify_binary_on_path() -> Result<(), SetupError> {
    match std::process::Command::new("git-chronicle")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        _ => BinaryNotFoundSnafu.fail(),
    }
}

/// Interactive provider selection prompt.
pub fn prompt_provider_selection() -> Result<ProviderConfig, SetupError> {
    eprintln!();
    eprintln!("Select LLM provider for batch annotation:");
    eprintln!("  [1] Claude Code (recommended) — uses existing Claude Code auth");
    eprintln!("  [2] Anthropic API key — uses ANTHROPIC_API_KEY env var");
    eprintln!("  [3] None — skip for now, live path still works");
    eprintln!();
    eprint!("Choice [1]: ");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context(InteractiveInputSnafu)?;
    let choice = input.trim();

    match choice {
        "" | "1" => {
            // Validate claude CLI exists
            let claude_ok = std::process::Command::new("claude")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !claude_ok {
                eprintln!("warning: `claude` CLI not found on PATH. Install Claude Code to use this provider.");
            }

            Ok(ProviderConfig {
                provider_type: ProviderType::ClaudeCode,
                model: None,
                api_key_env: None,
            })
        }
        "2" => {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                eprintln!("warning: ANTHROPIC_API_KEY is not currently set.");
            }
            Ok(ProviderConfig {
                provider_type: ProviderType::Anthropic,
                model: None,
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            })
        }
        "3" => Ok(ProviderConfig {
            provider_type: ProviderType::None,
            model: None,
            api_key_env: None,
        }),
        _ => {
            eprintln!("Invalid choice, defaulting to Claude Code");
            Ok(ProviderConfig {
                provider_type: ProviderType::ClaudeCode,
                model: None,
                api_key_env: None,
            })
        }
    }
}

/// Install skill files to ~/.claude/skills/chronicle/.
fn install_skills(home: &Path, options: &SetupOptions) -> Result<Vec<PathBuf>, SetupError> {
    let skills = [
        ("context/SKILL.md", embedded::SKILL_CONTEXT),
        ("annotate/SKILL.md", embedded::SKILL_ANNOTATE),
        ("backfill/SKILL.md", embedded::SKILL_BACKFILL),
    ];

    let base = home.join(".claude").join("skills").join("chronicle");
    let mut installed = Vec::new();

    for (rel_path, content) in &skills {
        let full_path = base.join(rel_path);
        if options.dry_run {
            eprintln!("[dry-run] Would create {}", full_path.display());
        } else {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).context(WriteFileSnafu {
                    path: parent.display().to_string(),
                })?;
            }
            std::fs::write(&full_path, content).context(WriteFileSnafu {
                path: full_path.display().to_string(),
            })?;
        }
        installed.push(full_path);
    }

    Ok(installed)
}

/// Install hook files to ~/.claude/hooks/.
fn install_hooks(home: &Path, options: &SetupOptions) -> Result<Vec<PathBuf>, SetupError> {
    let hooks = [
        (
            "post-tool-use/chronicle-annotate-reminder.sh",
            embedded::HOOK_ANNOTATE_REMINDER,
        ),
        (
            "pre-tool-use/chronicle-read-context-hint.sh",
            embedded::HOOK_READ_CONTEXT_HINT,
        ),
    ];

    let base = home.join(".claude").join("hooks");
    let mut installed = Vec::new();

    for (rel_path, content) in &hooks {
        let full_path = base.join(rel_path);
        if options.dry_run {
            eprintln!("[dry-run] Would create {}", full_path.display());
        } else {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).context(WriteFileSnafu {
                    path: parent.display().to_string(),
                })?;
            }
            std::fs::write(&full_path, content).context(WriteFileSnafu {
                path: full_path.display().to_string(),
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(&full_path, perms).context(WriteFileSnafu {
                    path: full_path.display().to_string(),
                })?;
            }
        }
        installed.push(full_path);
    }

    Ok(installed)
}

/// Update ~/.claude/CLAUDE.md with marker-delimited Chronicle section.
fn update_claude_md(home: &Path, options: &SetupOptions) -> Result<bool, SetupError> {
    let claude_md_path = home.join(".claude").join("CLAUDE.md");

    if options.dry_run {
        if claude_md_path.exists() {
            eprintln!(
                "[dry-run] Would update {} (add/replace Chronicle section)",
                claude_md_path.display()
            );
        } else {
            eprintln!(
                "[dry-run] Would create {} with Chronicle section",
                claude_md_path.display()
            );
        }
        return Ok(true);
    }

    if let Some(parent) = claude_md_path.parent() {
        std::fs::create_dir_all(parent).context(WriteFileSnafu {
            path: parent.display().to_string(),
        })?;
    }

    let existing = if claude_md_path.exists() {
        std::fs::read_to_string(&claude_md_path).context(ReadFileSnafu {
            path: claude_md_path.display().to_string(),
        })?
    } else {
        String::new()
    };

    let snippet = embedded::CLAUDE_MD_SNIPPET;
    let new_content = apply_marker_content(&existing, snippet);

    std::fs::write(&claude_md_path, &new_content).context(WriteFileSnafu {
        path: claude_md_path.display().to_string(),
    })?;

    Ok(true)
}

/// Apply marker-delimited content to a string.
/// - If markers exist, replace content between them.
/// - If no markers, append the content.
/// - If the string is empty, just use the content.
pub fn apply_marker_content(existing: &str, snippet: &str) -> String {
    if existing.contains(CLAUDE_MD_BEGIN) && existing.contains(CLAUDE_MD_END) {
        // Replace content between markers (inclusive)
        let mut result = String::new();
        let mut in_section = false;
        let mut replaced = false;
        for line in existing.lines() {
            if line.contains(CLAUDE_MD_BEGIN) {
                in_section = true;
                if !replaced {
                    result.push_str(snippet);
                    result.push('\n');
                    replaced = true;
                }
                continue;
            }
            if line.contains(CLAUDE_MD_END) {
                in_section = false;
                continue;
            }
            if !in_section {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    } else if existing.is_empty() {
        format!("{snippet}\n")
    } else {
        let mut content = existing.to_string();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(snippet);
        content.push('\n');
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_marker_empty_file() {
        let result = apply_marker_content("", "<!-- chronicle-setup-begin -->\nHello\n<!-- chronicle-setup-end -->");
        assert!(result.contains("<!-- chronicle-setup-begin -->"));
        assert!(result.contains("Hello"));
        assert!(result.contains("<!-- chronicle-setup-end -->"));
    }

    #[test]
    fn test_apply_marker_no_markers() {
        let existing = "# My Project\n\nSome content.\n";
        let snippet = "<!-- chronicle-setup-begin -->\nChronicle section\n<!-- chronicle-setup-end -->";
        let result = apply_marker_content(existing, snippet);
        assert!(result.starts_with("# My Project"));
        assert!(result.contains("Chronicle section"));
        assert!(result.contains("<!-- chronicle-setup-begin -->"));
    }

    #[test]
    fn test_apply_marker_existing_markers() {
        let existing = "# My Project\n\n<!-- chronicle-setup-begin -->\nOld content\n<!-- chronicle-setup-end -->\n\nOther stuff\n";
        let snippet = "<!-- chronicle-setup-begin -->\nNew content\n<!-- chronicle-setup-end -->";
        let result = apply_marker_content(existing, snippet);
        assert!(result.contains("New content"));
        assert!(!result.contains("Old content"));
        assert!(result.contains("Other stuff"));
    }

    #[test]
    fn test_apply_marker_idempotent() {
        let snippet = "<!-- chronicle-setup-begin -->\nChronicle section\n<!-- chronicle-setup-end -->";
        let first = apply_marker_content("", snippet);
        let second = apply_marker_content(&first, snippet);
        assert_eq!(first, second);
    }
}
