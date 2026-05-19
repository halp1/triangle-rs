use std::sync::Arc;

use parking_lot::Mutex;

use crate::utils::{
  EventEmitter,
  events::{AsyncCallback, Event},
};

#[derive(Clone)]
pub struct Hook {
  emitter: EventEmitter,
  handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl std::fmt::Debug for Hook {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Hook")
      .field("handles", &self.handles)
      .finish()
  }
}

impl Hook {
  pub fn new(emitter: EventEmitter) -> Self {
    Self {
      emitter,
      handles: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> &Self {
    let handle = self.emitter.on(callback);
    self.handles.lock().push(handle);
    self
  }

  pub fn once<T: Event>(&self, callback: impl Fn(T) + Send + 'static) -> &Self {
    let handle = self.emitter.once(callback);
    self.handles.lock().push(handle);
    self
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    self.emitter.wait::<T>().await
  }

  pub fn destroy(&self) {
    for handle in self.handles.lock().drain(..) {
      handle.abort();
    }
  }
}

impl Drop for Hook {
  fn drop(&mut self) {
    if std::sync::Arc::strong_count(&self.handles) > 1 {
      return;
    }
    let handles = self.handles.clone();
    if let Ok(rt) = tokio::runtime::Handle::try_current() {
      rt.spawn(async move {
        for handle in handles.lock().drain(..) {
          handle.abort();
        }
      });
    } else {
      if let Some(mut guard) = self.handles.try_lock() {
        for handle in guard.drain(..) {
          handle.abort();
        }
      }
    }
  }
}
