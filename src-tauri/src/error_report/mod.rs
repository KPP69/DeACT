//! Windows GUI startup error feedback (ARCHITECTURE.md section 3.6).
//!
//! DeACT is a Windows GUI-subsystem application: a Release build has no
//! console, so a startup failure must never rely on `eprintln!` or console
//! output. Failures are shown in a native Windows error dialog
//! (`MessageBoxW`). A failure that happens before logging is initialised
//! is shown in the dialog only; a failure after logging is initialised is
//! first recorded with `tracing::error!` and then shown in the dialog.
//! After the dialog the process exits; there is no silent continuation
//! and no fallback to a system directory.
//!
//! The message names the failed stage and a remedy, and never contains
//! secrets: the detail text comes from DeACT's own startup errors (paths,
//! configuration, SQLite, Tauri), which carry file paths and OS error
//! descriptions only — never keys, passwords or session data.
//!
//! This module is the only place that calls the Windows dialog API; no
//! business module calls a Windows API directly.

use std::fmt;

/// The startup stage a failure belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStage {
    /// Creating or probing the portable `data/` directory (pre-logging).
    DataDirectory,
    /// Initialising the logging system (pre-logging).
    Logging,
    /// Configuration, database, or Tauri application/window initialisation
    /// (post-logging).
    Application,
}

impl StartupStage {
    /// Short human-readable stage name used in the dialog and the log.
    pub fn name(self) -> &'static str {
        match self {
            StartupStage::DataDirectory => "data directory preparation",
            StartupStage::Logging => "logging initialisation",
            StartupStage::Application => "application initialisation",
        }
    }

    /// What the user can do about the failure, per stage.
    fn remedy(self) -> &'static str {
        match self {
            StartupStage::DataDirectory => {
                "Make sure the folder containing DeACT.exe is writable — \
                 for example move DeACT to a folder you own — then start \
                 DeACT again. DeACT stores everything in the data folder \
                 next to DeACT.exe and never uses a system folder instead."
            }
            StartupStage::Logging => {
                "Make sure the data\\logs folder next to DeACT.exe is \
                 writable and the disk is not full, then start DeACT again."
            }
            StartupStage::Application => {
                "Start DeACT again. If the problem persists, check the log \
                 file in data\\logs next to DeACT.exe for details."
            }
        }
    }
}

impl fmt::Display for StartupStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Build the exact text shown to the user for a startup failure. Pure and
/// unit-tested; the dialog itself is only shown by [`report`].
pub fn user_message(stage: StartupStage, detail: &str) -> String {
    format!(
        "DeACT could not start.\n\n\
         Failed stage: {stage}\n\
         Problem: {detail}\n\n\
         {remedy}\n\n\
         DeACT will now exit.",
        stage = stage.name(),
        remedy = stage.remedy(),
    )
}

/// Report a startup failure: log it first when logging is ready, then
/// show the native error dialog. The caller exits the process right after
/// this returns.
pub fn report(stage: StartupStage, detail: &str, logging_ready: bool) {
    if logging_ready {
        tracing::error!("startup failed during {}: {}", stage.name(), detail);
    }
    show_error_dialog(&user_message(stage, detail));
}

/// Show the native Windows error dialog. On non-Windows platforms (local
/// development only) the message goes to stderr instead; the shipped
/// Windows Release build always uses the dialog.
#[cfg(target_os = "windows")]
fn show_error_dialog(message: &str) {
    use std::iter::once;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let text: Vec<u16> = message.encode_utf16().chain(once(0)).collect();
    let caption: Vec<u16> = "DeACT — startup failed"
        .encode_utf16()
        .chain(once(0))
        .collect();
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive
    // the call, the parent window handle is null (no owner window exists
    // at startup), and the return value needs no cleanup.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Non-Windows fallback used only for local development builds.
#[cfg(not(target_os = "windows"))]
fn show_error_dialog(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_names_stage_problem_and_remedy() {
        let message = user_message(
            StartupStage::DataDirectory,
            "data directory X is not writable",
        );
        assert!(message.contains("data directory preparation"));
        assert!(message.contains("data directory X is not writable"));
        assert!(message.contains("writable"));
        assert!(message.contains("DeACT will now exit."));
    }

    #[test]
    fn every_stage_has_its_own_message() {
        let detail = "boom";
        let data = user_message(StartupStage::DataDirectory, detail);
        let logging = user_message(StartupStage::Logging, detail);
        let application = user_message(StartupStage::Application, detail);
        assert_ne!(data, logging);
        assert_ne!(logging, application);
        assert!(logging.contains("logging initialisation"));
        assert!(application.contains("application initialisation"));
        assert!(application.contains("data\\logs"));
    }
}
