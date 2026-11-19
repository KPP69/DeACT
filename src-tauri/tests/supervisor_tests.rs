//! Background supervisor tests: start, health check, shutdown.
//!
//! The supervisor runs on Tauri's async runtime; the tests drive it with
//! `tauri::async_runtime::block_on`, the same runtime the application
//! uses. No second Tokio runtime is created anywhere in these tests.

use deact_lib::supervisor::Supervisor;

#[test]
fn supervisor_starts_answers_ping_and_shuts_down() {
    let mut supervisor = Supervisor::start();
    tauri::async_runtime::block_on(supervisor.ping()).expect("supervisor answers a ping");
    supervisor.shutdown();
    let after = tauri::async_runtime::block_on(supervisor.ping());
    assert!(
        after.is_err(),
        "ping after shutdown must fail instead of hanging"
    );
}

#[test]
fn supervisor_shutdown_is_idempotent() {
    let mut supervisor = Supervisor::start();
    supervisor.shutdown();
    supervisor.shutdown();
}
