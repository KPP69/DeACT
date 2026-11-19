//! Background task supervision on Tauri's async runtime.
//!
//! All application background async work runs on the runtime Tauri
//! provides (`tauri::async_runtime`); DeACT never creates a second,
//! independent Tokio runtime. This module carries over the supervision
//! design of the old standalone runtime — one bounded command channel and
//! a supervisor task that answers health checks — without owning a
//! runtime. Commands travel through the bounded channel, background tasks
//! never touch UI state directly, and SQLite stays on its own synchronous
//! worker thread (see the `storage` module), never on this runtime.
//!
//! This stage connects no network service; the supervisor exists so later
//! stages have a controlled place to run background tasks.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

/// Maximum number of supervisor commands waiting to be processed.
const CHANNEL_CAPACITY: usize = 64;

/// How long a health check waits for the supervisor task to answer.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors from the background supervisor.
#[derive(Debug)]
pub enum SupervisorError {
    /// The supervisor task is stopped or its command channel is closed.
    NotRunning,
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupervisorError::NotRunning => {
                write!(f, "background supervisor is not running")
            }
        }
    }
}

impl Error for SupervisorError {}

/// Commands sent to the supervisor task.
enum SupervisorCommand {
    /// Health check: the supervisor answers as soon as it processes the
    /// command, proving the channel and the task are alive.
    Ping { reply: oneshot::Sender<()> },
    /// Stop the supervisor task.
    Shutdown,
}

/// A supervisor task running on Tauri's async runtime, fed by a bounded
/// command channel.
pub struct Supervisor {
    tx: Option<mpsc::Sender<SupervisorCommand>>,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Supervisor {
    /// Start the supervisor task on Tauri's async runtime.
    pub fn start() -> Self {
        let (tx, mut rx) = mpsc::channel::<SupervisorCommand>(CHANNEL_CAPACITY);
        let handle = tauri::async_runtime::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    SupervisorCommand::Ping { reply } => {
                        let _ = reply.send(());
                    }
                    SupervisorCommand::Shutdown => break,
                }
            }
        });
        Self {
            tx: Some(tx),
            handle: Some(handle),
        }
    }

    /// Health check: send a ping through the bounded channel and wait for
    /// the supervisor task to answer.
    pub async fn ping(&self) -> Result<(), SupervisorError> {
        let tx = self.tx.as_ref().ok_or(SupervisorError::NotRunning)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(SupervisorCommand::Ping { reply: reply_tx })
            .await
            .map_err(|_| SupervisorError::NotRunning)?;
        tokio::time::timeout(PING_TIMEOUT, reply_rx)
            .await
            .map_err(|_| SupervisorError::NotRunning)?
            .map_err(|_| SupervisorError::NotRunning)?;
        Ok(())
    }

    /// Stop the supervisor task. Safe to call more than once; also runs on
    /// drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.try_send(SupervisorCommand::Shutdown);
            drop(tx);
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}
