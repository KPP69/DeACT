//! Portable data directory resolution.
//!
//! The data directory is always `DeACT_Data/` beside the running
//! executable, resolved from `std::env::current_exe()`. It is never derived
//! from the current working directory, the user home directory, Documents,
//! AppData/LocalAppData, the Registry, or any other system location, and
//! there is no fallback: if the executable directory is not writable the
//! program must report the problem and exit.

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Name of the portable data directory beside the executable.
pub const DATA_DIR_NAME: &str = "DeACT_Data";

/// Errors that can occur while resolving or creating the data directory.
#[derive(Debug)]
pub enum PathsError {
    /// `std::env::current_exe()` failed.
    ExePathUnavailable(std::io::Error),
    /// The executable path has no parent directory.
    NoParentDirectory(PathBuf),
    /// A required directory could not be created.
    CreateFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The data directory exists but is not writable.
    NotWritable {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for PathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathsError::ExePathUnavailable(source) => {
                write!(f, "cannot determine the executable path: {source}")
            }
            PathsError::NoParentDirectory(path) => {
                write!(
                    f,
                    "executable path has no parent directory: {}",
                    path.display()
                )
            }
            PathsError::CreateFailed { path, source } => {
                write!(f, "cannot create directory {}: {source}", path.display())
            }
            PathsError::NotWritable { path, source } => {
                write!(
                    f,
                    "data directory {} is not writable: {source}. \
                     DeACT refuses to fall back to another location; \
                     run it from a writable directory",
                    path.display()
                )
            }
        }
    }
}

impl Error for PathsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PathsError::ExePathUnavailable(source)
            | PathsError::CreateFailed { source, .. }
            | PathsError::NotWritable { source, .. } => Some(source),
            PathsError::NoParentDirectory(_) => None,
        }
    }
}

/// All paths DeACT uses, derived from the executable location.
#[derive(Debug, Clone)]
pub struct PortablePaths {
    exe_dir: PathBuf,
    data_dir: PathBuf,
    logs_dir: PathBuf,
    cache_dir: PathBuf,
    backups_dir: PathBuf,
    config_path: PathBuf,
    database_path: PathBuf,
}

impl PortablePaths {
    /// Directory containing the running executable.
    pub fn exe_dir(&self) -> &Path {
        &self.exe_dir
    }

    /// `DeACT_Data/` beside the executable.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `DeACT_Data/logs/`.
    pub fn logs_dir(&self) -> &Path {
        &self.logs_dir
    }

    /// `DeACT_Data/cache/`.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// `DeACT_Data/backups/`.
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    /// `DeACT_Data/config.toml`.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// `DeACT_Data/deact.db`.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

/// Data directory for a given executable path: `<exe parent>/DeACT_Data`.
pub fn data_dir_for_exe(exe_path: &Path) -> Result<PathBuf, PathsError> {
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| PathsError::NoParentDirectory(exe_path.to_path_buf()))?;
    Ok(exe_dir.join(DATA_DIR_NAME))
}

/// Resolve paths from the running executable and create the data
/// directories. Returns an error (never a fallback location) when the
/// executable directory cannot be used.
pub fn resolve() -> Result<PortablePaths, PathsError> {
    let exe_path = std::env::current_exe().map_err(PathsError::ExePathUnavailable)?;
    resolve_for_exe(&exe_path)
}

/// Same as [`resolve`], but for an explicit executable path, so the logic
/// can be tested without relocating the test binary.
pub fn resolve_for_exe(exe_path: &Path) -> Result<PortablePaths, PathsError> {
    let data_dir = data_dir_for_exe(exe_path)?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| PathsError::NoParentDirectory(exe_path.to_path_buf()))?
        .to_path_buf();
    let paths = PortablePaths {
        logs_dir: data_dir.join("logs"),
        cache_dir: data_dir.join("cache"),
        backups_dir: data_dir.join("backups"),
        config_path: data_dir.join("config.toml"),
        database_path: data_dir.join("deact.db"),
        exe_dir,
        data_dir,
    };
    ensure_data_dirs(&paths)?;
    Ok(paths)
}

/// Create `DeACT_Data/`, `logs/`, `cache/` and `backups/`, then prove the
/// data directory is writable. A failed probe is a hard error.
fn ensure_data_dirs(paths: &PortablePaths) -> Result<(), PathsError> {
    for dir in [
        &paths.data_dir,
        &paths.logs_dir,
        &paths.cache_dir,
        &paths.backups_dir,
    ] {
        fs::create_dir_all(dir).map_err(|source| PathsError::CreateFailed {
            path: dir.clone(),
            source,
        })?;
    }
    // Writability probe: create and remove a temporary file. This must not
    // be skipped — a directory that only looks writable is not enough.
    let probe = paths.data_dir.join(".deact-write-test");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(source) => Err(PathsError::NotWritable {
            path: paths.data_dir.clone(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_beside_the_executable() {
        let exe = Path::new("/tmp/somewhere/bin/DeACT.exe");
        let dir = data_dir_for_exe(exe).expect("data dir resolves");
        assert_eq!(dir, Path::new("/tmp/somewhere/bin/DeACT_Data"));
    }
}
