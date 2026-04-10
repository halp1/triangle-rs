use crate::engine::queue::types::Mino;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
  pub mino: Mino,
  pub connections: u8,
}

pub const CONN_TOP: u8 = 0b1000;
pub const CONN_RIGHT: u8 = 0b0100;
pub const CONN_BOTTOM: u8 = 0b0010;
pub const CONN_LEFT: u8 = 0b0001;
pub const CONN_CORNER: u8 = 0b1_0000;
pub const CONN_ALL: u8 = CONN_TOP | CONN_RIGHT | CONN_BOTTOM | CONN_LEFT;

#[derive(Debug, Clone)]
pub struct BoardInitParams {
  pub width: usize,
  pub height: usize,
  pub buffer: usize,
}

#[derive(Debug, Clone)]
pub struct InsertGarbageParams {
  pub amount: usize,
  pub size: usize,
  pub column: usize,
  pub bombs: bool,
  pub is_beginning: bool,
  pub is_end: bool,
}

#[derive(Debug, Clone)]
pub struct ClearResult {
  pub lines: usize,
  pub garbage_cleared: usize,
}

#[derive(Debug, Clone)]
pub struct Board {
  pub state: Vec<Vec<Option<Tile>>>,
  pub height: usize,
  pub width: usize,
  pub buffer: usize,
}

impl Board {
  pub fn new(params: BoardInitParams) -> Self {
    let full_height = params.height + params.buffer;
    let state = (0..full_height).map(|_| vec![None; params.width]).collect();
    Board {
      state,
      height: params.height,
      width: params.width,
      buffer: params.buffer,
    }
  }

  pub fn full_height(&self) -> usize {
    self.height + self.buffer
  }

  pub fn occupied(&self, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= self.width as i32 || y >= self.full_height() as i32 {
      return true;
    }
    self.state[y as usize][x as usize].is_some()
  }

  pub fn add(&mut self, blocks: &[(Tile, i32, i32)]) {
    let full_height = self.full_height() as i32;
    let width = self.width as i32;
    for (tile, x, y) in blocks {
      if *y < 0 || *y >= full_height || *x < 0 || *x >= width {
        continue;
      }
      self.state[*y as usize][*x as usize] = Some(tile.clone());
    }
  }

  pub fn clear_lines(&mut self) -> ClearResult {
    let full_height = self.full_height();
    let width = self.width;
    let mut garbage_cleared = 0usize;
    let mut lines: Vec<usize> = Vec::new();

    for idx in 0..full_height {
      let row = &self.state[idx];
      let mut is_full = true;
      let mut has_garbage = false;

      for x in 0..width {
        match &row[x] {
          None => {
            is_full = false;
            break;
          }
          Some(t) if t.mino == Mino::Bomb => {
            is_full = false;
            break;
          }
          Some(t) if t.mino == Mino::Garbage => has_garbage = true,
          _ => {}
        }
      }

      if is_full {
        if idx > 0 {
          let row_above = &mut self.state[idx - 1];
          for x in 0..width {
            if let Some(b) = &mut row_above[x] {
              b.connections |= CONN_TOP;
              if b.connections & CONN_BOTTOM != 0 {
                b.connections &= 0b0_1111;
              }
            }
          }
        }
        if idx + 1 < full_height {
          let row_below = &mut self.state[idx + 1];
          for x in 0..width {
            if let Some(b) = &mut row_below[x] {
              b.connections |= CONN_BOTTOM;
              if b.connections & CONN_TOP != 0 {
                b.connections &= 0b0_1111;
              }
            }
          }
        }
        lines.push(idx);
        if has_garbage {
          garbage_cleared += 1;
        }
      }
    }

    for &line in lines.iter().rev() {
      self.state.remove(line);
      self.state.push(vec![None; self.width]);
    }

    ClearResult {
      lines: lines.len(),
      garbage_cleared,
    }
  }

  pub fn clear_bombs(&mut self, placed_blocks: &[(i32, i32)]) -> ClearResult {
    let full_height = self.full_height();

    let lowest_y = placed_blocks
      .iter()
      .map(|&(_, y)| y)
      .min()
      .unwrap_or(full_height as i32);
    if lowest_y == 0 {
      return ClearResult {
        lines: 0,
        garbage_cleared: 0,
      };
    }

    let bomb_columns: Vec<usize> = placed_blocks
      .iter()
      .filter(|&&(_, y)| y == lowest_y)
      .filter_map(|&(x, _)| {
        let below_y = (lowest_y - 1) as usize;
        if self.state[below_y][x as usize]
          .as_ref()
          .map(|t| t.mino == Mino::Bomb)
          .unwrap_or(false)
        {
          Some(x as usize)
        } else {
          None
        }
      })
      .collect();

    if bomb_columns.is_empty() {
      return ClearResult {
        lines: 0,
        garbage_cleared: 0,
      };
    }

    let mut lines: Vec<usize> = Vec::new();
    let mut cur_y = lowest_y as usize;

    loop {
      if cur_y == 0 {
        break;
      }
      let check_y = cur_y - 1;
      let has_bomb = bomb_columns.iter().any(|&col| {
        self.state[check_y][col]
          .as_ref()
          .map(|t| t.mino == Mino::Bomb)
          .unwrap_or(false)
      });
      if !has_bomb {
        break;
      }
      lines.push(check_y);
      cur_y = check_y;
    }

    if lines.is_empty() {
      return ClearResult {
        lines: 0,
        garbage_cleared: 0,
      };
    }

    let n = lines.len();
    for &line in &lines {
      self.state.remove(line);
      self.state.push(vec![None; self.width]);
    }

    ClearResult {
      lines: n,
      garbage_cleared: n,
    }
  }

  pub fn clear_bombs_and_lines(&mut self, placed_blocks: &[(i32, i32)]) -> ClearResult {
    let bombs = self.clear_bombs(placed_blocks);
    let lines = self.clear_lines();
    ClearResult {
      lines: lines.lines + bombs.lines,
      garbage_cleared: bombs.garbage_cleared + lines.garbage_cleared,
    }
  }

  pub fn perfect_clear(&self) -> bool {
    self.state.iter().all(|row| row.iter().all(|b| b.is_none()))
  }

  pub fn insert_garbage(&mut self, params: InsertGarbageParams) {
    let InsertGarbageParams {
      amount,
      size,
      column,
      bombs,
      is_beginning,
      is_end,
    } = params;
    let width = self.width;
    let full_height = self.full_height();

    let new_rows: Vec<Vec<Option<Tile>>> = (0..amount)
      .map(|y| {
        (0..width)
          .map(|x| {
            if x >= column && x < column + size {
              if bombs {
                Some(Tile {
                  mino: Mino::Bomb,
                  connections: 0,
                })
              } else {
                None
              }
            } else {
              let mut connection = 0u8;
              if is_end && y == 0 {
                connection |= CONN_BOTTOM;
              }
              if is_beginning && y == amount - 1 {
                connection |= CONN_TOP;
              }
              if x == 0 {
                connection |= CONN_LEFT;
              }
              if x == width - 1 {
                connection |= CONN_RIGHT;
              }
              if column > 0 && x == column - 1 {
                connection |= CONN_RIGHT;
              }
              if x == column + size {
                connection |= CONN_LEFT;
              }
              Some(Tile {
                mino: Mino::Garbage,
                connections: connection,
              })
            }
          })
          .collect()
      })
      .collect();

    let mut old_state = std::mem::take(&mut self.state);
    let mut combined = new_rows;
    combined.append(&mut old_state);

    let remove_start = full_height.saturating_sub(amount + 1);
    combined.drain(remove_start..remove_start + amount);

    self.state = combined;
  }

  pub fn reset(&mut self) {
    let full_height = self.full_height();
    self.state = (0..full_height).map(|_| vec![None; self.width]).collect();
  }
}
