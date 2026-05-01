use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1024;

pub trait Event: serde::de::DeserializeOwned + serde::Serialize + Clone + Send + 'static {
  const NAME: &'static str;
}

#[derive(Debug, Clone)]
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

  pub fn emit_raw(&self, command: &str, data: Value) {
    let _ = self.tx.send((command.to_string(), data));
  }

  pub fn emit<T: Event>(&self, event: T) {
    let data = serde_json::to_value(&event).unwrap_or(Value::Null);
    self.emit_raw(T::NAME, data);
  }

  /// Listen for a specific event type. The callback will be called with the parsed event data.

  pub fn on<T, F>(&self, callback: F) -> tokio::task::JoinHandle<()>
  where
    F: Fn(T) + Send + Sync + 'static,
    T: Event,
  {
    let mut rx = self.tx.subscribe();
    tokio::spawn(async move {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == T::NAME => {
            if let Ok(parsed) = serde_json::from_value::<T>(data) {
              callback(parsed);
            }
          }
          Ok(_) => {}
          Err(broadcast::error::RecvError::Closed) => break,
          Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
      }
    })
  }

  pub async fn once_raw(&self, command: &str) -> Option<Value> {
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

  pub async fn once<T: Event>(&self) -> Option<T> {
    let data = self.once_raw(T::NAME).await?;
    serde_json::from_value(data).ok()
  }
}
