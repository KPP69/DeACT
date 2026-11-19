//! SQLite storage with a single controlled writer.
//!
//! All database writes go through one dedicated worker thread
//! ([`DbWorker`]) fed by a bounded channel. No other module opens a write
//! connection, and the UI thread never touches the database directly. The
//! database lives at `data/deact.db`, uses WAL mode, and records its
//! schema version in a `meta` table so future migrations have a base to
//! build on. This stage only establishes the database foundation; it does
//! not design trading tables.
//!
//! The worker exposes only typed operations ([`DbWorker::schema_version`],
//! [`DbWorker::journal_mode`], [`DbWorker::get_meta`],
//! [`DbWorker::set_meta`]). There is deliberately no public API that
//! accepts an arbitrary SQL string: callers cannot smuggle SQL past the
//! storage layer, and every statement the worker runs is fixed text with
//! parameter binding.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use rusqlite::{params, Connection, OptionalExtension};

/// Maximum number of database requests waiting for the worker.
const CHANNEL_CAPACITY: usize = 64;

/// Errors from the storage layer.
#[derive(Debug)]
pub enum StorageError {
    /// An SQLite operation failed. The underlying error is preserved.
    Sqlite(rusqlite::Error),
    /// The worker thread is gone or its channel is closed.
    WorkerUnavailable,
    /// A reply from the worker could not be delivered.
    ReplyFailed,
    /// The `meta` table has no `schema_version` entry.
    MissingSchemaVersion,
    /// The `schema_version` entry exists but is not a valid integer.
    InvalidSchemaVersion(String),
    /// The database already contains tables but is missing the required
    /// `meta` table, so it cannot be initialised as a new database.
    IncompleteDatabase,
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
            StorageError::InvalidSchemaVersion(value) => {
                write!(f, "schema_version entry is not a valid integer: {value:?}")
            }
            StorageError::IncompleteDatabase => {
                write!(
                    f,
                    "database is incomplete: it contains tables but is \
                     missing the required meta table; refusing to \
                     initialise it as a new database"
                )
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

/// One request to the database worker. SQL strings never cross this
/// boundary: every variant carries typed values, and the worker runs only
/// fixed statements with parameter binding.
enum DbRequest {
    SchemaVersion {
        reply: Sender<Result<i64, StorageError>>,
    },
    JournalMode {
        reply: Sender<Result<String, StorageError>>,
    },
    /// Read one `meta` value by key (`None` when the key is absent).
    GetMeta {
        key: String,
        reply: Sender<Result<Option<String>, StorageError>>,
    },
    /// Insert or replace one `meta` value.
    SetMeta {
        key: String,
        value: String,
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
    /// timeout, and create the `meta` table with `schema_version = 1`
    /// when the database is new. An existing database is never re-seeded:
    /// its recorded version is kept, and a database that already holds
    /// tables but has no `meta` table is rejected instead of being
    /// rebuilt. Returns an error promptly when initialisation fails.
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

    /// Read a `meta` value by key. Returns `Ok(None)` when the key is
    /// absent; a missing key is not an error.
    pub fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::GetMeta {
            key: key.to_string(),
            reply: reply_tx,
        })?;
        reply_rx.recv().map_err(|_| StorageError::ReplyFailed)?
    }

    /// Insert or replace a `meta` value. This is the only public write
    /// operation besides initialisation; all writes go through the worker.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.send(DbRequest::SetMeta {
            key: key.to_string(),
            value: value.to_string(),
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

/// Open the connection, enable WAL, and set a busy timeout. Then:
/// - a fresh database (no user tables at all) gets its schema created
///   and `schema_version = 1` seeded, in one transaction, so a crash
///   cannot leave a half-initialised database behind;
/// - an existing database that already has a `meta` table is left
///   untouched — its recorded version is never re-inserted, so a
///   deleted `schema_version` row stays deleted and is reported by
///   `schema_version()` instead of being silently restored;
/// - an existing database that holds tables but has no `meta` table is
///   rejected as incomplete instead of being rebuilt as if it were new.
fn initialise(db_path: &Path) -> Result<Connection, StorageError> {
    let mut connection = Connection::open(db_path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "busy_timeout", 5000)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    if is_fresh_database(&connection)? {
        let tx = connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (\
                key TEXT PRIMARY KEY, \
                value TEXT NOT NULL\
            );\
            INSERT INTO meta (key, value) VALUES ('schema_version', '1');",
        )?;
        tx.commit()?;
    } else if !meta_table_exists(&connection)? {
        return Err(StorageError::IncompleteDatabase);
    }
    Ok(connection)
}

/// A database is fresh only when it holds no user tables at all.
/// Checking the `meta` table alone would misjudge a database that has
/// other tables but lost its `meta` table.
fn is_fresh_database(connection: &Connection) -> Result<bool, StorageError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 0)
}

fn meta_table_exists(connection: &Connection) -> Result<bool, StorageError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name = 'meta'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
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
            DbRequest::GetMeta { key, reply } => {
                let result = connection
                    .query_row(
                        "SELECT value FROM meta WHERE key = ?1",
                        params![key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(StorageError::from);
                let _ = reply.send(result);
            }
            DbRequest::SetMeta { key, value, reply } => {
                let result = connection
                    .execute(
                        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        params![key, value],
                    )
                    .map(|_| ())
                    .map_err(StorageError::from);
                let _ = reply.send(result);
            }
            DbRequest::Shutdown => break,
        }
    }
}

/// Read the schema version, distinguishing the three failure modes
/// instead of collapsing them into one error:
/// - no `schema_version` row: [`StorageError::MissingSchemaVersion`];
/// - the query itself fails (for example the `meta` table is gone):
///   [`StorageError::Sqlite`], with the underlying error preserved;
/// - the stored value is not an integer:
///   [`StorageError::InvalidSchemaVersion`], carrying the stored text.
fn schema_version(connection: &Connection) -> Result<i64, StorageError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    match value {
        Some(text) => text
            .parse::<i64>()
            .map_err(|_| StorageError::InvalidSchemaVersion(text)),
        None => Err(StorageError::MissingSchemaVersion),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "deact-storage-unit-{tag}-{}-{n}",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    /// A second, direct connection to the same database file, used only
    /// inside these tests to corrupt the database and prove the worker
    /// reports each failure kind correctly. Production code never opens
    /// a second connection and never runs caller-supplied SQL.
    fn raw_connection(db_path: &Path) -> Connection {
        let connection = Connection::open(db_path).expect("raw connection opens");
        connection
            .pragma_update(None, "busy_timeout", 5000)
            .expect("busy timeout set");
        connection
    }

    #[test]
    fn worker_meta_round_trip() {
        let base = unique_temp_dir("meta");
        std::fs::create_dir_all(&base).expect("base dir");
        let db_path = base.join("deact.db");
        let worker = DbWorker::open(&db_path).expect("database opens");
        assert_eq!(
            worker.get_meta("probe").expect("meta read"),
            None,
            "a missing key is None, not an error"
        );
        worker.set_meta("probe", "hello").expect("meta write");
        assert_eq!(
            worker.get_meta("probe").expect("meta read back"),
            Some("hello".to_string())
        );
        worker.set_meta("probe", "world").expect("meta overwrite");
        assert_eq!(
            worker.get_meta("probe").expect("meta read after overwrite"),
            Some("world".to_string())
        );
        assert_eq!(
            worker
                .schema_version()
                .expect("version untouched by meta writes"),
            1
        );
        drop(worker);
        cleanup(&base);
    }

    #[test]
    fn worker_schema_version_errors_are_distinguished() {
        let base = unique_temp_dir("version-errors");
        std::fs::create_dir_all(&base).expect("base dir");
        let db_path = base.join("deact.db");
        let worker = DbWorker::open(&db_path).expect("database opens");

        // 1. The row is gone: MissingSchemaVersion.
        raw_connection(&db_path)
            .execute("DELETE FROM meta WHERE key = 'schema_version'", [])
            .expect("delete schema_version row");
        assert!(
            matches!(
                worker.schema_version(),
                Err(StorageError::MissingSchemaVersion)
            ),
            "a missing schema_version row must be MissingSchemaVersion"
        );

        // 2. The row exists but is not an integer: InvalidSchemaVersion.
        raw_connection(&db_path)
            .execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', 'not-a-number')",
                [],
            )
            .expect("insert malformed schema_version row");
        match worker.schema_version() {
            Err(StorageError::InvalidSchemaVersion(value)) => {
                assert_eq!(value, "not-a-number");
            }
            other => panic!("expected InvalidSchemaVersion, got {other:?}"),
        }

        // 3. The query itself fails: the underlying SQLite error, not a
        // generic "missing version" error.
        raw_connection(&db_path)
            .execute("DROP TABLE meta", [])
            .expect("drop meta table");
        assert!(
            matches!(worker.schema_version(), Err(StorageError::Sqlite(_))),
            "a failed schema_version query must surface the SQLite error"
        );

        drop(worker);
        cleanup(&base);
    }

    #[test]
    fn reopening_after_schema_version_deleted_reports_missing() {
        let base = unique_temp_dir("version-deleted");
        std::fs::create_dir_all(&base).expect("base dir");
        let db_path = base.join("deact.db");
        {
            let worker = DbWorker::open(&db_path).expect("database opens");
            assert_eq!(worker.schema_version().expect("version"), 1);
        }
        // Simulate a damaged database: delete the version row with a
        // test-only connection, then close that connection.
        {
            let connection = raw_connection(&db_path);
            connection
                .execute("DELETE FROM meta WHERE key = 'schema_version'", [])
                .expect("delete schema_version row");
        }
        // Reopening must succeed, and must not restore the deleted row.
        let worker = DbWorker::open(&db_path).expect("reopen succeeds");
        assert_eq!(
            worker.get_meta("schema_version").expect("meta read"),
            None,
            "the deleted schema_version row must not be restored on reopen"
        );
        assert!(
            matches!(
                worker.schema_version(),
                Err(StorageError::MissingSchemaVersion)
            ),
            "a deleted schema_version row must be MissingSchemaVersion"
        );
        drop(worker);
        cleanup(&base);
    }

    #[test]
    fn existing_tables_without_meta_are_not_treated_as_new() {
        let base = unique_temp_dir("tables-no-meta");
        std::fs::create_dir_all(&base).expect("base dir");
        let db_path = base.join("deact.db");
        {
            let connection = raw_connection(&db_path);
            connection
                .execute(
                    "CREATE TABLE user_data (id INTEGER PRIMARY KEY, label TEXT NOT NULL)",
                    [],
                )
                .expect("create user table");
            connection
                .execute(
                    "INSERT INTO user_data (id, label) VALUES (1, 'keep me')",
                    [],
                )
                .expect("insert user row");
        }
        assert!(
            matches!(
                DbWorker::open(&db_path),
                Err(StorageError::IncompleteDatabase)
            ),
            "a database with tables but no meta table must be rejected, \
             not initialised as new"
        );
        // The database must not have been rebuilt: still no `meta`
        // table, and the user's table and row are untouched.
        let connection = raw_connection(&db_path);
        let meta_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'meta'",
                [],
                |row| row.get(0),
            )
            .expect("meta table check");
        assert_eq!(
            meta_count, 0,
            "initialisation must not recreate the meta table"
        );
        let label: String = connection
            .query_row("SELECT label FROM user_data WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("user row still readable");
        assert_eq!(label, "keep me", "user data must be untouched");
        drop(connection);
        cleanup(&base);
    }
}
