//! Background Tokio runtime.
//!
//! The runtime lives on its own threads, never on the UI thread. Commands
//! and events travel through one bounded channel, and background tasks
//! never touch UI state directly — the UI only receives snapshots that the
//! main thread hands it. This stage connects no network service; the
//! runtime exists so later stages have a controlled place to run one.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use tokio::runtime::{Builder, Runtime};
use tokio::sync::{mpsc, oneshot};

/// Maximum number of runtime commands waiting to be processed.
const CHANNEL_CAPACITY: usize = 64;

/// Errors from the background runtime.
#[derive(Debug)]
pub enum RuntimeError {
    /// The Tokio runtime could not be built.
    BuildFailed(std::io::Error),
    /// The runtime is stopped or its command channel is closed.
    NotRunning,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::BuildFailed(source) => {
                write!(f, "cannot start background runtime: {source}")
            }
            RuntimeError::NotRunning => {
                write!(f, "background runtime is not running")
            }
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RuntimeError::BuildFailed(source) => Some(source),
            RuntimeError::NotRunning => None,
        }
    }
}

/// Commands the main thread can send to the background runtime.
enum RuntimeCommand {
    /// Health check: the supervisor task answers as soon as it processes
    /// the command, proving the channel and the task are alive.
    Ping { reply: oneshot::Sender<()> },
    /// Stop the supervisor task.
    Shutdown,
}

/// A background multi-thread Tokio runtime with a bounded command channel.
pub struct BackgroundRuntime {
    runtime: Option<Runtime>,
    tx: Option<mpsc::Sender<RuntimeCommand>>,
}

impl BackgroundRuntime {
    /// Start the runtime and its supervisor task.
    pub fn start() -> Result<Self, RuntimeError> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("deact-runtime")
            .enable_all()
            .build()
            .map_err(RuntimeError::BuildFailed)?;
        let (tx, mut rx) = mpsc::channel::<RuntimeCommand>(CHANNEL_CAPACITY);
        runtime.spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    RuntimeCommand::Ping { reply } => {
                        let _ = reply.send(());
                    }
                    RuntimeCommand::Shutdown => break,
                }
            }
        });
        Ok(Self {
            runtime: Some(runtime),
            tx: Some(tx),
        })
    }

    /// Health check: send a ping through the bounded channel and wait for
    /// the supervisor task to answer.
    pub fn ping(&self) -> Result<(), RuntimeError> {
        let tx = self.tx.as_ref().ok_or(RuntimeError::NotRunning)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.blocking_send(RuntimeCommand::Ping { reply: reply_tx })
            .map_err(|_| RuntimeError::NotRunning)?;
        reply_rx
            .blocking_recv()
            .map_err(|_| RuntimeError::NotRunning)?;
        Ok(())
    }

    /// Stop the supervisor task and shut the runtime down (at most 5
    /// seconds). Safe to call more than once; also runs on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.blocking_send(RuntimeCommand::Shutdown);
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

impl Drop for BackgroundRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
