//! DeACT entry point: a thin shell around `deact_lib::run`, which owns
//! the whole ordered startup sequence (portable `data/` directory,
//! logging, configuration, SQLite worker, Tauri application).

// The Windows release build is a GUI-subsystem application: double-clicking
// DeACT.exe opens only the native window, with no console window attached.
// Debug builds keep the console subsystem so startup output stays visible
// during development. Startup failures in the Release GUI build are shown
// in a native error dialog (see the `error_report` module), never through
// the console; the attribute does not apply to non-Windows targets.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    deact_lib::run();
}
