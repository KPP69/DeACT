//! Minimal native desktop window (eframe / egui).
//!
//! The UI only displays a snapshot of the startup state handed to it by
//! `main`. It performs no database access, no filesystem scanning and no
//! network I/O in its callbacks — all of that happened before the window
//! was created, on other threads. This is intentionally a plain status
//! window, not the final product UI.

use std::path::PathBuf;

use eframe::egui;

/// Immutable snapshot of the startup state, rendered by the window.
#[derive(Debug, Clone)]
pub struct AppStatus {
    /// Application version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// Portable data directory (`DeACT_Data/` beside the executable).
    pub data_dir: PathBuf,
    /// Whether the data directory was created and verified writable.
    pub data_dir_ready: bool,
    /// Whether the SQLite database was initialised.
    pub database_ready: bool,
    /// Schema version recorded in the database, when known.
    pub schema_version: Option<i64>,
    /// Whether the background runtime answered its health check.
    pub runtime_ready: bool,
}

impl AppStatus {
    /// One-line overall state for the status display.
    pub fn overall(&self) -> &'static str {
        if self.data_dir_ready && self.database_ready && self.runtime_ready {
            "Running"
        } else {
            "Degraded — see status details"
        }
    }
}

/// The DeACT application window.
pub struct DeactApp {
    status: AppStatus,
}

impl DeactApp {
    /// Build the window from a startup status snapshot.
    pub fn new(status: AppStatus) -> Self {
        Self { status }
    }
}

impl eframe::App for DeactApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("DeACT");
        ui.label(format!("Version: {}", self.status.version));
        ui.label(format!("Status: {}", self.status.overall()));
        ui.separator();
        ui.label(format!(
            "Data directory: {}",
            self.status.data_dir.display()
        ));
        ui.label(format!(
            "Data directory ready: {}",
            yes_no(self.status.data_dir_ready)
        ));
        ui.label(format!(
            "Database ready: {}",
            yes_no(self.status.database_ready)
        ));
        ui.label(format!(
            "Database schema version: {}",
            self.status
                .schema_version
                .map_or_else(|| "unknown".to_string(), |version| version.to_string())
        ));
        ui.label(format!(
            "Background runtime ready: {}",
            yes_no(self.status.runtime_ready)
        ));
        ui.separator();
        ui.label("Foundation stage: no trading functionality yet.");
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// Open the native desktop window. Blocks until the window is closed.
pub fn run(status: AppStatus) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DeACT",
        options,
        Box::new(|_creation_context| Ok(Box::new(DeactApp::new(status)))),
    )
}
