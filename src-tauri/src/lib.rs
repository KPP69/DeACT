//! DeACT library crate: startup sequence and the Tauri application.
//!
//! Startup order (ARCHITECTURE.md sections 3.1, 3.3 and 3.6):
//! 1. Resolve the portable `data/` directory from the executable path and
//!    probe it for writability (pre-logging: a failure is shown in a
//!    native Windows error dialog and the process exits).
//! 2. Initialise logging into `data/logs/` (a failure is likewise shown
//!    in the dialog and exits; logging does not exist yet at that point).
//! 3. Load or create the configuration (`data/config/deact.toml`) and
//!    initialise the SQLite worker (`data/deact.db`) — failures from here
//!    on are recorded with `tracing::error!` and then shown in the dialog.
//! 4. Build and run the Tauri application: the portable directories are
//!    installed through `Context::config_mut()` as per-directory
//!    `appDirectoriesOverride` values, and the main window is created in
//!    Rust with `WebviewWindowBuilder::data_directory` pointing at
//!    `data/webview/`.
//! 5. On window close, shut the database worker down; the log guard drops
//!    last and flushes the log writer.
//!
//! The process never continues with an incomplete startup state, and no
//! stage falls back to a system directory.

pub mod config;
pub mod error_report;
#[cfg(feature = "fault-injection")]
pub mod fault;
pub mod logging;
pub mod paths;
pub mod storage;
pub mod supervisor;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::utils::config::{AppDirectoriesOverride, AppDirectoryOverrides};
use tauri::Manager;

use crate::error_report::StartupStage;
use crate::storage::DbWorker;
use crate::supervisor::Supervisor;

/// Backend state shared with the Tauri commands. Immutable after startup
/// except for the supervisor health flag, which the setup hook sets once
/// the health check has passed.
pub struct AppState {
    app_version: String,
    data_dir: PathBuf,
    config_path: PathBuf,
    database_path: PathBuf,
    webview_dir: PathBuf,
    schema_version: i64,
    config_version: u32,
    window_width: u32,
    window_height: u32,
    log_level: String,
    runtime_ready: AtomicBool,
}

/// Status snapshot returned to the frontend by the `app_status` command.
#[derive(Debug, Serialize)]
pub struct AppStatus {
    app_version: String,
    data_dir: String,
    config_path: String,
    database_path: String,
    webview_dir: String,
    database_ready: bool,
    schema_version: i64,
    runtime_ready: bool,
    config_version: u32,
    window_width: u32,
    window_height: u32,
    log_level: String,
}

/// The one Phase 1 command: a read-only status snapshot for the overview
/// page. Commands only validate and forward; no business logic lives in
/// the command layer.
#[tauri::command]
fn app_status(state: tauri::State<'_, AppState>) -> AppStatus {
    AppStatus {
        app_version: state.app_version.clone(),
        data_dir: state.data_dir.display().to_string(),
        config_path: state.config_path.display().to_string(),
        database_path: state.database_path.display().to_string(),
        webview_dir: state.webview_dir.display().to_string(),
        database_ready: true,
        schema_version: state.schema_version,
        runtime_ready: state.runtime_ready.load(Ordering::SeqCst),
        config_version: state.config_version,
        window_width: state.window_width,
        window_height: state.window_height,
        log_level: state.log_level.clone(),
    }
}

/// Report a post-logging startup failure and exit, as section 3.6 of the
/// architecture requires.
fn fail_after_logging(stage: StartupStage, detail: &str) -> ! {
    error_report::report(stage, detail, true);
    std::process::exit(1);
}

/// Run the application. Called by the binary's `main`; returns only when
/// the user closes the window and shutdown has completed. Every startup
/// failure is reported (dialog, plus a log record once logging exists)
/// and exits the process.
pub fn run() {
    // 1. Portable data directory, resolved from the executable path.
    #[cfg(feature = "fault-injection")]
    if fault::injected_stage() == Some(StartupStage::DataDirectory) {
        error_report::report(StartupStage::DataDirectory, fault::INJECTED_DETAIL, false);
        std::process::exit(1);
    }
    let paths = match paths::resolve() {
        Ok(paths) => paths,
        Err(err) => {
            error_report::report(StartupStage::DataDirectory, &err.to_string(), false);
            std::process::exit(1);
        }
    };

    // 2. Logging (console + data/logs/deact.log, non-blocking file writer).
    #[cfg(feature = "fault-injection")]
    if fault::injected_stage() == Some(StartupStage::Logging) {
        error_report::report(StartupStage::Logging, fault::INJECTED_DETAIL, false);
        std::process::exit(1);
    }
    let _log_guard = match logging::init(paths.logs_dir()) {
        Ok(guard) => guard,
        Err(err) => {
            error_report::report(StartupStage::Logging, &err.to_string(), false);
            std::process::exit(1);
        }
    };
    tracing::info!("DeACT starting, version {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("data directory: {}", paths.data_dir().display());

    // 3. Configuration (data/config/deact.toml).
    let (configuration, created) = match config::load_or_create(paths.config_path()) {
        Ok(pair) => pair,
        Err(err) => fail_after_logging(StartupStage::Application, &err.to_string()),
    };
    if created {
        tracing::info!("created default configuration");
    }
    tracing::info!(
        "configuration loaded (version {}, {}x{})",
        configuration.version,
        configuration.window_width,
        configuration.window_height
    );

    // 4. SQLite database worker (data/deact.db, WAL mode).
    let mut db_worker = match DbWorker::open(paths.database_path()) {
        Ok(worker) => worker,
        Err(err) => fail_after_logging(StartupStage::Application, &err.to_string()),
    };
    let schema_version = match db_worker.schema_version() {
        Ok(version) => version,
        Err(err) => fail_after_logging(StartupStage::Application, &err.to_string()),
    };
    tracing::info!("database ready, schema version {schema_version}");

    // Diagnostic builds can inject the post-logging failure here, before
    // the Tauri application is built.
    #[cfg(feature = "fault-injection")]
    if fault::injected_stage() == Some(StartupStage::Application) {
        fail_after_logging(StartupStage::Application, fault::INJECTED_DETAIL);
    }

    // 5. Tauri application. The per-directory override is installed on the
    // context before the builder runs; every directory comes from the
    // paths module, never from a system location.
    let state = AppState {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: paths.data_dir().to_path_buf(),
        config_path: paths.config_path().to_path_buf(),
        database_path: paths.database_path().to_path_buf(),
        webview_dir: paths.webview_dir().to_path_buf(),
        schema_version,
        config_version: configuration.version,
        window_width: configuration.window_width,
        window_height: configuration.window_height,
        log_level: configuration.log_level.clone(),
        runtime_ready: AtomicBool::new(false),
    };
    let mut context = tauri::generate_context!();
    context.config_mut().app.app_directories_override =
        Some(AppDirectoriesOverride::Directories(AppDirectoryOverrides {
            config: Some(paths.config_dir().to_path_buf()),
            data: Some(paths.data_dir().to_path_buf()),
            local_data: Some(paths.webview_dir().to_path_buf()),
            cache: Some(paths.cache_dir().to_path_buf()),
            log: Some(paths.logs_dir().to_path_buf()),
        }));
    let window_width = f64::from(configuration.window_width);
    let window_height = f64::from(configuration.window_height);
    let webview_dir = paths.webview_dir().to_path_buf();

    let app = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![app_status])
        .setup(move |app| {
            // The main window is created in Rust (never declared in
            // tauri.conf.json) so its WebView2 data directory can be the
            // absolute portable path data/webview/.
            let _window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("DeACT")
            .inner_size(window_width, window_height)
            .data_directory(webview_dir.clone())
            .build()?;

            // Background supervision on Tauri's async runtime, with the
            // health check the old standalone runtime used to provide.
            let supervisor = Supervisor::start();
            tauri::async_runtime::block_on(supervisor.ping())?;
            app.state::<AppState>()
                .runtime_ready
                .store(true, Ordering::SeqCst);
            tracing::info!("background supervisor ready");
            app.manage(supervisor);
            Ok(())
        });

    // 6. Run until the window closes. A builder/run failure (including a
    // failed window creation in the setup hook) is a post-logging startup
    // failure: log record, native dialog, exit.
    match app.run(context) {
        Ok(()) => {
            tracing::info!("DeACT shutting down");
        }
        Err(err) => {
            db_worker.shutdown();
            fail_after_logging(StartupStage::Application, &err.to_string());
        }
    }

    // 7. Orderly shutdown. The log guard drops after this, flushing the
    // file writer before the process exits.
    db_worker.shutdown();
}
