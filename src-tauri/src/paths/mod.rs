//! Portable data directory resolution.
//!
//! The data directory is always `data/` beside the running
//! executable, resolved from `std::env::current_exe()`. It is never derived
//! from the current working directory, the user home directory, Documents,
//! AppData/LocalAppData, the Registry, or any other system location, and
//! there is no fallback: if the executable directory is not writable the
//! program must report the problem and exit.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Name of the portable data directory beside the executable.
pub const DATA_DIR_NAME: &str = "data";

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
    /// The writability probe was created but could not be removed.
    ProbeCleanupFailed {
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
            PathsError::ProbeCleanupFailed { path, source } => {
                write!(
                    f,
                    "could not remove writability probe {}: {source}",
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
            | PathsError::NotWritable { source, .. }
            | PathsError::ProbeCleanupFailed { source, .. } => Some(source),
            PathsError::NoParentDirectory(_) => None,
        }
    }
}

/// All paths DeACT uses, derived from the executable location.
#[derive(Debug, Clone)]
pub struct PortablePaths {
    exe_dir: PathBuf,
    data_dir: PathBuf,
    config_dir: PathBuf,
    logs_dir: PathBuf,
    cache_dir: PathBuf,
    backups_dir: PathBuf,
    sessions_dir: PathBuf,
    telegram_sessions_dir: PathBuf,
    keys_dir: PathBuf,
    webview_dir: PathBuf,
    config_path: PathBuf,
    database_path: PathBuf,
}

impl PortablePaths {
    /// Directory containing the running executable.
    pub fn exe_dir(&self) -> &Path {
        &self.exe_dir
    }

    /// `data/` beside the executable.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `data/config/`.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// `data/logs/`.
    pub fn logs_dir(&self) -> &Path {
        &self.logs_dir
    }

    /// `data/cache/`.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// `data/backups/`.
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }

    /// `data/sessions/`.
    pub fn sessions_dir(&self) -> &Path {
        &self.sessions_dir
    }

    /// `data/sessions/telegram/`.
    pub fn telegram_sessions_dir(&self) -> &Path {
        &self.telegram_sessions_dir
    }

    /// `data/keys/`.
    pub fn keys_dir(&self) -> &Path {
        &self.keys_dir
    }

    /// `data/webview/` (the WebView2 user data folder).
    pub fn webview_dir(&self) -> &Path {
        &self.webview_dir
    }

    /// `data/config/deact.toml`.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// `data/deact.db`.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

/// Data directory for a given executable path: `<exe parent>/data`.
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
    let config_dir = data_dir.join("config");
    let sessions_dir = data_dir.join("sessions");
    let paths = PortablePaths {
        logs_dir: data_dir.join("logs"),
        cache_dir: data_dir.join("cache"),
        backups_dir: data_dir.join("backups"),
        telegram_sessions_dir: sessions_dir.join("telegram"),
        keys_dir: data_dir.join("keys"),
        webview_dir: data_dir.join("webview"),
        config_path: config_dir.join("deact.toml"),
        database_path: data_dir.join("deact.db"),
        config_dir,
        sessions_dir,
        exe_dir,
        data_dir,
    };
    ensure_data_dirs(&paths)?;
    Ok(paths)
}

/// Create the `data/` directory tree, then prove the data directory is
/// writable. A failed probe is a hard error.
fn ensure_data_dirs(paths: &PortablePaths) -> Result<(), PathsError> {
    for dir in [
        &paths.data_dir,
        &paths.config_dir,
        &paths.logs_dir,
        &paths.cache_dir,
        &paths.backups_dir,
        &paths.sessions_dir,
        &paths.telegram_sessions_dir,
        &paths.keys_dir,
        &paths.webview_dir,
    ] {
        fs::create_dir_all(dir).map_err(|source| PathsError::CreateFailed {
            path: dir.clone(),
            source,
        })?;
    }
    // Writability probe: create and remove a temporary file. This must not
    // be skipped — a directory that only looks writable is not enough.
    let probe = create_write_probe(&paths.data_dir)?;
    remove_write_probe(&probe)?;
    Ok(())
}

/// Create a uniquely named, empty probe file with exclusive creation
/// (`create_new`), so an existing file can never be overwritten or
/// truncated — including a leftover probe from an earlier run. Only a
/// name collision is retried; any other failure means the directory is
/// not writable and is reported as such.
fn create_write_probe(data_dir: &Path) -> Result<PathBuf, PathsError> {
    let mut last_error = None;
    for attempt in 0..3 {
        let name = format!(
            ".deact-write-test-{}-{}-{attempt}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0),
        );
        let probe = data_dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(file) => {
                drop(file);
                return Ok(probe);
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(source);
            }
            Err(source) => {
                return Err(PathsError::NotWritable {
                    path: data_dir.to_path_buf(),
                    source,
                });
            }
        }
    }
    Err(PathsError::NotWritable {
        path: data_dir.to_path_buf(),
        source: last_error.expect("at least one probe attempt ran"),
    })
}

/// Remove the probe file. A failure is reported as
/// [`PathsError::ProbeCleanupFailed`], never silently ignored: a probe
/// left behind would pollute the user's data directory.
fn remove_write_probe(probe: &Path) -> Result<(), PathsError> {
    fs::remove_file(probe).map_err(|source| PathsError::ProbeCleanupFailed {
        path: probe.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("deact-paths-unit-{tag}-{}-{n}", std::process::id()))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn data_dir_is_beside_the_executable() {
        let exe = Path::new("/tmp/somewhere/bin/DeACT.exe");
        let dir = data_dir_for_exe(exe).expect("data dir resolves");
        assert_eq!(dir, Path::new("/tmp/somewhere/bin/data"));
    }

    #[test]
    fn probe_never_overwrites_an_existing_file() {
        let base = unique_temp_dir("probe-keep");
        let exe = base.join("bin").join("DeACT.exe");
        let data_dir = base.join("bin").join("data");
        fs::create_dir_all(&data_dir).expect("data dir");
        // A file with the old fixed probe name must survive resolution
        // untouched, with its contents intact.
        let sentinel = data_dir.join(".deact-write-test");
        fs::write(&sentinel, b"do not overwrite").expect("sentinel file");
        let paths = resolve_for_exe(&exe).expect("resolution succeeds");
        assert_eq!(
            fs::read(&sentinel).expect("sentinel still readable"),
            b"do not overwrite",
            "the probe must not overwrite an existing file"
        );
        // No probe file is left behind: the only entries are the sentinel
        // (which was already there) and the subdirectories.
        let mut entries: Vec<_> = fs::read_dir(paths.data_dir())
            .expect("read data dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        entries.sort();
        assert_eq!(
            entries,
            vec![
                ".deact-write-test",
                "backups",
                "cache",
                "config",
                "keys",
                "logs",
                "sessions",
                "webview"
            ]
        );
        cleanup(&base);
    }

    #[test]
    fn probe_cleanup_failure_is_reported() {
        // `remove_file` on a directory fails deterministically (even for a
        // privileged user, where permission bits alone would not block
        // removal), which exercises the cleanup-error path.
        let base = unique_temp_dir("probe-cleanup");
        let dir = base.join("not-a-file");
        fs::create_dir_all(&dir).expect("directory");
        match remove_write_probe(&dir) {
            Err(PathsError::ProbeCleanupFailed { path, .. }) => {
                assert_eq!(path, dir);
            }
            other => panic!("expected ProbeCleanupFailed, got {other:?}"),
        }
        cleanup(&base);
    }
}
