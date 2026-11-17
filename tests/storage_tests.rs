//! SQLite tests: first initialisation, repeat initialisation, WAL mode,
//! worker writes and error handling.

#[path = "common.rs"]
mod common;

use std::fs;

use common::{cleanup, unique_temp_dir};
use deact::storage::DbWorker;

#[test]
fn sqlite_first_initialisation_creates_schema_version_and_wal() {
    let base = unique_temp_dir("storage-first");
    fs::create_dir_all(&base).expect("base dir");
    let db_path = base.join("deact.db");
    let worker = DbWorker::open(&db_path).expect("database opens");
    assert_eq!(worker.schema_version().expect("schema version reads"), 1);
    assert_eq!(
        worker.journal_mode().expect("journal mode reads"),
        "wal",
        "database must use WAL mode"
    );
    assert!(db_path.is_file(), "database file exists");
    drop(worker);
    cleanup(&base);
}

#[test]
fn sqlite_repeat_initialisation_keeps_version() {
    let base = unique_temp_dir("storage-repeat");
    fs::create_dir_all(&base).expect("base dir");
    let db_path = base.join("deact.db");
    {
        let worker = DbWorker::open(&db_path).expect("first open");
        assert_eq!(worker.schema_version().expect("version"), 1);
    }
    {
        let worker = DbWorker::open(&db_path).expect("second open");
        assert_eq!(
            worker.schema_version().expect("version after reopen"),
            1,
            "repeat initialisation must not reset the schema version"
        );
        assert_eq!(worker.journal_mode().expect("journal mode"), "wal");
    }
    cleanup(&base);
}

#[test]
fn database_worker_writes_and_reports_errors() {
    let base = unique_temp_dir("storage-worker");
    fs::create_dir_all(&base).expect("base dir");
    let db_path = base.join("deact.db");
    let worker = DbWorker::open(&db_path).expect("database opens");
    worker
        .execute("CREATE TABLE probe (id INTEGER PRIMARY KEY, label TEXT NOT NULL)")
        .expect("worker executes DDL");
    worker
        .execute("INSERT INTO probe (id, label) VALUES (1, 'hello')")
        .expect("worker executes INSERT");
    let value = worker
        .query_one_text("SELECT label FROM probe WHERE id = 1")
        .expect("worker executes SELECT");
    assert_eq!(value.as_deref(), Some("hello"), "written row reads back");
    let bad = worker.execute("INSERT INTO no_such_table VALUES (1)");
    assert!(bad.is_err(), "invalid SQL must be reported, not swallowed");
    // The worker must still work after an error.
    assert_eq!(worker.schema_version().expect("version after error"), 1);
    drop(worker);
    cleanup(&base);
}

#[test]
fn database_worker_is_unavailable_after_shutdown() {
    let base = unique_temp_dir("storage-shutdown");
    fs::create_dir_all(&base).expect("base dir");
    let db_path = base.join("deact.db");
    let mut worker = DbWorker::open(&db_path).expect("database opens");
    worker.shutdown();
    assert!(
        worker.schema_version().is_err(),
        "requests after shutdown must fail instead of hanging"
    );
    cleanup(&base);
}
