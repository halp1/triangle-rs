use std::sync::Arc;

use tokio::sync::Mutex;

use crate::utils::events::{AsyncCallback, Event};

use super::{Ribbon, WrapError};

#[derive(Debug, Clone)]
pub struct Hook {
  ribbon: Ribbon,
  handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl Hook {
  pub fn new(ribbon: Ribbon) -> Self {
    Self {
      ribbon,
      handles: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub async fn on<T: Event>(
    &mut self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> &mut Self {
    let handle = self.ribbon.on(callback).await;
    self.handles.lock().await.push(handle);
    self
  }

  pub async fn once<T: Event>(
    &mut self,
    callback: impl Fn(T) + Send + Sync + 'static,
  ) -> &mut Self {
    let handle = self.ribbon.once(callback).await;
    self.handles.lock().await.push(handle);
    self
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    self.ribbon.wait::<T>().await
  }

  pub async fn wrap<T: Event>(&mut self, event: impl Event) -> std::result::Result<T, WrapError> {
    self.ribbon.wrap::<T>(event).await
  }

  pub async fn wrap_with_error<T: Event>(
    &mut self,
    event: impl Event,
    error_events: &[&str],
  ) -> std::result::Result<T, WrapError> {
    self.ribbon.wrap_with_error::<T>(event, error_events).await
  }

  pub async fn destroy(&mut self) {
    for handle in self.handles.lock().await.drain(..) {
      handle.abort();
    }
  }
}

impl Drop for Hook {
  fn drop(&mut self) {
		let handles = self.handles.clone();
    tokio::spawn(async move {
			for handle in handles.lock().await.drain(..) {
				handle.abort();
			}
    });
  }
}
