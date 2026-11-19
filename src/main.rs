//! DeACT entry point: ordered startup, then the native window.
//!
//! Startup order:
//! 1. Resolve the executable path and initialise `DeACT_Data/`.
//! 2. Initialise logging.
//! 3. Load or create the configuration.
//! 4. Initialise the SQLite database worker.
//! 5. Start the background Tokio runtime.
//! 6. Open the desktop window.
//! 7. On window close, shut the runtime and the database worker down.
//!
//! Any failure in steps 1–5 is reported and exits the process; the program
//! never continues with an incomplete startup state.

// The Windows release build is a GUI-subsystem application: double-clicking
// DeACT.exe opens only the native window, with no console window attached.
// Debug builds keep the console subsystem so startup output stays visible
// during development. The file logger under `DeACT_Data/logs/` is unaffected
// either way; the attribute does not apply to non-Windows targets.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use deact::app::{self, AppStatus};
use deact::config;
use deact::logging;
use deact::paths;
use deact::runtime::BackgroundRuntime;
use deact::storage::DbWorker;

fn main() {
    if let Err(message) = real_main() {
        eprintln!("DeACT startup failed: {message}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    // 1. Portable data directory, resolved from the executable path.
    let portable = paths::resolve().map_err(|err| err.to_string())?;

    // 2. Logging (console + DeACT_Data/logs/deact.log).
    let _log_guard = logging::init(portable.logs_dir()).map_err(|err| err.to_string())?;
    tracing::info!("DeACT starting, version {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("data directory: {}", portable.data_dir().display());

    // 3. Configuration (DeACT_Data/config.toml).
    let (configuration, created) =
        config::load_or_create(portable.config_path()).map_err(|err| err.to_string())?;
    if created {
        tracing::info!("created default configuration");
    }
    tracing::info!(
        "configuration loaded (version {}, {}x{})",
        configuration.version,
        configuration.window_width,
        configuration.window_height
    );

    // 4. SQLite database worker (DeACT_Data/deact.db, WAL mode).
    let mut db_worker = DbWorker::open(portable.database_path()).map_err(|err| err.to_string())?;
    let schema_version = db_worker.schema_version().map_err(|err| err.to_string())?;
    tracing::info!("database ready, schema version {schema_version}");

    // 5. Background Tokio runtime (not on the UI thread).
    let mut runtime = BackgroundRuntime::start().map_err(|err| err.to_string())?;
    runtime.ping().map_err(|err| err.to_string())?;
    tracing::info!("background runtime ready");

    // 6. Desktop window. This blocks until the user closes the window.
    let status = AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: portable.data_dir().to_path_buf(),
        data_dir_ready: true,
        database_ready: true,
        schema_version: Some(schema_version),
        runtime_ready: true,
    };
    if let Err(err) = app::run(status) {
        runtime.shutdown();
        db_worker.shutdown();
        return Err(format!("desktop window failed: {err}"));
    }

    // 7. Orderly shutdown.
    tracing::info!("DeACT shutting down");
    runtime.shutdown();
    db_worker.shutdown();
    Ok(())
}
