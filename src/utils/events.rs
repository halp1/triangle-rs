use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1024;

/// Marker trait for strongly-typed events.
/// Kept for backwards compatibility with existing event type definitions.
pub trait Event: Clone + Send + 'static {
  const NAME: &'static str;
}

/// String-keyed broadcast event emitter backed by a tokio broadcast channel.
/// Events fan-out to all subscribers as `(command, data)` pairs.
#[derive(Clone)]
pub struct EventEmitter {
  tx: Arc<broadcast::Sender<(String, Value)>>,
}

impl EventEmitter {
  pub fn new() -> Self {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    Self { tx: Arc::new(tx) }
  }

  /// Subscribe to all events. Returns a receiver that yields `(command, data)`.
  pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
    self.tx.subscribe()
  }

  /// Broadcast an event to all current subscribers.
  pub fn emit(&self, command: &str, data: Value) {
    let _ = self.tx.send((command.to_string(), data));
  }

  /// Register a persistent callback for a specific command.
  /// Returns a `JoinHandle` — abort it to unregister.
  pub fn on<F>(&self, command: &str, callback: F) -> tokio::task::JoinHandle<()>
  where
    F: Fn(Value) + Send + 'static,
  {
    let command = command.to_string();
    let mut rx = self.tx.subscribe();
    tokio::spawn(async move {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == command => callback(data),
          Ok(_) => {}
          Err(broadcast::error::RecvError::Closed) => break,
          Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
      }
    })
  }

  /// Wait for the next occurrence of a specific command.
  pub async fn once(&self, command: &str) -> Option<Value> {
    let command = command.to_string();
    let mut rx = self.tx.subscribe();
    loop {
      match rx.recv().await {
        Ok((cmd, data)) if cmd == command => return Some(data),
        Ok(_) => {}
        Err(broadcast::error::RecvError::Closed) => return None,
        Err(broadcast::error::RecvError::Lagged(_)) => {}
      }
    }
  }
}
