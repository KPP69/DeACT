//! Startup fault injection for diagnostic builds.
//!
//! This module exists only when the `fault-injection` Cargo feature is
//! enabled. It is never compiled into a normal Release build: the
//! production binary contains no hidden failure switch. The diagnostic
//! build is a separate artifact, used only to verify the Windows GUI
//! startup error feedback (native dialog, log record, exit) on a real
//! Windows 11 machine.
//!
//! Usage (diagnostic build only):
//! `DeACT.exe --inject-failure data-dir|logging|application`
//!
//! The injected failure travels through the exact same reporting path as
//! a real failure of that stage.

use crate::error_report::StartupStage;

/// The stage selected by `--inject-failure`, if any.
pub fn injected_stage() -> Option<StartupStage> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = if let Some(value) = arg.strip_prefix("--inject-failure=") {
            Some(value.to_string())
        } else if arg == "--inject-failure" {
            args.next()
        } else {
            None
        };
        if let Some(value) = value {
            return match value.as_str() {
                "data-dir" => Some(StartupStage::DataDirectory),
                "logging" => Some(StartupStage::Logging),
                "application" => Some(StartupStage::Application),
                _ => None,
            };
        }
    }
    None
}

/// Detail text used for an injected failure, so a diagnostic run is
/// unmistakable in the dialog and in the log.
pub const INJECTED_DETAIL: &str =
    "injected diagnostic failure (fault-injection build, not a real error)";
