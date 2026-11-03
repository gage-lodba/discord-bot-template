//! Shard lifecycle and graceful shutdown.
//!
//! A [`ShardManager`] owns the signals shared between the [`Client`] running the
//! gateway connection and the bot code that wants to stop it. `shutdown_all`
//! asks the shard to close with code `1000`/`1001` (a clean, session-invalidating
//! close) and waits until the connection has ended, so `Client::start` can return
//! `Ok(())` instead of being killed mid-write.
//!
//! Reference: `docs/gateway.md` (Initiating a Disconnect, Connection Lifecycle),
//! `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes).
//!
//! [`Client`]: crate::client::Client

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::watch;

/// Handle for stopping the gateway connection(s).
///
/// The bot clones the `Arc<ShardManager>` out of a client's `shard_manager`
/// field and calls [`ShardManager::shutdown_all`] when it is time to stop.
#[derive(Debug)]
pub struct ShardManager {
    shutdown_requested: Arc<AtomicBool>,
    shutdown_tx: watch::Sender<bool>,
    // A `watch` sender fails to send when every receiver has been dropped, so
    // keep one receiver alive for the manager's lifetime.
    _shutdown_keepalive: watch::Receiver<bool>,
    finished: watch::Sender<bool>,
    _finished_keepalive: watch::Receiver<bool>,
}

impl ShardManager {
    /// Creates a manager with no shard running yet.
    pub(crate) fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (finished, finished_rx) = watch::channel(false);

        Self {
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            shutdown_tx,
            _shutdown_keepalive: shutdown_rx,
            finished,
            _finished_keepalive: finished_rx,
        }
    }

    /// Requests a clean shutdown of every shard and waits until they have ended.
    ///
    /// The shard closes the websocket with code `1000` (normal closure), which
    /// invalidates the session and marks the bot offline per
    /// `docs/gateway.md` (Initiating a Disconnect).
    pub async fn shutdown_all(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(true);

        let mut finished = self.finished.subscribe();
        let _ = finished.wait_for(|done| *done).await;
    }

    /// Creates a receiver that reports shutdown requests.
    pub(crate) fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    /// Returns true when [`ShardManager::shutdown_all`] has been called.
    pub(crate) fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }

    /// Marks the shard as finished so [`ShardManager::shutdown_all`] can return.
    pub(crate) fn mark_finished(&self) {
        let _ = self.finished.send(true);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn shutdown_all_signals_then_waits_until_the_shard_finishes() {
        let manager = Arc::new(ShardManager::new());
        let mut shutdown = manager.subscribe_shutdown();

        let waiting = tokio::spawn({
            let manager = Arc::clone(&manager);
            async move { manager.shutdown_all().await }
        });

        shutdown
            .changed()
            .await
            .expect("the shutdown signal is sent");
        assert!(*shutdown.borrow());
        assert!(manager.is_shutdown_requested());

        // `shutdown_all` must not return before the shard has ended.
        manager.mark_finished();

        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("shutdown_all returns once the shard has finished")
            .expect("the shutdown task did not panic");
    }

    #[tokio::test]
    async fn shutdown_all_returns_immediately_for_an_already_finished_shard() {
        let manager = Arc::new(ShardManager::new());
        manager.mark_finished();

        tokio::time::timeout(Duration::from_secs(1), manager.shutdown_all())
            .await
            .expect("an already-finished shard does not block shutdown");
    }
}
