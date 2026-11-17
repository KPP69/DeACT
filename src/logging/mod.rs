//! `tracing` logging to the console and to `DeACT_Data/logs/deact.log`.
//!
//! Logging is initialised once at startup. Log records never contain API
//! keys, sessions, signatures or complete authentication requests —
//! callers must not log such values, and this module performs no redaction
//! of message contents beyond that rule. File I/O happens on the logging
//! subscriber's own path, never on the UI thread.

use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

/// Errors that can occur while initialising logging.
#[derive(Debug)]
pub enum LoggingError {
    /// The log directory or file could not be created/opened.
    Io(std::io::Error),
    /// Another global subscriber is already installed.
    AlreadyInitialised(String),
}

impl fmt::Display for LoggingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoggingError::Io(source) => write!(f, "cannot initialise logging: {source}"),
            LoggingError::AlreadyInitialised(source) => {
                write!(f, "logging is already initialised: {source}")
            }
        }
    }
}

impl Error for LoggingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoggingError::Io(source) => Some(source),
            LoggingError::AlreadyInitialised(_) => None,
        }
    }
}

/// Keeps the log file path for diagnostics. The log file itself is owned
/// by the global `tracing` subscriber (via `FileMakeWriter`), so it stays
/// alive for the lifetime of the process. Drop the guard only at shutdown.
#[derive(Debug)]
pub struct LogGuard {
    path: PathBuf,
}

impl LogGuard {
    /// Path of the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A `MakeWriter` that hands out a locked handle to the shared log file.
#[derive(Debug, Clone)]
struct FileMakeWriter {
    file: Arc<Mutex<File>>,
}

impl<'a> MakeWriter<'a> for FileMakeWriter {
    type Writer = LockedFileWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let guard = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        LockedFileWriter { guard }
    }
}

/// `io::Write` over a locked `File`.
struct LockedFileWriter<'a> {
    guard: MutexGuard<'a, File>,
}

impl io::Write for LockedFileWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.guard.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.guard.flush()
    }
}

/// Initialise the global `tracing` subscriber: human-readable output to
/// the console (with ANSI colours) and to `deact.log` in `logs_dir`
/// (without ANSI colours, appended). Fails loudly when the log file cannot
/// be opened; callers must treat that as a startup failure.
pub fn init(logs_dir: &Path) -> Result<LogGuard, LoggingError> {
    std::fs::create_dir_all(logs_dir).map_err(LoggingError::Io)?;
    let path = logs_dir.join("deact.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(LoggingError::Io)?;
    let file = Arc::new(Mutex::new(file));

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(io::stdout)
                .with_ansi(true),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(FileMakeWriter { file })
                .with_ansi(false),
        );
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|source| LoggingError::AlreadyInitialised(source.to_string()))?;

    Ok(LogGuard { path })
}
