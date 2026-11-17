//! SQLite storage with a single controlled writer.
//!
//! All database writes go through one dedicated worker thread
//! ([`DbWorker`]) fed by a bounded channel. No other module opens a write
//! connection, and the UI thread never touches the database directly. The
//! database lives at `DeACT_Data/deact.db`, uses WAL mode, and records its
//! schema version in a `meta` table so future migrations have a base to
//! build on. This stage only establishes the database foundation; it does
//! not design trading tables.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use rusqlite::Connection;

/// Maximum number of database requests waiting for the worker.
const CHANNEL_CAPACITY: usize = 64;

/// Errors from the storage layer.
#[derive(Debug)]
pub enum StorageError {
    /// An SQLite operation failed.
    Sqlite(rusqlite::Error),
    /// The worker thread is gone or its channel is closed.
    WorkerUnavailable,
    /// A reply from the worker could not be delivered.
    ReplyFailed,
    /// The `meta` table has no `schema_version` entry.
    MissingSchemaVersion,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Sqlite(source) => write!(f, "sqlite error: {source}"),
            StorageError::WorkerUnavailable => {
                write!(f, "database worker is unavailable")
            }
            StorageError::ReplyFailed => {
                write!(f, "database worker reply was lost")
            }
            StorageError::MissingSchemaVersion => {
                write!(f, "database has no schema_version entry")
            }
        }
    }
}

impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StorageError::Sqlite(source) => Some(source),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(source: rusqlite::Error) -> Self {
        StorageError::Sqlite(source)
    }
}

/// One request to the database worker.
enum DbRequest {
    SchemaVersion {
        reply: Sender<Result<i64, StorageError>>,
    },
    JournalMode {
        reply: Sender<Result<String, StorageError>>,
    },
    /// Run one SQL statement and return the first column of the first row,
    /// if any. Used by tests and by future read paths.
    QueryOneText {
        sql: String,
        reply: Sender<Result<Option<String>, StorageError>>,
    },
    /// Run SQL that returns no rows (DDL / INSERT / UPDATE / DELETE).
    Execute {
        sql: String,
        reply: Sender<Result<(), StorageError>>,
    },
    Shutdown,
}

/// The single database writer. Owns the worker thread; dropping it (or
/// calling [`DbWorker::shutdown`]) stops the worker cleanly.
pub struct DbWorker {
    tx: Option<SyncSender<DbRequest>>,
    handle: Option<JoinHandle<()>>,
}

impl DbWorker {
    /// Open (or create) the database at `db_path`, enable WAL, set a busy
    /// timeout, and create the `meta` table with `schema_version = 1`.
    /// Returns an error promptly when initialisation fails.
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let (tx, rx) = sync_channel::<DbRequest>(CHANNEL_CAPACITY);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), StorageError>>();
        let path = db_path.to_path_buf();
        let handle = thread::Builder::new()
            .name("deact-db-worker".to_string())
            .spawn(move || worker_main(path, rx, ready_tx))
            .map_err(|_| StorageError::WorkerUnavailable)?;
        let mut worker = Self {
            tx: Some(tx),
            handle: Some(handle),
        };
        // Block until the worker reports initialisation success or failure.
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(worker),
            Ok(Err(err)) => {
                worker.shutdown();
                Err(err)
            }
            Err(_) => {
                worker.shutdown();
                Err(StorageError::WorkerUnavailable)
            }
        }
    }

    /// Current schema version recorded in the `meta` table.
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::SchemaVersion { reply: reply_tx })?;
        reply_rx.recv().map_err(|_| StorageError::ReplyFailed)?
    }

    /// Current journal mode (expected: `wal`).
    pub fn journal_mode(&self) -> Result<String, StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::JournalMode { reply: reply_tx })?;
        reply_rx.recv().map_err(|_| StorageError::ReplyFailed)?
    }

    /// Run one SQL statement and return the first column of the first row.
    pub fn query_one_text(&self, sql: &str) -> Result<Option<String>, StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::QueryOneText {
            sql: sql.to_string(),
            reply: reply_tx,
        })?;
        reply_rx.recv().map_err(|_| StorageError::ReplyFailed)?
    }

    /// Run SQL that returns no rows.
    pub fn execute(&self, sql: &str) -> Result<(), StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::Execute {
            sql: sql.to_string(),
            reply: reply_tx,
        })?;
        reply_rx.recv().map_err(|_| StorageError::ReplyFailed)?
    }

    /// Stop the worker thread and wait for it to finish. Safe to call
    /// more than once; also runs automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(DbRequest::Shutdown);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    fn send(&self, request: DbRequest) -> Result<(), StorageError> {
        match &self.tx {
            Some(tx) => tx
                .send(request)
                .map_err(|_| StorageError::WorkerUnavailable),
            None => Err(StorageError::WorkerUnavailable),
        }
    }
}

impl Drop for DbWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Worker thread entry point: initialise the database, report the result,
/// then serve requests until shutdown or channel close.
fn worker_main(
    db_path: PathBuf,
    rx: Receiver<DbRequest>,
    ready_tx: Sender<Result<(), StorageError>>,
) {
    let connection = initialise(&db_path);
    match connection {
        Ok(connection) => {
            let _ = ready_tx.send(Ok(()));
            serve(connection, rx);
        }
        Err(err) => {
            let _ = ready_tx.send(Err(err));
        }
    }
}

/// Open the connection, enable WAL, set a busy timeout and create the
/// `meta` table with the initial schema version.
fn initialise(db_path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open(db_path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "busy_timeout", 5000)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (\
            key TEXT PRIMARY KEY, \
            value TEXT NOT NULL\
        );\
        INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1');",
    )?;
    Ok(connection)
}

fn serve(connection: Connection, rx: Receiver<DbRequest>) {
    while let Ok(request) = rx.recv() {
        match request {
            DbRequest::SchemaVersion { reply } => {
                let result = schema_version(&connection);
                let _ = reply.send(result);
            }
            DbRequest::JournalMode { reply } => {
                let result = connection
                    .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                    .map_err(StorageError::from);
                let _ = reply.send(result);
            }
            DbRequest::QueryOneText { sql, reply } => {
                let result = connection
                    .query_row(&sql, [], |row| row.get::<_, Option<String>>(0))
                    .map_err(StorageError::from);
                let _ = reply.send(result);
            }
            DbRequest::Execute { sql, reply } => {
                let result = connection.execute_batch(&sql).map_err(StorageError::from);
                let _ = reply.send(result);
            }
            DbRequest::Shutdown => break,
        }
    }
}

fn schema_version(connection: &Connection) -> Result<i64, StorageError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    match value {
        Some(text) => text
            .parse::<i64>()
            .map_err(|_| StorageError::MissingSchemaVersion),
        None => Err(StorageError::MissingSchemaVersion),
    }
}
