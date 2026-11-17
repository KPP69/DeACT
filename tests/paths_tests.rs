//! Portable directory tests: resolution, creation, failure handling.

#[path = "common.rs"]
mod common;

use std::fs;

use common::{cleanup, unique_temp_dir};
use deact::paths::{data_dir_for_exe, resolve_for_exe};

#[test]
fn data_dir_is_beside_the_executable() {
    let base = unique_temp_dir("paths-resolve");
    let exe = base.join("bin").join("DeACT.exe");
    let dir = data_dir_for_exe(&exe).expect("data dir resolves");
    assert_eq!(dir, base.join("bin").join("DeACT_Data"));
    cleanup(&base);
}

#[test]
fn data_directories_are_created() {
    let base = unique_temp_dir("paths-create");
    let exe = base.join("bin").join("DeACT.exe");
    let paths = resolve_for_exe(&exe).expect("data directories resolve and are created");
    assert!(paths.data_dir().is_dir(), "DeACT_Data exists");
    assert!(paths.logs_dir().is_dir(), "logs/ exists");
    assert!(paths.cache_dir().is_dir(), "cache/ exists");
    assert!(paths.backups_dir().is_dir(), "backups/ exists");
    assert_eq!(paths.config_path(), paths.data_dir().join("config.toml"));
    assert_eq!(paths.database_path(), paths.data_dir().join("deact.db"));
    cleanup(&base);
}

#[test]
fn unwritable_location_fails_without_fallback() {
    // The data directory cannot be created when a component of its path is
    // a regular file. This fails deterministically even when running as a
    // privileged user (where permission bits alone would not block writes).
    let base = unique_temp_dir("paths-unwritable");
    let blocker = base.join("blocker");
    fs::create_dir_all(&base).expect("base dir");
    fs::write(&blocker, b"not a directory").expect("blocker file");
    let exe = blocker.join("DeACT.exe");
    let result = resolve_for_exe(&exe);
    assert!(
        result.is_err(),
        "resolution must fail when the data directory cannot be created, got {result:?}"
    );
    // No fallback directory was created beside the blocker.
    assert!(!blocker.join("DeACT_Data").exists());
    cleanup(&base);
}
