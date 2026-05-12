use std::future::{Future, Ready, ready};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use serde_json::Value;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1024;

pub trait Event:
  serde::de::DeserializeOwned + serde::Serialize + Clone + std::fmt::Debug + Send + 'static
{
  const NAME: &'static str;
  fn name(&self) -> &'static str {
    Self::NAME
  }
}

#[derive(Debug)]
pub enum WrapError {
  ServerError,
  ParseError(serde_json::Error),
  Error(String, Value),
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

#[derive(Debug)]
pub struct EventEmitter {
  tx: Arc<ArcSwapOption<broadcast::Sender<(String, Value)>>>,
}

impl Clone for EventEmitter {
  fn clone(&self) -> Self {
    Self {
      tx: self.tx.clone(),
    }
  }
}

impl EventEmitter {
  pub fn new() -> Self {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    Self {
      tx: Arc::new(ArcSwapOption::new(Some(Arc::new(tx)))),
    }
  }

  pub fn subscribe(&self) -> Option<broadcast::Receiver<(String, Value)>> {
    self.tx.load().as_deref().map(|tx| tx.subscribe())
  }

  pub fn emit_raw(&self, command: &str, data: Value) {
    if let Some(tx) = self.tx.load().as_deref() {
      let _ = tx.send((command.to_string(), data));
    }
  }

  pub fn emit<T: Event>(&self, event: T) {
    let data = serde_json::to_value(&event).unwrap_or(Value::Null);
    self.emit_raw(T::NAME, data);
  }

  pub fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> tokio::task::JoinHandle<()> {
    let Some(mut rx) = self.subscribe() else {
			tracing::error!("Failed to subscribe to events: no broadcaster available");
      return tokio::spawn(async {});
    };
    tokio::spawn(async move {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == T::NAME => match serde_json::from_value::<T>(data) {
            Ok(parsed) => {
              callback.clone().call(parsed).await;
            }
            Err(e) => {
              tracing::error!("Failed to parse event {}: {}", T::NAME, e);
            }
          },
          Ok(_) => {}
          Err(broadcast::error::RecvError::Closed) => break,
          Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
      }
    })
  }

  pub async fn once_raw(&self, command: &str) -> Option<Value> {
    let command = command.to_string();
    let mut rx = self.subscribe()?;
    loop {
      match rx.recv().await {
        Ok((cmd, data)) if cmd == command => return Some(data),
        Ok(_) => {}
        Err(broadcast::error::RecvError::Closed) => return None,
        Err(broadcast::error::RecvError::Lagged(_)) => {}
      }
    }
  }

  pub fn once<T: Event>(
    &self,
    callback: impl Fn(T) + Send + 'static,
  ) -> tokio::task::JoinHandle<()> {
    let emitter = self.clone();
    tokio::spawn(async move {
      emitter.wait::<T>().await.map(callback);
    })
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    let data = self.once_raw(T::NAME).await?;
    serde_json::from_value::<T>(data)
      .map_err(|e| {
        tracing::error!("Failed to parse event {}: {}", T::NAME, e);
        e
      })
      .ok()
  }

  pub async fn wrap_with_error<T: Event>(
    &self,
    emit: impl Future<Output = ()> + Send,
    error_events: &[&str],
  ) -> Result<T, WrapError> {
    let Some(mut rx) = self.subscribe() else {
      return Err(WrapError::ServerError);
    };
    emit.await;
    loop {
      match rx.recv().await {
        Ok((cmd, data)) if error_events.contains(&cmd.as_str()) => {
          return Err(WrapError::Error(cmd, data));
        }
        Ok((cmd, data)) if cmd == T::NAME => {
          return match serde_json::from_value::<T>(data) {
            Ok(parsed) => Ok(parsed),
            Err(e) => {
              tracing::error!("Failed to parse event {}: {}", T::NAME, e);
              Err(WrapError::ParseError(e))
            }
          };
        }
        Ok(_) => {}
        Err(broadcast::error::RecvError::Closed) => return Err(WrapError::ServerError),
        Err(broadcast::error::RecvError::Lagged(_)) => {}
      }
    }
  }

  pub async fn wrap<T: Event>(
    &self,
    emit: impl Future<Output = ()> + Send,
  ) -> Result<T, WrapError> {
    self.wrap_with_error::<T>(emit, &["client.error"]).await
  }

  pub fn hook(&self) -> crate::classes::ribbon::Hook {
    crate::classes::ribbon::Hook::new(self.clone())
  }

  pub fn destroy(&self) {
    self.tx.store(None);
  }

  pub fn clear(&self) {
    let (new_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    self.tx.store(Some(Arc::new(new_tx)));
  }
}
