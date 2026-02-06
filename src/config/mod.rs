pub mod user_config;

use crate::error::Result;
use crate::error::chronicle_error::GitSnafu;
use crate::git::GitOps;
use snafu::ResultExt;

/// Chronicle configuration, assembled from defaults + git config.
#[derive(Debug, Clone)]
pub struct ChronicleConfig {
    pub enabled: bool,
    pub sync: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub notes_ref: String,
    pub max_diff_lines: u32,
    pub skip_trivial: bool,
    pub trivial_threshold: u32,
}

impl Default for ChronicleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sync: false,
            provider: None,
            model: None,
            notes_ref: "refs/notes/chronicle".to_string(),
            max_diff_lines: 2000,
            skip_trivial: true,
            trivial_threshold: 3,
        }
    }
}

/// Load config from git config, merging with defaults.
pub fn load_config(git_ops: &dyn GitOps) -> Result<ChronicleConfig> {
    let mut config = ChronicleConfig::default();

    if let Some(val) = git_ops.config_get("chronicle.enabled").context(GitSnafu)? {
        config.enabled = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("chronicle.sync").context(GitSnafu)? {
        config.sync = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("chronicle.provider").context(GitSnafu)? {
        config.provider = Some(val);
    }

    if let Some(val) = git_ops.config_get("chronicle.model").context(GitSnafu)? {
        config.model = Some(val);
    }

    if let Some(val) = git_ops.config_get("chronicle.noteref").context(GitSnafu)? {
        config.notes_ref = val;
    }

    if let Some(val) = git_ops.config_get("chronicle.maxdifflines").context(GitSnafu)? {
        if let Ok(n) = val.parse::<u32>() {
            config.max_diff_lines = n;
        }
    }

    if let Some(val) = git_ops.config_get("chronicle.skiptrivial").context(GitSnafu)? {
        config.skip_trivial = val == "true" || val == "1";
    }

    if let Some(val) = git_ops.config_get("chronicle.trivialthreshold").context(GitSnafu)? {
        if let Ok(n) = val.parse::<u32>() {
            config.trivial_threshold = n;
        }
    }

    Ok(config)
}
