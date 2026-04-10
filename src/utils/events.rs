use tokio::sync::broadcast;

/// A generic event emitter backed by a tokio broadcast channel.
///
/// All events are dispatched as `(command, data)` pairs where `data`
/// is an arbitrary JSON value. Subscribers receive a clone of every
/// event emitted while their receiver is alive.
#[derive(Clone)]
pub struct EventEmitter {
  sender: broadcast::Sender<(String, serde_json::Value)>,
}

impl EventEmitter {
  pub fn new() -> Self {
    let (sender, _) = broadcast::channel(512);
    Self { sender }
  }

  /// Emit `(command, data)` to all current subscribers.
  pub fn emit(&self, command: &str, data: serde_json::Value) {
    let _ = self.sender.send((command.to_string(), data));
  }

  /// Create a new receiver that will receive all future events.
  pub fn subscribe(&self) -> broadcast::Receiver<(String, serde_json::Value)> {
    self.sender.subscribe()
  }

  /// Await the next occurrence of `command`, skipping all other events.
  pub async fn once(&self, command: &str) -> Option<serde_json::Value> {
    let mut rx = self.subscribe();
    let command = command.to_string();
    loop {
      match rx.recv().await {
        Ok((cmd, data)) if cmd == command => return Some(data),
        Ok(_) => continue,
        Err(broadcast::error::RecvError::Closed) => return None,
        Err(broadcast::error::RecvError::Lagged(_)) => continue,
      }
    }
  }

  /// Register a persistent callback that fires whenever `command` is emitted.
  /// The callback runs in a spawned task; drop the returned `JoinHandle` to
  /// cancel it.
  pub fn on<F>(&self, command: &str, callback: F) -> tokio::task::JoinHandle<()>
  where
    F: Fn(serde_json::Value) + Send + 'static,
  {
    let mut rx = self.subscribe();
    let command = command.to_string();
    tokio::spawn(async move {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == command => callback(data),
          Ok(_) => {}
          Err(broadcast::error::RecvError::Closed) => break,
          Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
      }
    })
  }
}

impl Default for EventEmitter {
  fn default() -> Self {
    Self::new()
  }
}
