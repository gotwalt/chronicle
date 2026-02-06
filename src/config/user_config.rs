use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SetupError;
use crate::error::setup_error::{
    NoHomeDirectorySnafu, ReadConfigSnafu, ReadFileSnafu, WriteConfigSnafu, WriteFileSnafu,
};
use snafu::ResultExt;

/// User-level config stored at ~/.git-chronicle.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserConfig {
    pub provider: ProviderConfig,
}

/// Provider configuration within user config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

/// Supported provider types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderType {
    ClaudeCode,
    Anthropic,
    None,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::ClaudeCode => write!(f, "claude-code"),
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::None => write!(f, "none"),
        }
    }
}

impl UserConfig {
    /// Path to the user config file (~/.git-chronicle.toml).
    pub fn path() -> Result<PathBuf, SetupError> {
        let home = std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| NoHomeDirectorySnafu.build())?;
        Ok(home.join(".git-chronicle.toml"))
    }

    /// Load user config from ~/.git-chronicle.toml.
    /// Returns Ok(None) if the file does not exist.
    pub fn load() -> Result<Option<Self>, SetupError> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path).context(ReadFileSnafu {
            path: path.display().to_string(),
        })?;
        let config: UserConfig = toml::from_str(&contents).context(ReadConfigSnafu)?;
        Ok(Some(config))
    }

    /// Save user config to ~/.git-chronicle.toml.
    pub fn save(&self) -> Result<(), SetupError> {
        let path = Self::path()?;
        let contents = toml::to_string_pretty(self).context(WriteConfigSnafu)?;
        std::fs::write(&path, contents).context(WriteFileSnafu {
            path: path.display().to_string(),
        })?;
        Ok(())
    }
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            provider: ProviderConfig {
                provider_type: ProviderType::None,
                model: None,
                api_key_env: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_serialization() {
        let config = UserConfig {
            provider: ProviderConfig {
                provider_type: ProviderType::ClaudeCode,
                model: None,
                api_key_env: None,
            },
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("\"claude-code\""));

        let config2 = UserConfig {
            provider: ProviderConfig {
                provider_type: ProviderType::Anthropic,
                model: Some("claude-sonnet-4-5-20250929".to_string()),
                api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            },
        };
        let toml_str2 = toml::to_string_pretty(&config2).unwrap();
        assert!(toml_str2.contains("\"anthropic\""));
        assert!(toml_str2.contains("claude-sonnet-4-5-20250929"));
    }

    #[test]
    fn test_roundtrip() {
        let config = UserConfig {
            provider: ProviderConfig {
                provider_type: ProviderType::ClaudeCode,
                model: Some("claude-sonnet-4-5-20250929".to_string()),
                api_key_env: None,
            },
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: UserConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_none_provider() {
        let config = UserConfig {
            provider: ProviderConfig {
                provider_type: ProviderType::None,
                model: None,
                api_key_env: None,
            },
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("\"none\""));
        let parsed: UserConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::ClaudeCode.to_string(), "claude-code");
        assert_eq!(ProviderType::Anthropic.to_string(), "anthropic");
        assert_eq!(ProviderType::None.to_string(), "none");
    }
}
