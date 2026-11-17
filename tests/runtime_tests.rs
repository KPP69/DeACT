//! Background runtime tests: start, health check, shutdown.

use deact::runtime::BackgroundRuntime;

#[test]
fn runtime_starts_answers_ping_and_shuts_down() {
    let mut runtime = BackgroundRuntime::start().expect("runtime starts");
    runtime.ping().expect("runtime answers a ping");
    runtime.shutdown();
    assert!(
        runtime.ping().is_err(),
        "ping after shutdown must fail instead of hanging"
    );
}

#[test]
fn runtime_shutdown_is_idempotent() {
    let mut runtime = BackgroundRuntime::start().expect("runtime starts");
    runtime.shutdown();
    runtime.shutdown();
}
