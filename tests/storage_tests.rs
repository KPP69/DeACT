//! SQLite tests: first initialisation, repeat initialisation, WAL mode,
//! typed meta writes through the worker, and requests after shutdown.
//! Error-kind distinctions for schema_version are covered by unit tests
//! inside the storage module, where a second connection may corrupt the
//! database; no test (and no production code) runs caller-supplied SQL.

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
fn database_worker_meta_writes_and_reads_back() {
    let base = unique_temp_dir("storage-worker");
    fs::create_dir_all(&base).expect("base dir");
    let db_path = base.join("deact.db");
    let worker = DbWorker::open(&db_path).expect("database opens");
    assert_eq!(
        worker.get_meta("probe").expect("meta read"),
        None,
        "a missing key is None, not an error"
    );
    worker
        .set_meta("probe", "hello")
        .expect("worker writes a meta value");
    assert_eq!(
        worker.get_meta("probe").expect("meta read back"),
        Some("hello".to_string()),
        "written value reads back through the worker"
    );
    worker
        .set_meta("probe", "world")
        .expect("worker overwrites a meta value");
    assert_eq!(
        worker.get_meta("probe").expect("meta read after overwrite"),
        Some("world".to_string())
    );
    // The worker must still work after meta writes, and the schema
    // version must be untouched by them.
    assert_eq!(worker.schema_version().expect("version after writes"), 1);
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
