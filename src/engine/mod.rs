pub mod board;
pub mod constants;
pub mod events;
pub mod garbage;
pub mod multiplayer;
pub mod queue;
pub mod utils;

use board::{Board, BoardInitParams, InsertGarbageParams, Tile};
use constants::*;
use garbage::{GarbageQueue, GarbageQueueInitParams, IncomingGarbage, OutgoingGarbage};
use multiplayer::IgeHandler;
use queue::types::Mino;
use queue::{Queue, QueueInitParams, QueueSnapshot};
use utils::{
  damage_calc::{GarbageCalcConfig, GarbageCalcInput, garbage_calc_v2},
  increase::IncreaseTracker,
  kicks::{legal, perform_kick},
  tetromino::{Tetromino, TetrominoInitParams, TetrominoSnapshot},
};

use serde::{Deserialize, Serialize};

use crate::classes::ribbon::Hook;
use crate::engine::utils::KickTable;
use crate::engine::utils::tetromino::data::MinoExt;
use crate::types::game::{
  Buffering, ComboTable, GarbageBlocking, GarbageTargetBonus, Handling, Key, Passthrough, Spin,
  SpinBonuses, ige, replay,
};
use crate::utils::EventEmitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOptions {
  pub spin_bonuses: SpinBonuses,
  pub combo_table: ComboTable,
  pub garbage_target_bonus: GarbageTargetBonus,
  pub clutch: bool,
  pub garbage_blocking: GarbageBlocking,
  pub stock: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcOptions {
  pub garbage: f64,
  pub b2b: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct B2bOptions {
  pub chaining: bool,
  pub charging: Option<B2bCharging>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct B2bCharging {
  pub at: u64,
  pub base: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovementOptions {
  pub infinite: bool,
  pub lock_resets: u64,
  pub lock_time: f64,
  pub may_20g: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedOptions {
  pub spin180: bool,
  pub hard_drop: bool,
  pub hold: bool,
  pub undo: bool,
  pub retry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiscOptions {
  pub movement: MovementOptions,
  pub allowed: AllowedOptions,
  pub infinite_hold: bool,
  pub stride: bool,
  pub username: Option<String>,
  pub date: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncreasableValue {
  pub value: f64,
  pub increase: f64,
  pub margin_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiplayerOptions {
  pub opponents: Vec<u64>,
  pub passthrough: Passthrough,
}

#[derive(Debug, Clone)]
pub struct EngineInitParams {
  pub queue: QueueInitParams,
  pub board: BoardInitParams,
  pub kick_table: KickTable,
  pub options: GameOptions,
  pub gravity: IncreasableValue,
  pub garbage: GarbageQueueInitParams,
  pub handling: Handling,
  pub pc: Option<PcOptions>,
  pub b2b: B2bOptions,
  pub multiplayer: Option<MultiplayerOptions>,
  pub misc: MiscOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
  pub garbage_sent: u64,
  pub garbage_attack: u64,
  pub garbage_receive: u64,
  pub garbage_cleared: u64,
  pub combo: i32,
  pub b2b: i32,
  pub pieces: u32,
  pub lines: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftState {
  pub held: bool,
  pub arr: f64,
  pub das: f64,
  pub dir: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputKeys {
  pub soft_drop: bool,
  pub rotate_ccw: bool,
  pub rotate_cw: bool,
  pub rotate_180: bool,
  pub hold: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTime {
  pub start: f64,
  pub zero: bool,
  pub locked: bool,
  pub prev: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputState {
  pub l_shift: ShiftState,
  pub r_shift: ShiftState,
  pub last_shift: i8,
  pub keys: InputKeys,
  pub first_input_time: f64,
  pub time: InputTime,
  pub last_piece_time: f64,
}

#[derive(Debug, Clone)]
pub struct DynamicState {
  pub gravity: Option<IncreaseTracker>,
  pub garbage_multiplier: Option<IncreaseTracker>,
  pub garbage_cap: Option<IncreaseTracker>,
}

#[derive(Debug, Clone)]
pub struct MultiplierState {
  pub options: MultiplayerOptions,
  pub targets: Vec<u64>,
  pub passthrough_network: bool,
  pub passthrough_replay: bool,
  pub passthrough_travel: bool,
}

#[derive(Debug, Clone)]
pub struct PracticeState {
  pub undo: Vec<EngineSnapshot>,
  pub redo: Vec<EngineSnapshot>,
  pub retry: bool,
  pub retry_iter: u32,
  pub last_piece: Option<Box<EngineSnapshot>>,
}

#[derive(Debug, Clone)]
pub struct SpikeState {
  pub count: u32,
  pub timer: u32,
}

#[derive(Debug, Clone)]
pub struct ResCache {
  pub pieces: u32,
  pub garbage_sent: Vec<u32>,
  pub garbage_received: Vec<OutgoingGarbage>,
  pub keys: Vec<String>,
  pub last_lock: f64,
}

#[derive(Debug, Clone)]
pub struct EngineSnapshot {
  pub is_undo_redo: bool,
  pub board: Vec<Vec<Option<Tile>>>,
  pub falling: TetrominoSnapshot,
  pub frame: u64,
  pub garbage: crate::engine::garbage::GarbageQueueSnapshot,
  pub hold: Option<Mino>,
  pub hold_locked: bool,
  pub last_spin: Option<Spin>,
  pub last_was_clear: bool,
  pub queue: QueueSnapshot,
  pub __internal_queue: QueueSnapshot,
  pub input: InputState,
  pub subframe: f64,
  pub targets: Option<Vec<u64>>,
  pub stats: EngineStats,
  pub glock: f64,
  pub stock: u64,
  pub state: u32,
  pub spike: SpikeState,
  pub time_frame_offset: u64,
  pub res_cache: ResCache,
  pub practice: PracticeState,
  pub ige: multiplayer::IgeHandlerSnapshot,
}

#[derive(Debug, Clone)]
pub struct Engine {
  pub queue: Queue,
  __internal_queue: Queue,

  pub held: Option<Mino>,
  pub hold_locked: bool,

  pub falling: Tetromino,

  kick_table: KickTable,

  undo_hook: Hook,

  pub board: Board,

  pub last_spin: Option<Spin>,
  pub last_was_clear: bool,

  pub stats: EngineStats,
  pub game_options: GameOptions,
  pub garbage_queue: GarbageQueue,
  pub frame: u64,
  pub subframe: f64,
  pub initializer: EngineInitParams,
  pub handling: Handling,
  pub input: InputState,
  pub pc: Option<PcOptions>,
  pub b2b: B2bOptions,
  pub dynamic: (IncreaseTracker, IncreaseTracker, IncreaseTracker),
  pub glock: f64,
  pub multiplayer: Option<MultiplierState>,
  pub ige_handler: IgeHandler,
  pub misc: MiscOptions,
  pub state: u32,
  pub stock: u64,
  pub practice: PracticeState,
  pub time_frame_offset: u64,
  pub spike: SpikeState,
  pub res_cache: ResCache,
  pub events: EventEmitter,
}

impl<'de> Deserialize<'de> for Engine {
  fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    unimplemented!("Engine cannot be deserialized")
  }
}

impl Serialize for Engine {
  fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    unimplemented!("Engine cannot be serialized")
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockResult {
  pub mino: Mino,
  pub garbage_cleared: u32,
  pub lines: u32,
  pub spin: Spin,
  pub raw_garbage: Vec<u32>,
  pub garbage: Vec<u32>,
  pub surge: u32,
  pub stats: EngineStats,
  pub garbage_added: Option<Vec<OutgoingGarbage>>,
  pub topout: bool,
  pub piece_time: f64,
  pub key_presses: Vec<String>,
}

impl Engine {
  pub fn new(params: EngineInitParams) -> Self {
    let events = EventEmitter::new();
    let mut e = Engine {
      queue: Queue::new(params.queue.clone()),
      __internal_queue: Queue::new(params.queue.clone()),
      events: events.clone(),
      undo_hook: events.hook(),
      held: None,
      hold_locked: false,
      falling: Tetromino::new(TetrominoInitParams {
        symbol: Mino::T,
        initial_rotation: 0,
        board_height: params.board.height,
        board_width: params.board.width,
        from: None,
      }),
      kick_table: params.kick_table.clone(),
      board: Board::new(params.board.clone()),
      last_spin: None,
      last_was_clear: false,
      stats: EngineStats {
        garbage_sent: 0,
        garbage_attack: 0,
        garbage_receive: 0,
        garbage_cleared: 0,
        combo: -1,
        b2b: -1,
        pieces: 0,
        lines: 0,
      },
      game_options: params.options.clone(),
      garbage_queue: GarbageQueue::new(params.garbage.clone()),
      frame: 0,
      subframe: 0.0,
      initializer: params.clone(),
      handling: params.handling.clone(),
      input: InputState {
        l_shift: ShiftState {
          held: false,
          arr: 0.0,
          das: 0.0,
          dir: -1,
        },
        r_shift: ShiftState {
          held: false,
          arr: 0.0,
          das: 0.0,
          dir: 1,
        },
        last_shift: -1,
        keys: InputKeys {
          soft_drop: false,
          rotate_ccw: false,
          rotate_cw: false,
          rotate_180: false,
          hold: false,
        },
        first_input_time: -1.0,
        time: InputTime {
          start: 0.0,
          zero: true,
          locked: false,
          prev: 0.0,
        },
        last_piece_time: 0.0,
      },
      pc: params.pc.clone(),
      b2b: params.b2b.clone(),
      dynamic: (
        IncreaseTracker::new(
          params.gravity.value,
          params.gravity.increase,
          params.gravity.margin_time as u32,
        ),
        IncreaseTracker::new(
          params.garbage.multiplier.value,
          params.garbage.multiplier.increase,
          params.garbage.multiplier.margin_time as u32,
        ),
        IncreaseTracker::new(
          params.garbage.cap.value,
          params.garbage.cap.increase,
          params.garbage.cap.margin_time as u32,
        ),
      ),
      glock: 0.0,
      multiplayer: params.multiplayer.as_ref().map(|mp| MultiplierState {
        passthrough_network: [Passthrough::Consistent, Passthrough::Zero].contains(&mp.passthrough),
        passthrough_replay: mp.passthrough != Passthrough::Full,
        passthrough_travel: [Passthrough::Zero, Passthrough::Limited].contains(&mp.passthrough),
        options: mp.clone(),
        targets: Vec::new(),
      }),
      ige_handler: IgeHandler::new(
        params
          .multiplayer
          .as_ref()
          .map(|mp| mp.opponents.clone())
          .unwrap_or_default(),
      ),
      misc: params.misc.clone(),
      state: 0,
      stock: params.options.stock,
      practice: PracticeState {
        undo: Vec::new(),
        redo: Vec::new(),
        retry: false,
        retry_iter: 0,
        last_piece: None,
      },
      time_frame_offset: 0,
      spike: SpikeState { count: 0, timer: 0 },
      res_cache: ResCache {
        pieces: 0,
        garbage_sent: Vec::new(),
        garbage_received: Vec::new(),
        keys: Vec::new(),
        last_lock: 0.0,
      },
    };
    e.__internal_queue.set_min_length(14);
    e.next_piece(false, false);
    e
  }

  pub fn reset(&mut self) {
    let params = self.initializer.clone();
    *self = Engine::new(params);
  }

  fn flush_res(&mut self) -> ResCache {
    let res = ResCache {
      pieces: self.res_cache.pieces,
      garbage_sent: self.res_cache.garbage_sent.clone(),
      garbage_received: self.res_cache.garbage_received.clone(),
      keys: self.res_cache.keys.drain(..).collect(),
      last_lock: self.res_cache.last_lock,
    };
    self.res_cache.pieces = 0;
    self.res_cache.garbage_sent.clear();
    self.res_cache.garbage_received.clear();
    res
  }

  fn effective_gravity(&self) -> f64 {
    if self.glock <= 0.0 {
      self.dynamic.0.get()
    } else if self.glock <= 180.0 {
      (1.0 - self.glock / 180.0).powi(2) * self.dynamic.0.get()
    } else {
      0.0
    }
  }

  fn has_hit_wall(&self) -> bool {
    self.state & STATE_WALL != 0
  }
  fn is_sleep(&self) -> bool {
    self.state & STATE_SLEEP != 0
  }
  #[allow(dead_code)]
  fn set_sleep(&mut self, v: bool) {
    if v {
      self.state |= STATE_SLEEP;
    } else {
      self.state &= !STATE_SLEEP;
    }
  }
  fn is_forced_to_lock(&self) -> bool {
    self.state & ACTION_FORCELOCK != 0
  }

  fn is_20g(&self) -> bool {
    let is_20g = self.dynamic.0.get() > self.board.height as f64;
    let mode_20g = self.misc.movement.may_20g;
    if self.input.keys.soft_drop {
      let prefer_soft = self.handling.may20g || (is_20g && mode_20g);
      return (self.handling.sdf == 41.0
        || self.dynamic.0.get() * self.handling.sdf > self.board.height as f64)
        && prefer_soft;
    }
    is_20g && mode_20g
  }

  fn should_lock(&self) -> bool {
    !self.misc.movement.infinite
      && self.falling.lock_resets >= self.misc.movement.lock_resets as u32
  }

  fn should_fall_faster(&self) -> bool {
    if self.misc.movement.infinite {
      return false;
    }
    self.falling.rot_resets > self.misc.movement.lock_resets as u32 + 15
  }

  fn internal_lock_check(&mut self, subframe: f64) -> bool {
    self.falling.locking += subframe;
    self.falling.locking > self.misc.movement.lock_time
      || self.is_forced_to_lock()
      || self.should_lock()
  }

  fn internal_fall(&mut self, value: f64) -> bool {
    if self.falling.safe_lock > 0 {
      self.falling.safe_lock -= 1;
    }

    let y = self.falling.location[1];
    let y1_raw = (y - value) * 1e6;
    let mut y1 = y1_raw.round() / 1e6;
    let mut y2 = y - 1.0;

    if y1 % 1.0 == 0.0 {
      y1 -= 1e-6;
    }
    if y2 % 1.0 == 0.0 {
      y2 += 2e-6;
    }

    let blocks1 = self.falling.absolute_at(None, Some(y1), None);
    let blocks2 = self.falling.absolute_at(None, Some(y2), None);

    if !legal(&blocks1, &self.board.state) || !legal(&blocks2, &self.board.state) {
      return false;
    }

    let highest_y = self.falling.highest_y;
    if highest_y > y1 {
      self.falling.highest_y = y1.floor();
    }
    self.falling.location[1] = y1;
    if self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    self.state &= !STATE_FLOOR;

    if y1 < highest_y || self.misc.movement.infinite {
      self.falling.lock_resets = 0;
      self.falling.rot_resets = 0;
    }

    true
  }

  fn slam_to_floor(&mut self) {
    while self.internal_fall(1.0) {}
  }

  pub fn topped_out(&self) -> bool {
    for block in self.falling.absolute_blocks() {
      if block.1 < 0 {
        return true;
      }
      if let Some(row) = self.board.state.get(block.1 as usize) {
        if let Some(cell) = row.get(block.0 as usize) {
          if cell.is_some() {
            return true;
          }
        } else {
          return true;
        }
      } else {
        return true;
      }
    }
    false
  }

  fn dcd(&mut self) {
    if !self.has_hit_wall() || self.handling.dcd == 0.0 {
      return;
    }
    let das = self.handling.das;
    let dcd = self.handling.dcd;
    let arr = self.handling.arr;
    self.input.l_shift.das = self.input.l_shift.das.min(das - dcd);
    self.input.l_shift.arr = arr;
    self.input.r_shift.das = self.input.r_shift.das.min(das - dcd);
    self.input.r_shift.arr = arr;
  }

  fn fall_step(&mut self, subframe: f64) {
    if self.is_sleep() {
      return;
    }

    let mut fall = self.effective_gravity() * subframe;

    if self.glock > 0.0 {
      self.glock -= subframe;
    }
    if self.glock < 0.0 {
      self.glock = 0.0;
    }

    if self.input.keys.soft_drop {
      if self.handling.sdf == 41.0 {
        fall = 400.0 * subframe;
      } else {
        fall *= self.handling.sdf;
        fall = fall.max(0.05 * self.handling.sdf);
      }
    }

    if self.should_lock() {
      let check = self
        .falling
        .absolute_at(None, Some(self.falling.location[1] - 1.0), None);
      if !legal(&check, &self.board.state) {
        fall = 20.0;
        self.state |= ACTION_FORCELOCK;
      }
    }

    if self.should_fall_faster() {
      fall += 0.5
        * subframe
        * (self
          .falling
          .rot_resets
          .saturating_sub(self.misc.movement.lock_resets as u32 + 15)) as f64;
    }

    let mut drop_factor = fall;
    while drop_factor > 0.0 {
      let step = drop_factor.min(1.0);
      let y_before = self.falling.location[1];

      if !self.internal_fall(step) {
        let sub = subframe;
        if self.internal_lock_check(sub) {
          if self.handling.safelock {
            self.falling.safe_lock = 7;
          }
          self.lock(false);
        }
        return;
      }

      if y_before.floor() != self.falling.location[1].floor() {
        self.state &= !ROTATION_ALL;
      }

      drop_factor -= step;
    }
  }

  fn clamp_rotation(&self, amount: i32) -> u8 {
    ((self.falling.rotation() as i32 + amount).rem_euclid(4)) as u8
  }

  fn detect_tspin_kick(&self, kick_id: &str, kick: &[i32; 2]) -> bool {
    ((kick_id == "23" || kick_id == "03") && kick[0] == 1 && kick[1] == -2)
      || ((kick_id == "21" || kick_id == "01") && kick[0] == -1 && kick[1] == -2)
  }

  fn perform_rotation_kick(&self, to: u8) -> Option<crate::engine::utils::kicks::KickResult> {
    let falling = &self.falling;
    let blocks: Vec<_> = falling.states[to as usize % falling.states.len()]
      .iter()
      .map(|&(bx, by, c)| (bx, by, c))
      .collect();
    perform_kick(
      self.kick_table,
      falling.symbol,
      falling.location,
      [falling.aox, falling.aoy],
      !self.misc.movement.infinite
        && falling.total_rotations > self.misc.movement.lock_resets as u32 + 15,
      &blocks,
      falling.rotation(),
      to,
      &self.board.state,
    )
  }

  fn internal_rotate(
    &mut self,
    new_x: f64,
    new_y: f64,
    new_rotation: u8,
    rotation_direction: i32,
    kick_id: &str,
    kick: &[i32; 2],
  ) -> bool {
    let is_180 = rotation_direction.abs() >= 2;
    let dir = if is_180 {
      if new_rotation as i32 > self.falling.rotation() as i32 {
        1i32
      } else {
        -1
      }
    } else {
      rotation_direction
    };

    if is_180 {
      self.state |= ROTATION_180;
    }
    self.falling.location[0] = new_x;
    self.falling.location[1] = new_y;
    self.falling.set_rotation(new_rotation as i32);

    if dir == 1 {
      self.state |= ROTATION_RIGHT;
    } else {
      self.state |= ROTATION_LEFT;
    }
    self.state &= !(ROTATION_SPIN | ROTATION_MINI | ROTATION_SPIN_ALL);

    if self.falling.lock_resets < 31 {
      self.falling.lock_resets += 1;
    }
    if self.falling.rot_resets < 63 {
      self.falling.rot_resets += 1;
    }

    let fin_or_tst = self.detect_tspin_kick(kick_id, kick);
    let spin = self.detect_spin(fin_or_tst);
    self.last_spin = Some(spin);

    if spin != Spin::None {
      self.state |= ROTATION_SPIN;
      if spin == Spin::Mini {
        self.state |= ROTATION_MINI;
      }
    }

    self.falling.total_rotations += 1;
    self.dcd();
    if !self.should_lock() {
      self.falling.locking = 0.0;
    }
    true
  }

  fn rotate(&mut self, amount: i32, _is_irs: bool) -> bool {
    if self.is_sleep() {
      if self.handling.irs == Buffering::Tap {
        self.falling.irs = (self.falling.irs as i32 + amount).rem_euclid(4) as i8;
      }
      return false;
    }

    let to = self.clamp_rotation(amount);
    self.state |= ACTION_MOVE | ACTION_ROTATE;

    if let Some(kick) = self.perform_rotation_kick(to) {
      self.internal_rotate(
        kick.new_location[0],
        kick.new_location[1],
        to,
        amount,
        &kick.id.clone(),
        &kick.kick,
      );
      return true;
    }
    false
  }

  fn consider_blockout(&mut self, _is_silent: bool) -> bool {
    let abs = self.falling.absolute_blocks();
    if legal(&abs, &self.board.state) {
      return false;
    }

    let mut clutched = false;

    if self.last_was_clear && self.game_options.clutch {
      let orig_y = self.falling.location[1];
      let orig_hy = self.falling.highest_y;

      while self.falling.y() < self.board.full_height() as i32 {
        self.falling.location[1] += 1.0;
        self.falling.highest_y += 1.0;
        let abs = self.falling.absolute_blocks();
        if legal(&abs, &self.board.state) {
          clutched = true;
          break;
        }
      }

      if !clutched {
        self.falling.location[1] = orig_y;
        self.falling.highest_y = orig_hy;
      }
    }

    !clutched
  }

  pub fn initiate_piece(&mut self, piece: Mino, ignore_blockout: bool, is_hold: bool) {
    if self.handling.irs == Buffering::Hold {
      let mut rotation_state: i8 = 0;
      if self.input.keys.rotate_ccw {
        rotation_state = rotation_state.wrapping_sub(1);
      }
      if self.input.keys.rotate_cw {
        rotation_state = rotation_state.wrapping_add(1);
      }
      if self.input.keys.rotate_180 {
        rotation_state = rotation_state.wrapping_add(2);
      }
      self.falling.irs = rotation_state.rem_euclid(4) as i8;
    }

    if self.handling.ihs == Buffering::Hold && self.input.keys.hold && !is_hold {
      self.state |= ACTION_IHS;
    }

    self.dcd();
    self.state &= !(ROTATION_ALL
      | STATE_ALL
      | ACTION_FORCELOCK
      | ACTION_SOFTDROP
      | ACTION_MOVE
      | ACTION_ROTATE);
    self.input.first_input_time = -1.0;

    if !is_hold {
      self.hold_locked = false;
    }

    let spawn_rot = self
      .kick_table
      .data()
      .spawn_rotation
      .get(&piece.into())
      .copied()
      .unwrap_or(0);

    let previous = self.falling.snapshot();

    self.falling = Tetromino::new(TetrominoInitParams {
      symbol: piece,
      initial_rotation: spawn_rot,
      board_height: self.board.height,
      board_width: self.board.width,
      from: Some(previous),
    });

    if !ignore_blockout && self.consider_blockout(is_hold) {
      self.state &= !ACTION_IHS;
      self.falling.irs = 0;
    } else {
      if self.state & ACTION_IHS != 0 {
        self.state &= !ACTION_IHS;
        self.hold(false, ignore_blockout);
      } else {
        if self.falling.irs != 0 {
          self.rotate(self.falling.irs as i32, true);
          self.falling.irs = 0;
        }
        if !self.consider_blockout(!ignore_blockout || is_hold) && self.is_20g() {
          self.slam_to_floor();
        }
      }
    }

    self.events.emit(events::falling::New { piece, is_hold });
  }

  pub fn next_piece(&mut self, ignore_blockout: bool, is_hold: bool) {
    let piece = self.queue.shift().expect("queue is empty");
    self.__internal_queue.shift();
    self.initiate_piece(piece, ignore_blockout, is_hold);
  }

  pub fn hold(&mut self, _ihs: bool, ignore_blockout: bool) -> bool {
    if self.is_sleep() {
      if self.handling.ihs == Buffering::Tap {
        self.state |= ACTION_IHS;
      }
      return false;
    }
    if self.hold_locked || !self.misc.allowed.hold {
      return false;
    }
    self.hold_locked = !self.misc.infinite_hold;
    if let Some(save) = self.held {
      let current = self.falling.symbol;
      self.held = Some(current);
      self.initiate_piece(save, ignore_blockout, true);
    } else {
      self.held = Some(self.falling.symbol);
      self.next_piece(ignore_blockout, true);
    }
    self.hold_locked = !self.misc.infinite_hold;
    true
  }

  fn connect_blocks(&self, blocks: &[(i32, i32)]) -> Vec<(i32, i32, u8)> {
    let exists = |x: i32, y: i32| blocks.iter().any(|&(bx, by)| bx == x && by == y);

    blocks
      .iter()
      .map(|&(x, y)| {
        let mut s = 0u8;
        if !exists(x, y - 1) {
          s |= 0b1000;
        }
        if !exists(x + 1, y) {
          s |= 0b0100;
        }
        if !exists(x, y + 1) {
          s |= 0b0010;
        }
        if !exists(x - 1, y) {
          s |= 0b0001;
        }

        let has_corner = match s {
          v if v == (0b1000 | 0b0001) => !exists(x + 1, y + 1),
          v if v == (0b1000 | 0b0100) => !exists(x - 1, y + 1),
          v if v == (0b0010 | 0b0001) => !exists(x + 1, y - 1),
          v if v == (0b0010 | 0b0100) => !exists(x - 1, y - 1),
          0b0010 => !exists(x + 1, y - 1) && !exists(x - 1, y - 1),
          0b0001 => !exists(x + 1, y - 1) && !exists(x + 1, y + 1),
          0b1000 => !exists(x - 1, y + 1) && !exists(x + 1, y + 1),
          0b0100 => !exists(x - 1, y - 1) && !exists(x - 1, y + 1),
          _ => false,
        };
        if has_corner {
          s |= 0b1_0000;
        }

        (x, y, s)
      })
      .collect()
  }

  fn detect_spin_from_corners(&self, fin_or_tst: bool) -> Spin {
    use utils::kicks::{
      CORNER_TABLE_J, CORNER_TABLE_L, CORNER_TABLE_S, CORNER_TABLE_T, CORNER_TABLE_Z,
    };

    let blocks = self.falling.blocks();
    let abs: Vec<(i32, i32)> = blocks
      .iter()
      .map(|&(bx, by, _)| (bx + self.falling.x(), -by + self.falling.y() - 1))
      .collect();

    if legal(&abs, &self.board.state) {
      return Spin::None;
    }

    let symbol = self.falling.symbol.as_str().to_ascii_lowercase();
    let rotation = self.falling.rotation() as usize;

    let mut corners = 0;
    let mut front_corners = 0;

    let check_t = symbol == "t";
    let is_z = symbol == "z";
    let is_l = symbol == "l";
    let is_s = symbol == "s";
    let is_j = symbol == "j";

    if check_t {
      let table = CORNER_TABLE_T[rotation];
      for (cx, cy, idx1, idx2) in table {
        if self
          .board
          .occupied(self.falling.x() + cx + 1, self.falling.y() - cy - 1)
        {
          corners += 1;
          let rot = rotation as u8;
          if rot == idx1 || rot == idx2 {
            front_corners += 1;
          }
        }
      }
    } else {
      let table_opt: Option<&[[(i32, i32); 4]; 4]> = if is_z {
        Some(&CORNER_TABLE_Z)
      } else if is_l {
        Some(&CORNER_TABLE_L)
      } else if is_s {
        Some(&CORNER_TABLE_S)
      } else if is_j {
        Some(&CORNER_TABLE_J)
      } else {
        None
      };

      if let Some(table) = table_opt {
        let rot_table = table[rotation];
        for (cx, cy) in rot_table {
          if self
            .board
            .occupied(self.falling.x() + cx + 1, self.falling.y() - cy - 1)
          {
            corners += 1;
          }
        }
      } else {
        return Spin::None;
      }
    }

    if corners < 3 {
      return Spin::None;
    }

    let spin_bonuses = self.spin_bonuses_mini();
    let mut spin = Spin::Normal;
    if spin_bonuses && front_corners != 2 {
      spin = Spin::Mini;
    }
    if fin_or_tst {
      spin = Spin::Normal;
    }
    spin
  }

  fn spin_bonuses_mini(&self) -> bool {
    let rules = &*utils::kicks::SPIN_BONUS_RULES;
    rules
      .get(&self.game_options.spin_bonuses)
      .map(|r| r.types_mini.contains(&self.falling.symbol.as_str()))
      .unwrap_or(false)
  }

  fn max_spin(a: Spin, b: Spin) -> Spin {
    let score = |s| match s {
      Spin::Normal => 2,
      Spin::Mini => 1,
      Spin::None => 0,
    };
    if score(b) >= score(a) { b } else { a }
  }

  fn detect_spin(&self, fin_or_tst: bool) -> Spin {
    let bonuses = &self.game_options.spin_bonuses;
    if bonuses == &SpinBonuses::None {
      return Spin::None;
    }

    let symbol = self.falling.symbol.as_str().to_ascii_lowercase();
    let t_spin = match bonuses {
      SpinBonuses::All
      | SpinBonuses::AllMini
      | SpinBonuses::AllMiniPlus
      | SpinBonuses::AllPlus
      | SpinBonuses::TSpins
      | SpinBonuses::TSpinsPlus => {
        if symbol == "t" {
          Some(self.detect_spin_from_corners(fin_or_tst))
        } else {
          None
        }
      }
      _ => None,
    };

    let all_spin = self.falling.is_all_spin_position(&self.board.state);

    match bonuses {
      SpinBonuses::Stupid => {
        if self.falling.is_stupid_spin_position(&self.board.state) {
          Spin::Normal
        } else {
          Spin::None
        }
      }
      SpinBonuses::TSpins => t_spin.unwrap_or(Spin::None),
      SpinBonuses::TSpinsPlus => Self::max_spin(
        t_spin.unwrap_or(Spin::None),
        if all_spin && symbol == "t" {
          Spin::Mini
        } else {
          Spin::None
        },
      ),
      SpinBonuses::All => {
        t_spin
          .unwrap_or(Spin::None)
          .max_with(if all_spin { Spin::Normal } else { Spin::None })
      }
      SpinBonuses::AllMini => {
        t_spin
          .unwrap_or(Spin::None)
          .max_with(if all_spin { Spin::Mini } else { Spin::None })
      }
      SpinBonuses::AllPlus => Self::max_spin(
        t_spin.unwrap_or(Spin::None),
        if all_spin {
          if symbol == "t" {
            Spin::Mini
          } else {
            Spin::Normal
          }
        } else {
          Spin::None
        },
      ),
      SpinBonuses::AllMiniPlus => Self::max_spin(
        t_spin.unwrap_or(Spin::None),
        if all_spin { Spin::Mini } else { Spin::None },
      ),
      SpinBonuses::MiniOnly => {
        let t = t_spin.unwrap_or(Spin::None);
        let base = if t == Spin::Normal { Spin::Mini } else { t };
        Self::max_spin(base, if all_spin { Spin::Mini } else { Spin::None })
      }
      SpinBonuses::Handheld => self.detect_spin_from_corners(fin_or_tst),
      _ => Spin::None,
    }
  }

  fn lock(&mut self, hard: bool) -> LockResult {
    self.hold_locked = false;

    let blocks = self.falling.blocks().to_vec();
    let mut placed: Vec<(Mino, i32, i32)> = Vec::with_capacity(blocks.len());
    let mut placed_pos: Vec<(i32, i32)> = Vec::with_capacity(blocks.len());
    let mut connect_input: Vec<(i32, i32)> = Vec::with_capacity(blocks.len());

    for &(bx, by, _) in &blocks {
      let x = self.falling.x() + bx;
      let y = self.falling.y() - by;
      placed.push((self.falling.symbol, x, y));
      placed_pos.push((x, y));
      connect_input.push((x, -y));
    }

    let connected = self.connect_blocks(&connect_input);
    let board_add: Vec<(Tile, i32, i32)> = connected
      .iter()
      .map(|&(cx, cy, cs)| {
        (
          Tile {
            mino: self.falling.symbol,
            connections: cs,
          },
          cx,
          -cy,
        )
      })
      .collect();
    self.board.add(&board_add);

    let clear_res = self.board.clear_bombs_and_lines(&placed_pos);
    let lines = clear_res.lines;
    let garbage_cleared = clear_res.garbage_cleared;
    let pc = self.board.perfect_clear();

    self.stats.garbage_cleared += garbage_cleared as u64;

    let mut broke_b2b: Option<i32> = Some(self.stats.b2b);
    let last_spin = self.last_spin.unwrap_or(Spin::None);

    if lines > 0 {
      self.stats.combo += 1;
      let is_powerful = (last_spin != Spin::None) || lines >= 4;
      let pc_b2b = pc && self.pc.as_ref().map(|p| p.b2b).unwrap_or(0) > 0;

      if is_powerful && !pc_b2b {
        self.stats.b2b += 1;
        broke_b2b = None;
      }
      if pc_b2b {
        self.stats.b2b += self.pc.as_ref().unwrap().b2b as i32;
        broke_b2b = None;
      }
      if broke_b2b.is_some() {
        self.stats.b2b = -1;
      }
    } else {
      self.stats.combo = -1;
      broke_b2b = None;
    }

    let special_bonus = self.garbage_queue.options().special_bonus
      && garbage_cleared > 0
      && (last_spin != Spin::None || lines >= 4);
    let g_special_bonus: u32 = if special_bonus { 1 } else { 0 };

    let combo_table = self.game_options.combo_table.clone();

    let calc_input = GarbageCalcInput {
      b2b: self.stats.b2b.max(0),
      combo: self.stats.combo.max(0),
      enemies: 0,
      lines,
      piece: self.falling.symbol,
      spin: last_spin,
    };
    let calc_config = GarbageCalcConfig {
      spin_bonuses: self.game_options.spin_bonuses.clone(),
      combo_table,
      garbage_target_bonus: self.game_options.garbage_target_bonus.clone(),
      b2b_chaining: self.b2b.chaining,
      b2b_charging: self.b2b.charging.is_some(),
    };
    let garbage_result = garbage_calc_v2(&calc_input, &calc_config);

    let garb_mult = self.dynamic.1.get();
    let mut g_events: Vec<u32> = Vec::new();
    if garbage_result.garbage > 0.0 || g_special_bonus > 0 {
      let rounded = self
        .garbage_queue
        .round(garbage_result.garbage * garb_mult + g_special_bonus as f64);
      g_events.push(rounded);
    }

    let mut surged: u32 = 0;
    if let Some(btb) = broke_b2b {
      if let Some(charging) = &self.b2b.charging {
        if (btb as i64 + 1) > charging.at as i64 {
          surged = ((btb as f64 - charging.at as f64 + charging.base as f64 + 1.0) * garb_mult)
            .floor() as u32;
          let g1 = (surged as f64 / 3.0).round() as u32;
          let g2 = (surged as f64 / 3.0).round() as u32;
          let g3 = surged.saturating_sub(2 * g1);
          g_events.splice(0..0, [g1, g2, g3]);
        }
      }
    }

    if pc {
      if let Some(pc_opts) = &self.pc {
        let rounded = self.garbage_queue.round(pc_opts.garbage * garb_mult);
        g_events.push(rounded);
      }
    }

    let mut filtered_garbage: Vec<u32> = g_events.into_iter().filter(|&g| g > 0).collect();
    let raw_garbage = filtered_garbage.clone();

    for &g in &raw_garbage {
      self.stats.garbage_attack += g as u64;
    }

    let mut garbage_added: Option<Vec<OutgoingGarbage>> = None;

    if lines > 0 {
      self.last_was_clear = true;
      while !filtered_garbage.is_empty() {
        if *filtered_garbage.first().unwrap() == 0 {
          filtered_garbage.remove(0);
          continue;
        }

        let legacy_opener = self
          .misc
          .date
          .map(|d| {
            let cutoff = chrono::DateTime::parse_from_rfc3339("2025-02-16T00:00:00Z")
              .unwrap()
              .with_timezone(&chrono::Utc);
            d < cutoff
          })
          .unwrap_or(false);
        let (remaining, cancelled) =
          self
            .garbage_queue
            .cancel(filtered_garbage[0], self.stats.pieces, legacy_opener);

        for c in &cancelled {
          self.events.emit(events::garbage::Cancel {
            iid: c.cid,
            amount: c.amount,
            size: c.size,
          });
        }

        if remaining == 0 {
          filtered_garbage.remove(0);
        } else {
          filtered_garbage[0] = remaining;
          break;
        }
      }
    } else {
      self.last_was_clear = false;
      let garbages = self
        .garbage_queue
        .tank(self.frame, self.dynamic.2.get(), hard);
      if !garbages.is_empty() {
        for (idx, g) in garbages.iter().enumerate() {
          let is_beg = idx == 0 || garbages[idx - 1].id != g.id;
          let is_end = idx == garbages.len() - 1 || garbages[idx + 1].id != g.id;
          let bomb_opt = self.garbage_queue.options().bombs;
          self.board.insert_garbage(InsertGarbageParams {
            amount: g.amount,
            size: g.size,
            column: g.column,
            bombs: bomb_opt,
            is_beginning: is_beg,
            is_end,
          });
          self.events.emit(events::garbage::Tank {
            iid: g.id,
            column: g.column,
            amount: g.amount,
            size: g.size,
          });
        }
        garbage_added = Some(garbages);
      }
    }

    self.events.emit(events::falling::LockPre {});
    self.next_piece(false, false);
    self.last_spin = None;

    let topout = {
      let abs = self.falling.absolute_blocks();
      !legal(&abs, &self.board.state)
    };

    let sent_total: u32 = filtered_garbage.iter().sum();

    if !filtered_garbage.is_empty() {
      if let Some(mp) = &self.multiplayer {
        for &target in &mp.targets.clone() {
          for &g in &filtered_garbage {
            self.ige_handler.send(target, g);
          }
        }
      }
    }

    self.stats.garbage_sent += sent_total as u64;

    if sent_total > 0 {
      self.spike.count += sent_total;
      self.spike.timer = 60;
    }

    self.res_cache.pieces += 1;
    self
      .res_cache
      .garbage_sent
      .extend_from_slice(&filtered_garbage);
    if let Some(ref ga) = garbage_added {
      self.res_cache.garbage_received.extend_from_slice(ga);
    }
    self.res_cache.last_lock = self.frame as f64 + self.subframe;

    self.stats.pieces += 1;
    self.stats.lines += lines as u64;

    let result = LockResult {
      mino: self.falling.symbol,
      garbage_cleared,
      lines,
      spin: last_spin,
      raw_garbage,
      garbage: filtered_garbage,
      surge: surged,
      stats: self.stats.clone(),
      garbage_added,
      topout,
      piece_time: ((self.frame as f64 + self.subframe - self.res_cache.last_lock) * 10.0).round()
        / 10.0,
      key_presses: Vec::new(),
    };

    self.events.emit(events::falling::Lock(result.clone()));
    result
  }

  fn internal_shift(&mut self) -> bool {
    if !self.is_sleep() {
      self.state |= ACTION_MOVE;
      let check = self.falling.absolute_at(
        Some(self.falling.location[0] + self.input.last_shift as f64),
        None,
        None,
      );
      if legal(&check, &self.board.state) {
        self.falling.location[0] += self.input.last_shift as f64;
        if self.falling.lock_resets < 31 {
          self.falling.lock_resets += 1;
        }
        self.state &= !(ROTATION_ALL | STATE_WALL);
        if self.is_20g() {
          self.slam_to_floor();
        }
        if !self.should_lock() {
          self.falling.locking = 0.0;
        }
        return true;
      } else {
        self.state |= STATE_WALL;
      }
    }
    false
  }

  fn process_shift(&mut self, shift: u8, delta: f64) {
    let (held, dir, das_val, arr_val) = if shift == 0 {
      (
        self.input.l_shift.held,
        self.input.l_shift.dir,
        self.input.l_shift.das,
        self.input.l_shift.arr,
      )
    } else {
      (
        self.input.r_shift.held,
        self.input.r_shift.dir,
        self.input.r_shift.das,
        self.input.r_shift.arr,
      )
    };

    if !held || self.input.last_shift != dir {
      return;
    }

    let das = self.handling.das;
    let arr = self.handling.arr;
    let arr_delta = (delta - (das - das_val).max(0.0)).max(0.0);
    let new_das = (das_val + delta).min(das);

    if shift == 0 {
      self.input.l_shift.das = new_das;
    } else {
      self.input.r_shift.das = new_das;
    }
    if new_das < das {
      return;
    }
    if self.is_sleep() {
      return;
    }

    let new_arr = arr_val + arr_delta;
    if shift == 0 {
      self.input.l_shift.arr = new_arr;
    } else {
      self.input.r_shift.arr = new_arr;
    }
    if new_arr < arr {
      return;
    }

    let arr_mult = if arr == 0.0 {
      self.board.width as u64
    } else {
      (new_arr / arr).floor() as u64
    };
    let consume = arr * arr_mult as f64;
    if shift == 0 {
      self.input.l_shift.arr -= consume;
    } else {
      self.input.r_shift.arr -= consume;
    }

    for _ in 0..arr_mult {
      self.internal_shift();
    }
  }

  fn process_all_shift(&mut self, sub_frame_diff: f64) {
    self.process_shift(0, sub_frame_diff);
    self.process_shift(1, sub_frame_diff);
  }

  fn process_subframe(&mut self, subframe: f64) {
    if subframe <= self.subframe {
      return;
    }
    let delta = subframe - self.subframe;
    self.process_all_shift(delta);
    self.fall_step(delta);
    self.subframe = subframe;
  }

  fn process_interrupts(&mut self) {
    if self.misc.allowed.retry && self.practice.retry {
      if self.misc.stride {
        self.retry();
        return;
      }
      let tick = self.practice.retry_iter;
      self.practice.retry_iter += 1;
      if tick > 15 {
        self.retry();
      }
    }
  }

  fn tick_spike(&mut self) {
    if self.spike.timer > 0 {
      self.spike.timer -= 1;
      if self.spike.timer == 0 {
        self.spike.count = 0;
      }
    }
  }

  pub fn move_right(&mut self) -> bool {
    let res = self.falling.move_right(&self.board.state);
    if res && self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    res
  }

  pub fn move_left(&mut self) -> bool {
    let res = self.falling.move_left(&self.board.state);
    if res && self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    res
  }

  pub fn das_right(&mut self) -> bool {
    let res = self.falling.das_right(&self.board.state);
    if res && self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    res
  }

  pub fn das_left(&mut self) -> bool {
    let res = self.falling.das_left(&self.board.state);
    if res && self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    res
  }

  pub fn soft_drop(&mut self) -> bool {
    let res = self.falling.soft_drop(&self.board.state);
    if res && self.game_options.spin_bonuses != SpinBonuses::Stupid {
      self.last_spin = None;
    }
    res
  }

  pub fn hard_drop(&mut self) -> LockResult {
    while self.internal_fall(1.0) {}
    self.lock(true)
  }

  pub fn rotate_cw(&mut self) -> bool {
    self.rotate(1, false)
  }
  pub fn rotate_ccw(&mut self) -> bool {
    self.rotate(-1, false)
  }
  pub fn rotate_180(&mut self) -> bool {
    self.rotate(2, false)
  }

  pub fn undo(&mut self) -> bool {
    if !self.misc.allowed.undo || self.practice.undo.is_empty() {
      return false;
    }
    let snap = self.snapshot_internal(true);
    self.practice.redo.push(snap);
    let prev = self.practice.undo.pop().unwrap();
    self.from_snapshot(&prev);
    self.practice.retry = false;
    self.practice.retry_iter = 0;
    true
  }

  pub fn redo(&mut self) -> bool {
    if !self.misc.allowed.undo || self.practice.redo.is_empty() {
      return false;
    }
    let snap = self.snapshot_internal(true);
    self.practice.undo.push(snap);
    let next = self.practice.redo.pop().unwrap();
    self.from_snapshot(&next);
    self.practice.retry = false;
    self.practice.retry_iter = 0;
    true
  }

  pub fn retry(&mut self) {
    if self.misc.allowed.undo {
      if let Some(ref snap) = self.practice.last_piece {
        let s = *snap.clone();
        self.practice.undo.push(s);
      }
      if self.practice.undo.len() > 100 {
        self.practice.undo.remove(0);
      }
    }

    self.practice.retry = false;
    self.practice.retry_iter = 0;
    self.held = None;
    self.hold_locked = false;
    self.__internal_queue.clear();
    self.__internal_queue.repopulate_once();
    self.queue.from_snapshot(&self.__internal_queue.snapshot());
    self.board.reset();
    self.garbage_queue.reset();
    self.stats = EngineStats {
      garbage_sent: 0,
      garbage_attack: 0,
      garbage_receive: 0,
      garbage_cleared: 0,
      combo: -1,
      b2b: -1,
      pieces: 0,
      lines: 0,
    };
    self.time_frame_offset = self.frame;
    self.next_piece(false, false);
  }

  pub fn receive_garbage(&mut self, garbages: Vec<IncomingGarbage>) {
    self.garbage_queue.receive(garbages);
  }

  pub fn tick(&mut self, frames: &[replay::Frame]) -> ResCache {
    self.subframe = 0.0;

    for frame in frames {
      match &frame.data {
        replay::FrameData::KeyDown(kp) => self.handle_keydown(kp),
        replay::FrameData::KeyUp(kp) => self.handle_keyup(kp),
        replay::FrameData::IGE(ige) => self.handle_ige(ige),
        _ => {}
      }
    }

    self.frame += 1;
    self.process_all_shift(1.0 - self.subframe);
    self.fall_step(1.0 - self.subframe);
    self.process_interrupts();
    self.tick_spike();

    self.dynamic.0.tick();
    self.dynamic.1.tick();
    self.dynamic.2.tick();

    self.flush_res()
  }

  fn handle_keydown(&mut self, keypress: &replay::Keypress) {
    self.process_subframe(keypress.subframe);
    self.res_cache.keys.push(keypress.key.as_str().to_string());

    match keypress.key {
      Key::MoveLeft => {
        self.falling.keys += 1;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
        self.input.l_shift.held = true;
        self.input.l_shift.das = if keypress.hoisted {
          self.handling.das - self.handling.dcd
        } else {
          0.0
        };
        self.input.l_shift.arr = self.handling.arr;
        self.input.last_shift = self.input.l_shift.dir;
        self.internal_shift();
      }
      Key::MoveRight => {
        self.falling.keys += 1;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
        self.input.r_shift.held = true;
        self.input.r_shift.das = if keypress.hoisted {
          self.handling.das - self.handling.dcd
        } else {
          0.0
        };
        self.input.r_shift.arr = self.handling.arr;
        self.input.last_shift = self.input.r_shift.dir;
        self.internal_shift();
      }
      Key::SoftDrop => {
        self.input.keys.soft_drop = true;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
      }
      Key::Retry => {
        self.practice.retry = true;
        self.practice.retry_iter = 0;
      }
      Key::Undo => {
        self.undo();
      }
      Key::Redo => {
        self.redo();
      }
      Key::RotateCCW => {
        self.input.keys.rotate_ccw = true;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
        self.rotate(-1, false);
        self.falling.keys += 1;
      }
      Key::RotateCW => {
        self.input.keys.rotate_cw = true;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
        self.rotate(1, false);
        self.falling.keys += 1;
      }
      Key::Rotate180 => {
        if !self.misc.allowed.spin180 {
          return;
        }
        self.input.keys.rotate_180 = true;
        if self.input.first_input_time < 0.0 {
          self.input.first_input_time = self.frame as f64 + self.subframe;
        }
        self.rotate(2, false);
        self.falling.keys += 2;
      }
      Key::HardDrop => {
        if !self.misc.allowed.hard_drop || self.falling.safe_lock != 0 {
          return;
        }
        self.hard_drop();
      }
      Key::Hold => {
        self.input.keys.hold = true;
        self.hold(false, false);
      }
    }
  }

  fn handle_keyup(&mut self, keypress: &replay::Keypress) {
    self.process_subframe(keypress.subframe);
    match keypress.key {
      Key::MoveLeft => {
        self.input.l_shift.held = false;
        self.input.l_shift.das = 0.0;
        self.input.last_shift = if self.input.r_shift.held {
          self.input.r_shift.dir
        } else {
          self.input.last_shift
        };
        if self.handling.cancel {
          self.input.r_shift.arr = self.handling.arr;
          self.input.r_shift.das = 0.0;
        }
      }
      Key::MoveRight => {
        self.input.r_shift.held = false;
        self.input.r_shift.das = 0.0;
        self.input.last_shift = if self.input.l_shift.held {
          self.input.l_shift.dir
        } else {
          self.input.last_shift
        };
        if self.handling.cancel {
          self.input.l_shift.arr = self.handling.arr;
          self.input.l_shift.das = 0.0;
        }
      }
      Key::SoftDrop => {
        self.state |= ACTION_SOFTDROP;
        self.input.keys.soft_drop = false;
      }
      Key::Retry => {
        self.practice.retry = false;
        self.practice.retry_iter = 0;
      }
      Key::RotateCCW => {
        self.input.keys.rotate_ccw = false;
      }
      Key::RotateCW => {
        self.input.keys.rotate_cw = false;
      }
      Key::Rotate180 => {
        self.input.keys.rotate_180 = false;
      }
      Key::Hold => {
        self.input.keys.hold = false;
      }
      _ => {}
    }
  }

  fn handle_ige(&mut self, ige: &ige::IGE) {
    match &ige.data {
      ige::IGEData::Interaction(interaction) => match interaction {
        ige::interaction::InteractionData::Garbage(data) => {
          let ige::interaction::Garbage {
            gameid,
            ackiid,
            iid,
            amt,
            size,
            ..
          } = data;
          let original = *amt as u32;
          let amount = if self
            .multiplayer
            .as_ref()
            .map(|m| m.passthrough_network)
            .unwrap_or(false)
          {
            self
              .ige_handler
              .receive(*gameid, *ackiid, *iid, *amt as u32)
          } else {
            *amt as u32
          };
          let gf = u64::MAX / 2 - self.garbage_queue.options().garbage.speed;
          self.receive_garbage(vec![IncomingGarbage {
            frame: gf,
            amount,
            size: *size as usize,
            cid: *iid,
            gameid: *gameid,
            confirmed: false,
          }]);
          self.stats.garbage_receive += amount as u64;
          self.events.emit(events::garbage::Receive {
            iid: *iid,
            amount,
            original_amount: original,
          });
        }
        _ => {}
      },

      ige::IGEData::InteractionConfirm(interaction) => match interaction {
        ige::interaction::InteractionData::Garbage(data) => {
          let ige::interaction::Garbage { gameid, iid, .. } = data;
          self.garbage_queue.confirm(*iid, *gameid, self.frame);
          self.events.emit(events::garbage::Confirm {
            iid: *iid,
            gameid: *gameid,
            frame: self.frame,
          });
        }
        _ => {}
      },
      ige::IGEData::Target(data) => {
        if let Some(mp) = &mut self.multiplayer {
          mp.targets = data.targets.clone();
        }
      }
      _ => {}
    }
  }

  pub fn snapshot(&self) -> EngineSnapshot {
    self.snapshot_internal(false)
  }

  fn snapshot_internal(&self, is_undo_redo: bool) -> EngineSnapshot {
    EngineSnapshot {
      is_undo_redo,
      board: self.board.state.clone(),
      falling: self.falling.snapshot(),
      frame: self.frame,
      garbage: self.garbage_queue.snapshot(),
      hold: self.held,
      hold_locked: self.hold_locked,
      last_spin: self.last_spin,
      last_was_clear: self.last_was_clear,
      queue: self.queue.snapshot(),
      __internal_queue: self.__internal_queue.snapshot(),
      input: self.input.clone(),
      subframe: self.subframe,
      targets: self.multiplayer.as_ref().map(|m| m.targets.clone()),
      stats: self.stats.clone(),
      glock: self.glock,
      stock: self.stock,
      state: self.state,
      spike: self.spike.clone(),
      time_frame_offset: self.time_frame_offset,
      res_cache: self.res_cache.clone(),
      practice: if is_undo_redo {
        PracticeState {
          last_piece: None,
          redo: Vec::new(),
          undo: Vec::new(),
          retry: self.practice.retry,
          retry_iter: self.practice.retry_iter,
        }
      } else {
        self.practice.clone()
      },
      ige: self.ige_handler.snapshot(),
    }
  }

  pub fn from_snapshot(&mut self, snapshot: &EngineSnapshot) {
    let is_undo_redo = snapshot.is_undo_redo;

    self.board.state = snapshot.board.clone();

    self.falling = Tetromino::from_snapshot(&snapshot.falling, self.board.height, self.board.width);

    if !is_undo_redo {
      self.frame = snapshot.frame;
      self.subframe = snapshot.subframe;
    }

    self.garbage_queue.from_snapshot(&snapshot.garbage);
    self.held = snapshot.hold;
    self.hold_locked = snapshot.hold_locked;
    self.last_spin = snapshot.last_spin;
    self.last_was_clear = snapshot.last_was_clear;
    self.queue.from_snapshot(&snapshot.queue);
    self
      .__internal_queue
      .from_snapshot(&snapshot.__internal_queue);

    let p = &self.initializer;
    self.dynamic = (
      IncreaseTracker::new(
        p.gravity.value,
        p.gravity.increase,
        p.gravity.margin_time as u32,
      ),
      IncreaseTracker::new(
        p.garbage.multiplier.value,
        p.garbage.multiplier.increase,
        p.garbage.multiplier.margin_time as u32,
      ),
      IncreaseTracker::new(
        p.garbage.cap.value,
        p.garbage.cap.increase,
        p.garbage.cap.margin_time as u32,
      ),
    );
    for _ in 0..self.frame {
      self.dynamic.0.tick();
      self.dynamic.1.tick();
      self.dynamic.2.tick();
    }

    if !is_undo_redo {
      self.input = snapshot.input.clone();
    } else {
      self.input.first_input_time = snapshot.input.first_input_time;
      self.input.time = snapshot.input.time.clone();
      self.input.last_piece_time = snapshot.input.last_piece_time;
      let soft = self.input.keys.soft_drop;
      self.input.keys = snapshot.input.keys.clone();
      self.input.keys.soft_drop = soft;
    }

    if let Some(mp) = &mut self.multiplayer {
      if let Some(targets) = &snapshot.targets {
        if !is_undo_redo {
          mp.targets = targets.clone();
        }
      }
    }

    self.stats = snapshot.stats.clone();
    self.glock = snapshot.glock;
    self.stock = snapshot.stock;
    self.state = snapshot.state;
    self.spike = snapshot.spike.clone();
    self.time_frame_offset = snapshot.time_frame_offset;
    self.ige_handler.from_snapshot(&snapshot.ige);
    self.res_cache = snapshot.res_cache.clone();

    self.practice = PracticeState {
      retry: snapshot.practice.retry,
      retry_iter: snapshot.practice.retry_iter,
      last_piece: if !is_undo_redo {
        snapshot.practice.last_piece.clone()
      } else {
        self.practice.last_piece.clone()
      },
      redo: if !is_undo_redo {
        snapshot.practice.redo.clone()
      } else {
        self.practice.redo.clone()
      },
      undo: if !is_undo_redo {
        snapshot.practice.undo.clone()
      } else {
        self.practice.undo.clone()
      },
    };
  }

  pub fn get_preview(&self, piece: Mino) -> &utils::tetromino::data::PreviewData {
    MinoExt::from(piece).preview()
  }

  // get text() {
  //   const boardTop = this.board.state.findIndex((row: Tile[]) =>
  //     row.every((block) => block === null)
  //   );
  //   const height = Math.max(this.garbageQueue.size, boardTop, 0);

  //   const output: string[] = [];
  //   for (let i = 0; i < height; i++) {
  //     let str = i % 2 === 0 ? "|" : " ";
  //     if (i < this.garbageQueue.size) str += " " + chalk.bgRed(" ") + " ";
  //     else str += "   ";
  //     if (i > boardTop) {
  //       output.push(
  //         str + "  ".repeat(this.board.width) + " " + (i % 2 === 0 ? "|" : " ")
  //       );
  //       continue;
  //     }

  //     for (let j = 0; j < this.board.width; j++) {
  //       const block = this.board.state[i][j];
  //       str += block ? Engine.colorMap[block.mino]("  ") : "  ";
  //     }

  //     output.push(str + " " + (i % 2 === 0 ? "|" : " "));
  //   }

  //   return output.reverse().join("\n");
  // }

  pub fn print(&self) {
    use colored::Colorize;

    let board_top = self
      .board
      .state
      .iter()
      .position(|row| row.iter().all(|block| block.is_none()))
      .unwrap_or(0);
    let height = self
      .board
      .height
      .max(self.garbage_queue.size() as usize)
      .max(board_top);

    for i in (0..height).rev() {
      let mut line = String::new();
      line.push_str(if i % 2 == 0 { "|" } else { " " });
      if i < self.garbage_queue.size() as usize {
        line.push_str(&format!(" {} ", " ".on_red()));
      } else {
        line.push_str("   ");
      }
      if i >= board_top {
        line.push_str(&"  ".repeat(self.board.width));
        line.push_str(if i % 2 == 0 { "|" } else { " " });
        println!("{}", line);
        continue;
      }

      for j in 0..self.board.width {
        let block = &self.board.state[i][j];
        if let Some(block) = block {
          let colored = match block.mino {
            Mino::I => "  ".on_truecolor(0, 240, 240),
            Mino::J => "  ".on_truecolor(0, 114, 188),
            Mino::L => "  ".on_truecolor(240, 160, 0),
            Mino::O => "  ".on_truecolor(240, 240, 0),
            Mino::S => "  ".on_truecolor(0, 240, 0),
            Mino::T => "  ".on_truecolor(160, 0, 240),
            Mino::Z => "  ".on_truecolor(240, 0, 0),
            Mino::Garbage => "  ".on_bright_black(),
            Mino::Bomb => "  ".on_truecolor(255, 165, 0),
          };
          line.push_str(&colored.to_string());
        } else {
          line.push_str("  ");
        }
      }
      line.push_str(if i % 2 == 0 { "|" } else { " " });
      println!("{}", line);
    }
  }
}

trait SpinExt {
  fn max_with(self, other: Spin) -> Spin;
}

impl SpinExt for Spin {
  fn max_with(self, other: Spin) -> Spin {
    Engine::max_spin(self, other)
  }
}
