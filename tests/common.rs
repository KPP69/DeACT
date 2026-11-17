//! Shared helpers for integration tests: unique temporary directories.
//! Tests never touch the real `DeACT_Data/` directory.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh, unique temporary directory path for one test. The directory is
/// created by the caller (or by the code under test) as needed.
pub fn unique_temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("deact-test-{tag}-{}-{n}", std::process::id()))
}

/// Best-effort cleanup of a temporary directory tree.
pub fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}
