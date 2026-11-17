//! DeACT library crate.
//!
//! The binary (`src/main.rs`) is a thin startup shell: it resolves the
//! portable data directory, initialises logging, configuration, the SQLite
//! worker and the background Tokio runtime, then hands control to the UI.
//! All logic lives in these modules so the integration tests in `tests/`
//! can exercise them without launching a window.

pub mod app;
pub mod config;
pub mod logging;
pub mod paths;
pub mod runtime;
pub mod storage;
