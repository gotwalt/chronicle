use crate::error::Result;
use crate::error::ultragit_error::GitSnafu;
use crate::git::GitOps;
use snafu::ResultExt;

/// Ultragit configuration, assembled from defaults + git config.
#[derive(Debug, Clone)]
pub struct UltragitConfig {
    pub enabled: bool,
    pub sync: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub notes_ref: String,
    pub max_diff_lines: u32,
    pub skip_trivial: bool,
    pub trivial_threshold: u32,
}

impl Default for UltragitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sync: false,
            provider: None,
            model: None,
            notes_ref: "refs/notes/ultragit".to_string(),
            max_diff_lines: 2000,
            skip_trivial: true,
            trivial_threshold: 3,
        }
    }
}

/// Load config from git config, merging with defaults.
pub fn load_config(git_ops: &dyn GitOps) -> Result<UltragitConfig> {
    let mut config = UltragitConfig::default();

    if let Some(val) = git_ops.config_get("ultragit.enabled").context(GitSnafu)? {
        config.enabled = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("ultragit.sync").context(GitSnafu)? {
        config.sync = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("ultragit.provider").context(GitSnafu)? {
        config.provider = Some(val);
    }

    if let Some(val) = git_ops.config_get("ultragit.model").context(GitSnafu)? {
        config.model = Some(val);
    }

    if let Some(val) = git_ops.config_get("ultragit.noteref").context(GitSnafu)? {
        config.notes_ref = val;
    }

    if let Some(val) = git_ops.config_get("ultragit.maxdifflines").context(GitSnafu)? {
        if let Ok(n) = val.parse::<u32>() {
            config.max_diff_lines = n;
        }
    }

    if let Some(val) = git_ops.config_get("ultragit.skiptrivial").context(GitSnafu)? {
        config.skip_trivial = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("ultragit.trivialthreshold").context(GitSnafu)? {
        if let Ok(n) = val.parse::<u32>() {
            config.trivial_threshold = n;
        }
    }

    Ok(config)
}
