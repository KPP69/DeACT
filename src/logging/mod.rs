//! `tracing` logging to the console and to `DeACT_Data/logs/deact.log`.
//!
//! Logging is initialised once at startup. Log records never contain API
//! keys, sessions, signatures or complete authentication requests —
//! callers must not log such values, and this module performs no redaction
//! of message contents beyond that rule.
//!
//! File logging is non-blocking: log records are sent over a channel to a
//! dedicated writer thread owned by `tracing-appender`, so a slow disk
//! never blocks the thread that emitted the log record (including the UI
//! thread). The [`LogGuard`] holds the [`WorkerGuard`]; when the guard is
//! dropped at shutdown, the writer thread is joined and any buffered
//! records are flushed before the process exits.

use std::error::Error;
use std::fmt;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
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

/// Holds the log file path and the `tracing-appender` worker guard. The
/// guard must stay alive for as long as logging is used: dropping it
/// flushes the remaining buffered records and joins the background writer
/// thread. `main` keeps this guard until the process exits, so no log
/// record is lost at shutdown.
#[derive(Debug)]
pub struct LogGuard {
    path: PathBuf,
    _worker_guard: WorkerGuard,
}

impl LogGuard {
    /// Path of the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Initialise the global `tracing` subscriber: human-readable output to
/// the console (with ANSI colours) and to `deact.log` in `logs_dir`
/// (without ANSI colours, appended) through a non-blocking writer. Fails
/// loudly when the log file cannot be opened; callers must treat that as
/// a startup failure.
pub fn init(logs_dir: &Path) -> Result<LogGuard, LoggingError> {
    std::fs::create_dir_all(logs_dir).map_err(LoggingError::Io)?;
    let path = logs_dir.join("deact.log");
    // Open the file eagerly so a permission or disk problem is reported
    // at startup instead of surfacing only on the first log write. The
    // appender below opens the same file in append mode.
    drop(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(LoggingError::Io)?,
    );
    let file_appender = rolling::never(logs_dir, "deact.log");
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(io::stdout)
                .with_ansi(true),
        )
        .with(
            tracing_subscriber::fmt::Layer::default()
                .with_writer(non_blocking)
                .with_ansi(false),
        );
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|source| LoggingError::AlreadyInitialised(source.to_string()))?;

    Ok(LogGuard {
        path,
        _worker_guard: worker_guard,
    })
}
