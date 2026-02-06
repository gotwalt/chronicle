use crate::config::user_config::UserConfig;
use crate::error::chronicle_error::SetupSnafu;
use crate::error::Result;
use snafu::ResultExt;

pub fn run() -> Result<()> {
    // Load existing config if present
    let existing = UserConfig::load().context(SetupSnafu)?;

    if let Some(ref config) = existing {
        eprintln!("Current provider: {}", config.provider.provider_type);
    } else {
        eprintln!("No existing configuration found.");
    }

    // Prompt for new provider
    let new_provider = crate::setup::prompt_provider_selection().context(SetupSnafu)?;
    let new_config = UserConfig {
        provider: new_provider,
    };

    new_config.save().context(SetupSnafu)?;
    eprintln!("Configuration updated: provider = {}", new_config.provider.provider_type);

    Ok(())
}
