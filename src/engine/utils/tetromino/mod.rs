pub mod data;
pub mod types;

use crate::engine::board::Tile;
use crate::engine::queue::types::Mino;
use crate::engine::utils::kicks::perform_kick;
use data::TETROMINOES;
pub use data::{MatrixData, PreviewData, TetrominoEntry};
pub use types::{Block, ROT_0, ROT_1, ROT_2, ROT_3, Rotation};

#[derive(Debug, Clone)]
pub struct TetrominoSnapshot {
  pub symbol: Mino,
  pub location: [f64; 2],
  pub locking: f64,
  pub lock_resets: i32,
  pub rot_resets: i32,
  pub safe_lock: i32,
  pub highest_y: f64,
  pub rotation: Rotation,
  pub falling_rotations: i32,
  pub total_rotations: i32,
  pub irs: i32,
  pub ihs: bool,
  pub aox: i32,
  pub aoy: i32,
  pub keys: i32,
}

#[derive(Debug, Clone)]
pub struct TetrominoInitParams {
  pub symbol: Mino,
  pub initial_rotation: Rotation,
  pub board_height: i32,
  pub board_width: i32,
  pub from: Option<TetrominoSnapshot>,
}

#[derive(Debug, Clone)]
pub struct Tetromino {
  rotation: Rotation,
  pub symbol: Mino,
  pub states: Vec<Vec<Block>>,
  pub location: [f64; 2],

  pub locking: f64,
  pub lock_resets: i32,
  pub rot_resets: i32,
  pub safe_lock: i32,
  pub highest_y: f64,
  pub falling_rotations: i32,
  pub total_rotations: i32,
  pub irs: i32,
  pub ihs: bool,
  pub aox: i32,
  pub aoy: i32,
  pub keys: i32,
}

impl Tetromino {
  pub fn new(params: TetrominoInitParams) -> Self {
    let symbol_lower = params.symbol.as_str().to_ascii_lowercase();
    let tetromino = TETROMINOES
      .get(symbol_lower.as_str())
      .expect("unknown mino type");

    let states: Vec<Vec<Block>> = tetromino.matrix.data.iter().map(|r| r.clone()).collect();

    let start_x = (params.board_width as f64 / 2.0 - tetromino.matrix.w as f64 / 2.0).floor();
    let start_y = params.board_height as f64 + 2.04;

    Tetromino {
      rotation: params.initial_rotation,
      symbol: params.symbol,
      states,
      location: [start_x, start_y],
      locking: 0.0,
      lock_resets: 0,
      rot_resets: 0,
      safe_lock: params.from.as_ref().map(|f| f.safe_lock).unwrap_or(0),
      highest_y: params.board_height as f64 + 2.0,
      falling_rotations: 0,
      total_rotations: 0,
      irs: params.from.as_ref().map(|f| f.irs).unwrap_or(0),
      ihs: params.from.as_ref().map(|f| f.ihs).unwrap_or(false),
      aox: 0,
      aoy: 0,
      keys: 0,
    }
  }

  pub fn rotation(&self) -> Rotation {
    self.rotation % 4
  }

  pub fn set_rotation(&mut self, value: i32) {
    self.rotation = (((value % 4) + 4) % 4) as Rotation;
  }

  pub fn x(&self) -> i32 {
    self.location[0] as i32
  }

  pub fn set_x(&mut self, value: i32) {
    self.location[0] = value as f64;
  }

  pub fn y(&self) -> i32 {
    self.location[1].floor() as i32
  }

  pub fn set_y(&mut self, value: f64) {
    self.location[1] = value;
  }

  pub fn blocks(&self) -> &[Block] {
    let rot = (self.rotation as usize).min(self.states.len().saturating_sub(1));
    &self.states[rot]
  }

  pub fn absolute_blocks(&self) -> Vec<(i32, i32)> {
    let blocks = self.blocks();
    let x = self.x();
    let y = self.y();
    blocks
      .iter()
      .map(|&(bx, by, _)| (bx + x, -by + y))
      .collect()
  }

  pub fn absolute_at(
    &self,
    x: Option<f64>,
    y: Option<f64>,
    rotation: Option<Rotation>,
  ) -> Vec<(i32, i32)> {
    let px = x.unwrap_or(self.location[0]) as i32;
    let py = (y.unwrap_or(self.location[1])).floor() as i32;
    let rot = {
      let r = rotation.unwrap_or(self.rotation) as i32;
      (((r % 4) + 4) % 4) as usize
    };
    let state = &self.states[rot.min(self.states.len().saturating_sub(1))];
    state
      .iter()
      .map(|&(bx, by, _)| (bx + px, -by + py))
      .collect()
  }

  fn legal_at(&self, board: &[Vec<Option<Tile>>], x: i32, y: i32) -> bool {
    let blocks: Vec<(i32, i32)> = self
      .blocks()
      .iter()
      .map(|&(bx, by, _)| (bx + x, -by + y))
      .collect();
    super::kicks::legal(&blocks, board)
  }

  pub fn is_stupid_spin_position(&self, board: &[Vec<Option<Tile>>]) -> bool {
    !self.legal_at(board, self.x(), self.y() - 1)
  }

  pub fn is_all_spin_position(&self, board: &[Vec<Option<Tile>>]) -> bool {
    !self.legal_at(board, self.x() - 1, self.y())
      && !self.legal_at(board, self.x() + 1, self.y())
      && !self.legal_at(board, self.x(), self.y() + 1)
      && !self.legal_at(board, self.x(), self.y() - 1)
  }

  pub fn move_right(&mut self, board: &[Vec<Option<Tile>>]) -> bool {
    if self.legal_at(board, self.x() + 1, self.y()) {
      self.location[0] += 1.0;
      return true;
    }
    false
  }

  pub fn move_left(&mut self, board: &[Vec<Option<Tile>>]) -> bool {
    if self.legal_at(board, self.x() - 1, self.y()) {
      self.location[0] -= 1.0;
      return true;
    }
    false
  }

  pub fn das_right(&mut self, board: &[Vec<Option<Tile>>]) -> bool {
    if self.move_right(board) {
      while self.move_right(board) {}
      return true;
    }
    false
  }

  pub fn das_left(&mut self, board: &[Vec<Option<Tile>>]) -> bool {
    if self.move_left(board) {
      while self.move_left(board) {}
      return true;
    }
    false
  }

  pub fn soft_drop(&mut self, board: &[Vec<Option<Tile>>]) -> bool {
    let start = self.location[1];
    while self.legal_at(board, self.x(), self.y() - 1) {
      self.location[1] -= 1.0;
    }
    start != self.location[1]
  }

  pub fn snapshot(&self) -> TetrominoSnapshot {
    TetrominoSnapshot {
      symbol: self.symbol,
      location: self.location,
      locking: self.locking,
      lock_resets: self.lock_resets,
      rot_resets: self.rot_resets,
      safe_lock: self.safe_lock,
      highest_y: self.highest_y,
      rotation: self.rotation(),
      falling_rotations: self.falling_rotations,
      total_rotations: self.total_rotations,
      irs: self.irs,
      ihs: self.ihs,
      aox: self.aox,
      aoy: self.aoy,
      keys: self.keys,
    }
  }

  pub fn from_snapshot(snapshot: &TetrominoSnapshot, board_height: i32, board_width: i32) -> Self {
    let mut t = Tetromino::new(TetrominoInitParams {
      symbol: snapshot.symbol,
      initial_rotation: snapshot.rotation,
      board_height,
      board_width,
      from: Some(snapshot.clone()),
    });
    t.location = snapshot.location;
    t.locking = snapshot.locking;
    t.lock_resets = snapshot.lock_resets;
    t.rot_resets = snapshot.rot_resets;
    t.safe_lock = snapshot.safe_lock;
    t.highest_y = snapshot.highest_y;
    t.rotation = snapshot.rotation;
    t.falling_rotations = snapshot.falling_rotations;
    t.total_rotations = snapshot.total_rotations;
    t.irs = snapshot.irs;
    t.ihs = snapshot.ihs;
    t.aox = snapshot.aox;
    t.aoy = snapshot.aoy;
    t.keys = snapshot.keys;
    t
  }
}
