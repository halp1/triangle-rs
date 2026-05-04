use std::future::{Future, Ready, ready};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1024;

pub trait Event: serde::de::DeserializeOwned + serde::Serialize + Clone + Send + 'static {
  const NAME: &'static str;
}

pub trait AsyncCallback<T>: Clone + Send + 'static {
  type Future: Future<Output = ()> + Send + 'static;
  fn call(self, arg: T) -> Self::Future;
}

impl<T, F, Fut> AsyncCallback<T> for F
where
  F: FnOnce(T) -> Fut + Clone + Send + 'static,
  Fut: Future<Output = ()> + Send + 'static,
{
  type Future = Fut;
  fn call(self, arg: T) -> Fut {
    (self)(arg)
  }
}

pub struct SyncFn<F>(pub F);

impl<F: Clone> Clone for SyncFn<F> {
  fn clone(&self) -> Self {
    SyncFn(self.0.clone())
  }
}

impl<T, F> AsyncCallback<T> for SyncFn<F>
where
  F: FnOnce(T) + Clone + Send + 'static,
  T: Send + 'static,
{
  type Future = Ready<()>;
  fn call(self, arg: T) -> Ready<()> {
    (self.0)(arg);
    ready(())
  }
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

  pub fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> tokio::task::JoinHandle<()> {
    let mut rx = self.tx.subscribe();
    tokio::spawn(async move {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == T::NAME => {
            if let Ok(parsed) = serde_json::from_value::<T>(data) {
              callback.clone().call(parsed).await;
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
