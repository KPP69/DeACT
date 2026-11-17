//! Configuration tests: default generation, loading, corrupt files.

#[path = "common.rs"]
mod common;

use std::fs;

use common::{cleanup, unique_temp_dir};
use deact::config::{load, load_or_create, save, AppConfig};

#[test]
fn default_configuration_is_generated_on_first_launch() {
    let base = unique_temp_dir("config-default");
    fs::create_dir_all(&base).expect("base dir");
    let path = base.join("config.toml");
    let (config, created) = load_or_create(&path).expect("default config is created");
    assert!(created, "first launch creates the configuration");
    assert_eq!(config, AppConfig::default());
    assert!(path.is_file(), "config file exists after creation");
    let text = fs::read_to_string(&path).expect("config file is readable");
    assert!(
        text.contains("version = 1"),
        "config contains the schema version"
    );
    cleanup(&base);
}

#[test]
fn existing_configuration_is_loaded() {
    let base = unique_temp_dir("config-load");
    fs::create_dir_all(&base).expect("base dir");
    let path = base.join("config.toml");
    let expected = AppConfig {
        window_width: 1280,
        window_height: 720,
        log_level: "debug".to_string(),
        ..AppConfig::default()
    };
    save(&path, &expected).expect("config is saved");
    let (loaded, created) = load_or_create(&path).expect("existing config loads");
    assert!(!created, "existing config is not recreated");
    assert_eq!(loaded, expected);
    cleanup(&base);
}

#[test]
fn corrupt_configuration_is_reported_and_not_overwritten() {
    let base = unique_temp_dir("config-corrupt");
    fs::create_dir_all(&base).expect("base dir");
    let path = base.join("config.toml");
    let corrupt = b"this is = = not valid toml [[[";
    fs::write(&path, corrupt).expect("corrupt config is written");
    let result = load(&path);
    assert!(result.is_err(), "corrupt config must fail to load");
    let after = fs::read(&path).expect("config file still exists");
    assert_eq!(after, corrupt, "corrupt file must not be overwritten");
    cleanup(&base);
}
