use crate::engine::queue::types::Mino;
use crate::engine::utils::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BagSnapshot {
  pub rng: i64,
  pub id: u64,
  pub extra: Vec<Mino>,
  pub last_generated: Option<Mino>,
}

pub trait Bag: Send + Sync {
  fn next(&mut self) -> Vec<Mino>;
  fn snapshot(&self) -> BagSnapshot;
  fn from_snapshot(&mut self, snapshot: &BagSnapshot);
  fn box_clone(&self) -> Box<dyn Bag>;
}

impl Clone for Box<dyn Bag> {
  fn clone(&self) -> Self {
    self.box_clone()
  }
}

fn standard_pieces() -> [Mino; 7] {
  [
    Mino::Z,
    Mino::L,
    Mino::O,
    Mino::S,
    Mino::I,
    Mino::J,
    Mino::T,
  ]
}

// ─── Bag7 ──────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Bag7 {
  rng: Rng,
  id: u64,
}

impl Bag7 {
  pub fn new(seed: i64) -> Self {
    Bag7 {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for Bag7 {
  fn next(&mut self) -> Vec<Mino> {
    let mut pieces = standard_pieces().to_vec();
    self.rng.shuffle_array(&mut pieces);
    self.id += 1;
    pieces
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Bag14 ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Bag14 {
  rng: Rng,
  id: u64,
}

impl Bag14 {
  pub fn new(seed: i64) -> Self {
    Bag14 {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for Bag14 {
  fn next(&mut self) -> Vec<Mino> {
    let mut pieces: Vec<Mino> = standard_pieces()
      .iter()
      .chain(standard_pieces().iter())
      .cloned()
      .collect();
    self.rng.shuffle_array(&mut pieces);
    self.id += 1;
    pieces
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Classic ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Classic {
  rng: Rng,
  id: u64,
  last_generated: Option<Mino>,
}

impl Classic {
  pub fn new(seed: i64) -> Self {
    Classic {
      rng: Rng::new(seed),
      id: 0,
      last_generated: None,
    }
  }
}

impl Bag for Classic {
  fn next(&mut self) -> Vec<Mino> {
    let pieces = standard_pieces();
    let mut piece = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    if Some(piece) == self.last_generated {
      piece = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    }
    self.last_generated = Some(piece);
    self.id += 1;
    vec![piece]
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: self.last_generated,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
    self.last_generated = s.last_generated;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Pairs ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Pairs {
  rng: Rng,
  id: u64,
}

impl Pairs {
  pub fn new(seed: i64) -> Self {
    Pairs {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for Pairs {
  fn next(&mut self) -> Vec<Mino> {
    let mut shuffled = standard_pieces().to_vec();
    self.rng.shuffle_array(&mut shuffled);
    let selected = &shuffled[0..3];
    let mut result = Vec::with_capacity(6);
    for &p in selected {
      result.push(p);
      result.push(p);
      result.push(p);
    }
    self.rng.shuffle_array(&mut result);
    self.id += 1;
    result
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Random (TotalMayhem) ──────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct TotalMayhem {
  rng: Rng,
  id: u64,
}

impl TotalMayhem {
  pub fn new(seed: i64) -> Self {
    TotalMayhem {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for TotalMayhem {
  fn next(&mut self) -> Vec<Mino> {
    let pieces = standard_pieces();
    let piece = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    self.id += 1;
    vec![piece]
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Bag7Plus1 ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Bag7Plus1 {
  rng: Rng,
  id: u64,
}

impl Bag7Plus1 {
  pub fn new(seed: i64) -> Self {
    Bag7Plus1 {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for Bag7Plus1 {
  fn next(&mut self) -> Vec<Mino> {
    let pieces = standard_pieces();
    let extra = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    let mut bag = standard_pieces().to_vec();
    bag.push(extra);
    self.rng.shuffle_array(&mut bag);
    self.id += 1;
    bag
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Bag7Plus2 ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Bag7Plus2 {
  rng: Rng,
  id: u64,
}

impl Bag7Plus2 {
  pub fn new(seed: i64) -> Self {
    Bag7Plus2 {
      rng: Rng::new(seed),
      id: 0,
    }
  }
}

impl Bag for Bag7Plus2 {
  fn next(&mut self) -> Vec<Mino> {
    let pieces = standard_pieces();
    let extra1 = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    let extra2 = pieces[(self.rng.next_float() * 7.0) as usize % 7];
    let mut bag = standard_pieces().to_vec();
    bag.push(extra1);
    bag.push(extra2);
    self.rng.shuffle_array(&mut bag);
    self.id += 1;
    bag
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: vec![],
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── Bag7PlusX ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct Bag7PlusX {
  rng: Rng,
  id: u64,
  extra: Vec<Mino>,
}

impl Bag7PlusX {
  pub fn new(seed: i64) -> Self {
    Bag7PlusX {
      rng: Rng::new(seed),
      id: 0,
      extra: vec![],
    }
  }
}

impl Bag for Bag7PlusX {
  fn next(&mut self) -> Vec<Mino> {
    const EXTRA_COUNTS: [usize; 4] = [3, 2, 1, 1];
    let extra_count = EXTRA_COUNTS[(self.id as usize) % EXTRA_COUNTS.len()];

    if self.extra.is_empty() {
      let mut pool = standard_pieces().to_vec();
      self.rng.shuffle_array(&mut pool);
      self.extra = pool;
    }

    let mut bag = standard_pieces().to_vec();
    for _ in 0..extra_count {
      if let Some(p) = self.extra.pop() {
        bag.push(p);
      } else {
        let mut pool = standard_pieces().to_vec();
        self.rng.shuffle_array(&mut pool);
        self.extra = pool;
        if let Some(p) = self.extra.pop() {
          bag.push(p);
        }
      }
    }
    self.rng.shuffle_array(&mut bag);
    self.id += 1;
    bag
  }

  fn snapshot(&self) -> BagSnapshot {
    BagSnapshot {
      rng: self.rng.seed(),
      id: self.id,
      extra: self.extra.clone(),
      last_generated: None,
    }
  }

  fn from_snapshot(&mut self, s: &BagSnapshot) {
    self.rng = Rng::new(s.rng);
    self.id = s.id;
    self.extra = s.extra.clone();
  }

  fn box_clone(&self) -> Box<dyn Bag> {
    Box::new(self.clone())
  }
}

// ─── BagType enum and factory ──────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BagType {
  Bag7,
  Bag14,
  Classic,
  Pairs,
  #[serde(rename = "total mayhem")]
  TotalMayhem,
  #[serde(rename = "7+1")]
  Bag7Plus1,
  #[serde(rename = "7+2")]
  Bag7Plus2,
  #[serde(rename = "7+X")]
  Bag7PlusX,
}

pub fn make_bag(bag_type: BagType, seed: i64) -> Box<dyn Bag> {
  match bag_type {
    BagType::Bag7 => Box::new(Bag7::new(seed)),
    BagType::Bag14 => Box::new(Bag14::new(seed)),
    BagType::Classic => Box::new(Classic::new(seed)),
    BagType::Pairs => Box::new(Pairs::new(seed)),
    BagType::TotalMayhem => Box::new(TotalMayhem::new(seed)),
    BagType::Bag7Plus1 => Box::new(Bag7Plus1::new(seed)),
    BagType::Bag7Plus2 => Box::new(Bag7Plus2::new(seed)),
    BagType::Bag7PlusX => Box::new(Bag7PlusX::new(seed)),
  }
}
