pub mod data;

pub use data::{
  CORNER_TABLE_J, CORNER_TABLE_L, CORNER_TABLE_S, CORNER_TABLE_T, CORNER_TABLE_Z, KICK_TABLES,
  KickTable, SPIN_BONUS_RULES,
};

use crate::engine::board::Tile;
use crate::engine::utils::tetromino::types::Rotation;

pub fn legal(blocks: &[(i32, i32)], board: &[Vec<Option<Tile>>]) -> bool {
  if board.is_empty() {
    return false;
  }
  let board_height = board.len() as i32;
  let board_width = board[0].len() as i32;

  for &(x, y) in blocks {
    if x < 0 || x >= board_width {
      return false;
    }
    if y < 0 || y >= board_height {
      return false;
    }
    if board[y as usize][x as usize].is_some() {
      return false;
    }
  }
  true
}

#[derive(Debug, Clone)]
pub struct KickResult {
  pub kick: [i32; 2],
  pub new_location: [f64; 2],
  pub id: String,
  pub index: usize,
}

pub fn perform_kick(
  kick_table_name: &str,
  piece: &str,
  piece_location: [f64; 2],
  ao: [i32; 2],
  max_movement: bool,
  blocks: &[(i32, i32, u8)],
  start_rotation: Rotation,
  end_rotation: Rotation,
  board: &[Vec<Option<Tile>>],
) -> Option<KickResult> {
  let tables = &*KICK_TABLES;
  let table = match tables.get(kick_table_name) {
    Some(t) => t,
    None => return None,
  };

  let floor_y = piece_location[1].floor() as i32;
  let base_x = piece_location[0] as i32 - ao[0];
  let base_y = floor_y - ao[1];

  let initial_blocks: Vec<(i32, i32)> = blocks
    .iter()
    .map(|&(bx, by, _)| (base_x + bx, base_y - by))
    .collect();

  if legal(&initial_blocks, board) {
    return Some(KickResult {
      kick: [0, 0],
      new_location: piece_location,
      id: format!("{}{}", start_rotation, end_rotation),
      index: 0,
    });
  }

  let kick_id = format!("{}{}", start_rotation, end_rotation);
  let kicks = table.get_kicks(piece, &kick_id);

  let mut test_blocks = vec![(0i32, 0i32); blocks.len()];

  for (i, &(dx, dy)) in kicks.iter().enumerate() {
    let new_y = if max_movement {
      piece_location[1] - dy as f64 - ao[1] as f64
    } else {
      piece_location[1].ceil() - 0.1 - dy as f64 - ao[1] as f64
    };

    let floor_new_y = new_y.floor() as i32;
    let moved_base_x = base_x + dx;

    for (j, &(bx, by, _)) in blocks.iter().enumerate() {
      test_blocks[j] = (moved_base_x + bx, floor_new_y - by);
    }

    if legal(&test_blocks, board) {
      return Some(KickResult {
        new_location: [piece_location[0] + dx as f64 - ao[0] as f64, new_y],
        kick: [dx, -dy],
        id: kick_id,
        index: i,
      });
    }
  }

  None
}
