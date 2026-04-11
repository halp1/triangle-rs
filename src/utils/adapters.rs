use std::{collections::HashMap, process::Stdio, sync::Arc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::{
  io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
  process::{Child, ChildStdin, Command},
  sync::{Mutex, broadcast},
};

use crate::{
  Engine,
  engine::{EngineSnapshot, queue::types::Mino},
  error::{Result, TriangleError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterKey {
  #[serde(rename = "moveLeft")]
  MoveLeft,
  #[serde(rename = "moveRight")]
  MoveRight,
  #[serde(rename = "dasLeft")]
  DasLeft,
  #[serde(rename = "dasRight")]
  DasRight,
  #[serde(rename = "softDrop")]
  SoftDrop,
  #[serde(rename = "hardDrop")]
  HardDrop,
  #[serde(rename = "rotateCCW")]
  RotateCcw,
  #[serde(rename = "rotateCW")]
  RotateCw,
  #[serde(rename = "rotate180")]
  Rotate180,
  #[serde(rename = "hold")]
  Hold,
}

impl AdapterKey {
  pub fn as_input_key(&self) -> &'static str {
    match self {
      Self::MoveLeft | Self::DasLeft => "moveLeft",
      Self::MoveRight | Self::DasRight => "moveRight",
      Self::SoftDrop => "softDrop",
      Self::HardDrop => "hardDrop",
      Self::RotateCcw => "rotateCCW",
      Self::RotateCw => "rotateCW",
      Self::Rotate180 => "rotate180",
      Self::Hold => "hold",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct AdapterInfo<T = Value> {
  #[serde(rename = "type")]
  pub kind: String,
  pub name: String,
  pub version: String,
  pub author: String,
  #[serde(default)]
  pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct AdapterMove<T = Value> {
  #[serde(rename = "type")]
  pub kind: String,
  pub keys: Vec<AdapterKey>,
  #[serde(default)]
  pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
#[serde(tag = "type")]
pub enum IncomingMessage<T = Value> {
  #[serde(rename = "info")]
  Info {
    name: String,
    version: String,
    author: String,
    #[serde(default)]
    data: Option<T>,
  },
  #[serde(rename = "move")]
  Move {
    keys: Vec<AdapterKey>,
    #[serde(default)]
    data: Option<T>,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingConfig<T = Value> {
  #[serde(rename = "type")]
  pub kind: &'static str,
  pub board_width: usize,
  pub board_height: usize,
  pub kicks: String,
  pub spins: String,
  pub combo_table: String,
  #[serde(rename = "b2bCharing")]
  pub b2b_charing: bool,
  pub b2b_charge_at: i32,
  pub b2b_charge_base: i32,
  pub b2b_chaining: bool,
  pub garbage_multiplier: f64,
  pub garbage_cap: f64,
  pub garbage_special_bonus: bool,
  pub pc_b2b: i32,
  pub pc_garbage: f64,
  pub queue: Vec<Mino>,
  #[serde(default)]
  pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingState<T = Value> {
  #[serde(rename = "type")]
  pub kind: &'static str,
  pub board: Vec<Vec<Option<Mino>>>,
  pub current: Mino,
  pub hold: Option<Mino>,
  pub queue: Vec<Mino>,
  pub garbage: Vec<i32>,
  pub combo: i32,
  pub b2b: i32,
  #[serde(default)]
  pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingPieces<T = Value> {
  #[serde(rename = "type")]
  pub kind: &'static str,
  pub pieces: Vec<Mino>,
  #[serde(default)]
  pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
#[serde(rename_all = "camelCase")]
pub struct OutgoingPlay<T = Value> {
  #[serde(rename = "type")]
  pub kind: &'static str,
  pub garbage_multiplier: f64,
  pub garbage_cap: f64,
  #[serde(default)]
  pub data: Option<T>,
}

pub trait Adapter<T = Value>
where
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  async fn initialize(&mut self) -> Result<AdapterInfo<T>>;

  async fn config(&mut self, engine: &Engine, data: Option<T>) -> Result<()>;

  async fn update(&mut self, engine: &Engine, data: Option<T>) -> Result<()>;

  async fn add_pieces(&mut self, pieces: Vec<Mino>, data: Option<T>) -> Result<()>;

  async fn play(&mut self, engine: &Engine, data: Option<T>) -> Result<AdapterMove<T>>;

  async fn stop(&mut self) -> Result<()>;
}

pub fn config_from_engine<T>(engine: &Engine, data: Option<T>) -> OutgoingConfig<T> {
  let queue = engine.queue.snapshot().value;

  OutgoingConfig {
    kind: "config",
    board_width: engine.board.width,
    board_height: engine.board.height,
    kicks: engine.initializer.kick_table.clone(),
    spins: engine.game_options.spin_bonuses.clone(),
    combo_table: engine.game_options.combo_table.clone(),
    b2b_charing: engine.b2b.charging.is_some(),
    b2b_charge_at: engine.b2b.charging.as_ref().map(|v| v.at).unwrap_or(0),
    b2b_charge_base: engine.b2b.charging.as_ref().map(|v| v.base).unwrap_or(0),
    b2b_chaining: engine.b2b.chaining,
    garbage_multiplier: engine.dynamic.1.get(),
    garbage_cap: engine.dynamic.2.get(),
    garbage_special_bonus: engine.initializer.garbage.special_bonus,
    pc_b2b: engine.pc.as_ref().map(|v| v.b2b).unwrap_or(0),
    pc_garbage: engine.pc.as_ref().map(|v| v.garbage).unwrap_or(0.0),
    queue,
    data,
  }
}

pub fn state_from_engine<T>(engine: &Engine, data: Option<T>) -> OutgoingState<T> {
  let snapshot = engine.snapshot(false);

  OutgoingState {
    kind: "state",
    board: engine
      .board
      .state
      .iter()
      .map(|row| {
        row
          .iter()
          .map(|tile| tile.as_ref().map(|v| v.mino))
          .collect()
      })
      .collect(),
    current: engine.falling.symbol,
    hold: engine.held,
    queue: engine.queue.snapshot().value,
    garbage: snapshot
      .garbage
      .queue
      .into_iter()
      .map(|item| item.amount)
      .collect(),
    combo: engine.stats.combo,
    b2b: engine.stats.b2b,
    data,
  }
}

pub fn play_from_engine<T>(engine: &Engine, data: Option<T>) -> OutgoingPlay<T> {
  OutgoingPlay {
    kind: "play",
    garbage_multiplier: engine.dynamic.1.get(),
    garbage_cap: engine.dynamic.2.get().floor(),
    data,
  }
}

#[derive(Debug, Clone)]
pub struct AdapterIoConfig {
  pub name: String,
  pub verbose: bool,
  pub path: String,
  pub env: HashMap<String, String>,
  pub args: Vec<String>,
}

impl AdapterIoConfig {
  pub fn new(path: impl Into<String>) -> Self {
    Self {
      name: "AdapterIO".to_string(),
      verbose: false,
      path: path.into(),
      env: HashMap::new(),
      args: Vec::new(),
    }
  }
}

pub struct AdapterIo<T = Value>
where
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  pub config: AdapterIoConfig,
  child: Option<Child>,
  stdin: Option<Arc<Mutex<ChildStdin>>>,
  events: broadcast::Sender<IncomingMessage<T>>,
  dead: bool,
}

impl<T> AdapterIo<T>
where
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  pub fn new(config: AdapterIoConfig) -> Self {
    let (events, _) = broadcast::channel(128);
    Self {
      config,
      child: None,
      stdin: None,
      events,
      dead: false,
    }
  }

  async fn start(&mut self) -> Result<()> {
    if self.child.is_some() {
      return Ok(());
    }

    let mut command = Command::new(&self.config.path);
    command.args(&self.config.args);
    command
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());
    for (key, value) in &self.config.env {
      command.env(key, value);
    }

    let mut child = command.spawn().map_err(|e| {
      TriangleError::Adapter(format!(
        "failed to spawn adapter {}: {}",
        self.config.path, e
      ))
    })?;

    let stdout = child
      .stdout
      .take()
      .ok_or_else(|| TriangleError::Adapter("adapter stdout unavailable".to_string()))?;
    let stderr = child
      .stderr
      .take()
      .ok_or_else(|| TriangleError::Adapter("adapter stderr unavailable".to_string()))?;
    let stdin = child
      .stdin
      .take()
      .ok_or_else(|| TriangleError::Adapter("adapter stdin unavailable".to_string()))?;

    self.stdin = Some(Arc::new(Mutex::new(stdin)));
    self.child = Some(child);

    let sender = self.events.clone();
    let verbose = self.config.verbose;
    let name = self.config.name.clone();
    tokio::spawn(async move {
      let mut lines = BufReader::new(stdout).lines();
      while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
          continue;
        }

        match serde_json::from_str::<IncomingMessage<T>>(trimmed) {
          Ok(message) => {
            let _ = sender.send(message);
          }
          Err(_) if verbose => {
            eprintln!("[{}] {}", name, trimmed);
          }
          Err(_) => {}
        }
      }
    });

    let verbose = self.config.verbose;
    let name = self.config.name.clone();
    tokio::spawn(async move {
      let mut lines = BufReader::new(stderr).lines();
      while let Ok(Some(line)) = lines.next_line().await {
        if verbose {
          eprintln!("[{}] {}", name, line);
        }
      }
    });

    Ok(())
  }

  async fn send<M>(&mut self, message: &M) -> Result<()>
  where
    M: Serialize,
  {
    if self.dead {
      return Ok(());
    }

    let Some(stdin) = &self.stdin else {
      return Err(TriangleError::Adapter(
        "adapter stdin unavailable".to_string(),
      ));
    };

    let payload = serde_json::to_vec(message)?;
    let mut writer = stdin.lock().await;
    writer.write_all(&payload).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
  }

  async fn await_info(&self) -> Result<AdapterInfo<T>> {
    let mut rx = self.events.subscribe();
    loop {
      match rx.recv().await {
        Ok(IncomingMessage::Info {
          name,
          version,
          author,
          data,
        }) => {
          return Ok(AdapterInfo {
            kind: "info".to_string(),
            name,
            version,
            author,
            data,
          });
        }
        Ok(_) => {}
        Err(broadcast::error::RecvError::Lagged(_)) => {}
        Err(broadcast::error::RecvError::Closed) => {
          return Err(TriangleError::Adapter("adapter channel closed".to_string()));
        }
      }
    }
  }

  async fn await_move(&self) -> Result<AdapterMove<T>> {
    let mut rx = self.events.subscribe();
    loop {
      match rx.recv().await {
        Ok(IncomingMessage::Move { keys, data }) => {
          return Ok(AdapterMove {
            kind: "move".to_string(),
            keys,
            data,
          });
        }
        Ok(_) => {}
        Err(broadcast::error::RecvError::Lagged(_)) => {}
        Err(broadcast::error::RecvError::Closed) => {
          return Err(TriangleError::Adapter("adapter channel closed".to_string()));
        }
      }
    }
  }
}

impl<T> Adapter<T> for AdapterIo<T>
where
  T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
  async fn initialize(&mut self) -> Result<AdapterInfo<T>> {
    self.start().await?;
    self.await_info().await
  }

  async fn config(&mut self, engine: &Engine, data: Option<T>) -> Result<()> {
    self.send(&config_from_engine(engine, data)).await
  }

  async fn update(&mut self, engine: &Engine, data: Option<T>) -> Result<()> {
    self.send(&state_from_engine(engine, data)).await
  }

  async fn add_pieces(&mut self, pieces: Vec<Mino>, data: Option<T>) -> Result<()> {
    self
      .send(&OutgoingPieces {
        kind: "pieces",
        pieces,
        data,
      })
      .await
  }

  async fn play(&mut self, engine: &Engine, data: Option<T>) -> Result<AdapterMove<T>> {
    self.send(&play_from_engine(engine, data)).await?;
    self.await_move().await
  }

  async fn stop(&mut self) -> Result<()> {
    self.dead = true;
    self.stdin = None;
    if let Some(child) = &mut self.child {
      let _ = child.kill().await;
    }
    self.child = None;
    Ok(())
  }
}

pub fn queue_from_snapshot(snapshot: &EngineSnapshot) -> Vec<Mino> {
  snapshot.queue.value.clone()
}
