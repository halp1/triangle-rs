pub mod bag;
pub mod types;

use bag::{Bag, BagSnapshot, BagType, make_bag};
use serde::{Deserialize, Serialize};
pub use types::Mino;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueInitParams {
  pub seed: i64,
  #[serde(rename = "type")]
  pub kind: BagType,
  pub min_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
  pub value: Vec<Mino>,
  pub bag: BagSnapshot,
}

#[derive(Clone)]
pub struct Queue {
  pub seed: i64,
  pub kind: BagType,
  pub bag: Box<dyn Bag>,
  pub min_length: usize,
  pieces: Vec<Mino>,
  repopulate_listener: Option<fn(Vec<Mino>)>,
}

impl std::fmt::Debug for Queue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Queue")
      .field("seed", &self.seed)
      .field("kind", &self.kind)
      .field("min_length", &self.min_length)
      .field("pieces", &self.pieces)
      .finish()
  }
}

impl Queue {
  pub fn new(params: QueueInitParams) -> Self {
    let mut q = Queue {
      seed: params.seed,
      kind: params.kind,
      bag: make_bag(params.kind, params.seed),
      min_length: params.min_length,
      pieces: Vec::new(),
      repopulate_listener: None,
    };
    q.repopulate();
    q
  }

  pub fn reset(&mut self) {
    self.bag = make_bag(self.kind, self.seed);
    self.pieces.clear();
    self.repopulate();
  }

  pub fn clear(&mut self) {
    self.pieces.clear();
  }

  pub fn on_repopulate(&mut self, listener: fn(Vec<Mino>)) {
    self.repopulate_listener = Some(listener);
  }

  pub fn set_min_length(&mut self, min_length: usize) {
    self.min_length = min_length;
    self.repopulate();
  }

  pub fn peek(&self) -> Option<Mino> {
    self.pieces.first().copied()
  }

  pub fn shift(&mut self) -> Option<Mino> {
    self.repopulate();
    if self.pieces.is_empty() {
      return None;
    }
    Some(self.pieces.remove(0))
  }

  pub fn repopulate_once(&mut self) -> Vec<Mino> {
    let new_values = self.bag.next();
    self.pieces.extend_from_slice(&new_values);
    new_values
  }

  fn repopulate(&mut self) {
    let mut added = Vec::new();
    while self.pieces.len() < self.min_length {
      let new_pieces = self.repopulate_once();
      added.extend_from_slice(&new_pieces);
    }
    if let Some(listener) = self.repopulate_listener {
      if !added.is_empty() {
        listener(added);
      }
    }
  }

  pub fn as_slice(&self) -> &[Mino] {
    &self.pieces
  }

  pub fn snapshot(&self) -> QueueSnapshot {
    QueueSnapshot {
      value: self.pieces.clone(),
      bag: self.bag.snapshot(),
    }
  }

  pub fn from_snapshot(&mut self, snapshot: &QueueSnapshot) {
    self.bag.from_snapshot(&snapshot.bag);
    self.pieces = snapshot.value.clone();
  }
}
