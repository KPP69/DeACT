//! Minimal configuration management (Serde + TOML).
//!
//! The configuration file lives only at `data/config/deact.toml`. On first
//! launch a default configuration is generated; later launches load the
//! existing file. A corrupt file is reported as an error and is never
//! silently overwritten, so a user's configuration cannot be destroyed by a
//! bad parse. Only application-level, non-sensitive settings belong in
//! this file: API secrets, passwords and Telegram sessions must never be
//! stored here.

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Errors that can occur while loading or saving configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read or written.
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// The file exists but is not valid TOML / does not match the schema.
    /// The file is left untouched.
    Parse {
        path: std::path::PathBuf,
        source: toml::de::Error,
    },
    /// Serialising the configuration failed.
    Serialize(toml::ser::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(
                    f,
                    "cannot access configuration file {}: {source}",
                    path.display()
                )
            }
            ConfigError::Parse { path, source } => {
                write!(
                    f,
                    "configuration file {} is invalid and was not modified: {source}",
                    path.display()
                )
            }
            ConfigError::Serialize(source) => {
                write!(f, "cannot serialise configuration: {source}")
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
            ConfigError::Serialize(source) => Some(source),
        }
    }
}

/// Minimal application configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Configuration schema version.
    pub version: u32,
    /// Initial window width in logical pixels.
    pub window_width: u32,
    /// Initial window height in logical pixels.
    pub window_height: u32,
    /// Log level (`error`, `warn`, `info`, `debug`, `trace`). Reserved for
    /// future use; logging currently always uses `info` and above.
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            window_width: 900,
            window_height: 600,
            log_level: "info".to_string(),
        }
    }
}

/// Load the configuration from `path`. A parse failure is an error and the
/// file is not modified.
pub fn load(path: &Path) -> Result<AppConfig, ConfigError> {
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Save the configuration to `path`, overwriting it.
pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    let text = toml::to_string_pretty(config).map_err(ConfigError::Serialize)?;
    fs::write(path, text).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Load the configuration, or generate and save the default when the file
/// does not exist yet. Returns the configuration and whether it was newly
/// created. A corrupt existing file is an error, never a silent reset.
pub fn load_or_create(path: &Path) -> Result<(AppConfig, bool), ConfigError> {
    if path.exists() {
        Ok((load(path)?, false))
    } else {
        let config = AppConfig::default();
        save(path, &config)?;
        Ok((config, true))
    }
}
