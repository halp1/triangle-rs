use std::sync::Arc;
use std::{collections::HashMap, pin::Pin};

use futures_util::future::join_all;
use serde_json::Value;
use tokio::sync::Mutex;
use parking_lot::Mutex as PMutex;

use crate::types::game::tick;
use crate::{
  classes::{
    ClientUser,
    game::{me::Me, player::Player},
  },
  engine::{
    AllowedOptions, B2bCharging, B2bOptions, Engine, EngineInitParams, EngineSnapshot, EngineStats,
    GameOptions, IncreasableValue, InputKeys, InputState, InputTime, MiscOptions, MovementOptions,
    MultiplayerOptions, PcOptions, PracticeState, ResCache, ShiftState, SpikeState,
    board::{BoardInitParams, CONN_ALL, Tile},
    constants::{ROTATION_MINI, ROTATION_SPIN},
    garbage::{
      GarbageCapParams, GarbageQueueInitParams, GarbageQueueSnapshot, GarbageSpeedParams,
      IncomingGarbage, MessinessParams, MultiplierParams, RoundingMode,
    },
    multiplayer::{GarbageRecord, IgeHandlerSnapshot, PlayerData},
    queue::{
      QueueInitParams, QueueSnapshot,
      bag::{BagSnapshot, BagType},
      types::Mino,
    },
    utils::{KickTable, TetrominoSnapshot},
  },
  types::game::{
    Buffering, ComboTable, GarbageBlocking, GarbageTargetBonus, Handling, Passthrough, ReadyPlayer,
    SpectateTarget, SpectatingStrategy, Spin, SpinBonuses,
  },
};

use super::ribbon::{Hook, Ribbon};

pub mod me;
pub mod player;

pub const FRAMES_PER_SECOND: u64 = 60;

fn jv_f64(v: &Value, key: &str, default: f64) -> f64 {
  v.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn jv_u64(v: &Value, key: &str, default: u64) -> u64 {
  v.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn jv_i64(v: &Value, key: &str, default: i64) -> i64 {
  v.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn jv_bool(v: &Value, key: &str, default: bool) -> bool {
  v.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn jv_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
  v.get(key).and_then(Value::as_str)
}

fn jv_string(v: &Value, key: &str, default: &str) -> String {
  jv_str(v, key).unwrap_or(default).to_string()
}

fn parse_kick_table(s: &str) -> KickTable {
  match s {
    "SRS+" => KickTable::SRSPlus,
    "SRS-X" | "SRSX" => KickTable::SRSX,
    "TETRA-X" | "TetraX" => KickTable::TetraX,
    "SRS" => KickTable::SRS,
    "NRS" => KickTable::NRS,
    "ARS" => KickTable::ARS,
    "ASC" => KickTable::ASC,
    "none" | "None" => KickTable::None,
    _ => KickTable::SRSPlus,
  }
}

fn parse_bag_type(v: Option<&str>) -> BagType {
  match v.unwrap_or("7-bag") {
    "7-bag" | "bag7" => BagType::Bag7,
    "14-bag" | "bag14" => BagType::Bag14,
    "classic" => BagType::Classic,
    "pairs" => BagType::Pairs,
    "total mayhem" => BagType::TotalMayhem,
    "7+1" => BagType::Bag7Plus1,
    "7+2" => BagType::Bag7Plus2,
    "7+X" | "7+x" => BagType::Bag7PlusX,
    _ => BagType::Bag7,
  }
}

fn parse_buffering(s: &str) -> Buffering {
  match s {
    "off" => Buffering::Off,
    "hold" => Buffering::Hold,
    _ => Buffering::Tap,
  }
}

fn parse_rounding_mode(v: Option<&str>) -> RoundingMode {
  match v.unwrap_or("down") {
    "rng" => RoundingMode::Rng,
    _ => RoundingMode::Down,
  }
}

fn parse_serde_str<T: serde::de::DeserializeOwned>(s: &str, default: T) -> T {
  serde_json::from_value(Value::String(s.to_string())).unwrap_or(default)
}

fn parse_mino(v: &Value) -> Option<Mino> {
  serde_json::from_value(v.clone()).ok()
}

fn parse_mino_array(v: &Value) -> Vec<Mino> {
  v.as_array()
    .map(|arr| arr.iter().filter_map(parse_mino).collect())
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct GameState {
  pub strategy: SpectatingStrategy,
  pub spectating_loop_handle: Arc<PMutex<Option<tokio::task::JoinHandle<()>>>>, // very cursed, maybe a better way?
  pub spectate_warning_counter: u64,
  pub players: Vec<Player>,
}

#[derive(Clone, Debug)]
pub struct Game {
  ribbon: Ribbon,
  hook: Hook,

  pub me: Option<Me>,
  pub state: Arc<PMutex<GameState>>,
  pub raw_players: Arc<Vec<ReadyPlayer>>,
}

impl Game {
  pub async fn new(
    ribbon: Ribbon,
    user: ClientUser,
    raw_players: Vec<ReadyPlayer>,
    strategy: SpectatingStrategy,
  ) -> Self {
    let me_in_game = raw_players.iter().find(|p| p.userid == user.id).is_some();

    let me = if me_in_game {
      Some(Me::new(ribbon.clone(), user.clone(), raw_players.clone()).await)
    } else {
      None
    };

    let players = join_all(
      raw_players
        .iter()
        .map(|p| Player::new(ribbon.clone(), strategy, p.clone(), raw_players.clone())),
    )
    .await;

    ribbon.set_faster_ping(true).await;

    let s = Self {
      ribbon: ribbon.clone(),
      hook: ribbon.hook(),
      me,
      state: Arc::new(PMutex::new(GameState {
        strategy,
        spectating_loop_handle: Arc::new(PMutex::new(None)),
        spectate_warning_counter: 0,
        players,
      })),
      raw_players: Arc::new(raw_players),
    };

    s.start_spectating_loop().await;

    if let Some(me) = &s.me {
      me.init().await;
    }

    s
  }

  async fn start_spectating_loop(&self) {
    let game = self.clone();

    self.state.lock().spectating_loop_handle.lock().replace(tokio::task::spawn(async move {
			loop {
				let start = std::time::Instant::now();

				for player in game.state.lock().players.clone() {
					player._tick().await;
				}

				if start.elapsed() > std::time::Duration::from_millis(1000 / FRAMES_PER_SECOND - 1) {
					let mut state = game.state.lock();
					state.spectate_warning_counter += 1;
					if state.spectate_warning_counter == 5 {
						tracing::warn!(
							"Spectating is falling behind! You are spectating too many players. Consider reducing the number of players you are spectating to improve performance."
						);
					}
				} else {
					game.state.lock().spectate_warning_counter = 0;
				}

				tokio::time::sleep((std::time::Duration::from_millis(1000 / FRAMES_PER_SECOND).saturating_sub(start.elapsed())).max(std::time::Duration::from_micros(50))).await;
			}
		}));
  }

  pub async fn _set_spectating_strategy(&self, strategy: SpectatingStrategy) {
    self.state.lock().strategy = strategy;
    for player in self.state.lock().players.clone() {
      player._set_spectating_strategy(strategy).await;
    }
  }

  pub fn create_engine(options: &Value, gameid: u64, players: &[ReadyPlayer]) -> Engine {
    let handling = options.get("handling").unwrap_or(&Value::Null);
    let seed = jv_i64(options, "seed", 0);
    let b2b_charging = jv_bool(options, "b2bcharging", false);

    Engine::new(EngineInitParams {
      multiplayer: Some(MultiplayerOptions {
        opponents: players
          .iter()
          .map(|p| p.gameid)
          .filter(|&id| id != gameid)
          .collect(),
        passthrough: match jv_string(options, "passthrough", "zero").as_str() {
          "zero" => Passthrough::Zero,
          "limited" => Passthrough::Limited,
          "full" => Passthrough::Full,
          "consistent" => Passthrough::Consistent,
          _ => panic!("Invalid passthrough option"),
        },
      }),
      board: BoardInitParams {
        width: jv_u64(options, "boardwidth", 10) as usize,
        height: jv_u64(options, "boardheight", 20) as usize,
        buffer: 20,
      },
      kick_table: parse_kick_table(jv_str(options, "kickset").unwrap_or("SRS+")),
      options: GameOptions {
        combo_table: parse_serde_str(
          jv_str(options, "combotable").unwrap_or("multiplier"),
          ComboTable::Multiplier,
        ),
        garbage_blocking: parse_serde_str(
          jv_str(options, "garbageblocking").unwrap_or("combo blocking"),
          GarbageBlocking::ComboBlocking,
        ),
        clutch: jv_bool(options, "clutch", true),
        garbage_target_bonus: parse_serde_str(
          jv_str(options, "garbagetargetbonus").unwrap_or("none"),
          GarbageTargetBonus::None,
        ),
        spin_bonuses: parse_serde_str(
          jv_str(options, "spinbonuses").unwrap_or("T-spins"),
          SpinBonuses::TSpins,
        ),
        stock: jv_u64(options, "stock", 0),
      },
      queue: QueueInitParams {
        min_length: 31,
        seed,
        kind: parse_bag_type(jv_str(options, "bagtype")),
      },
      garbage: GarbageQueueInitParams {
        cap: GarbageCapParams {
          absolute: jv_u64(options, "garbageabsolutecap", 0) as u32,
          increase: jv_f64(options, "garbagecapincrease", 0.0),
          max: jv_f64(options, "garbagecapmax", 40.0),
          value: jv_f64(options, "garbagecap", 8.0),
          margin_time: jv_u64(options, "garbagecapmargin", 0),
        },
        multiplier: MultiplierParams {
          value: jv_f64(options, "garbagemultiplier", 1.0),
          increase: jv_f64(options, "garbageincrease", 0.0),
          margin_time: jv_u64(options, "garbagemargin", 0),
        },
        board_width: jv_u64(options, "boardwidth", 10) as usize,
        garbage: GarbageSpeedParams {
          speed: jv_u64(options, "garbagespeed", 20),
          hole_size: jv_u64(options, "garbageholesize", 1),
        },
        messiness: MessinessParams {
          change: jv_f64(options, "messiness_change", 0.0),
          nosame: jv_bool(options, "messiness_nosame", false),
          timeout: jv_u64(options, "messiness_timeout", 0),
          within: jv_f64(options, "messiness_inner", 0.0),
          center: jv_bool(options, "messiness_center", false),
        },
        bombs: jv_bool(options, "usebombs", false),
        seed,
        rounding: parse_rounding_mode(jv_str(options, "roundmode")),
        opener_phase: jv_u64(options, "openerphase", 0) as u32,
        special_bonus: jv_bool(options, "garbagespecialbonus", false),
      },
      pc: if jv_bool(options, "allclears", false) {
        Some(PcOptions {
          garbage: jv_f64(options, "allclear_garbage", 0.0),
          b2b: jv_u64(options, "allclear_b2b", 0),
        })
      } else {
        None
      },
      b2b: B2bOptions {
        chaining: jv_bool(options, "b2bchaining", false),
        charging: if b2b_charging {
          Some(B2bCharging {
            at: jv_u64(options, "b2bcharge_at", 4),
            base: jv_u64(options, "b2bcharge_base", 3),
          })
        } else {
          None
        },
      },
      gravity: IncreasableValue {
        value: jv_f64(options, "g", 0.02),
        increase: jv_f64(options, "gincrease", 0.0),
        margin_time: jv_u64(options, "gmargin", 0),
      },
      misc: MiscOptions {
        movement: MovementOptions {
          infinite: jv_bool(options, "infinite_movement", false),
          lock_resets: jv_u64(options, "lockresets", 15),
          lock_time: jv_f64(options, "locktime", 30.0),
          may_20g: jv_bool(options, "gravitymay20g", false),
        },
        allowed: AllowedOptions {
          spin180: jv_bool(options, "allow180", false),
          hard_drop: jv_bool(options, "allow_harddrop", true),
          hold: jv_bool(options, "display_hold", true),
          undo: jv_bool(options, "can_undo", false),
          retry: jv_bool(options, "can_retry", false),
        },
        infinite_hold: jv_bool(options, "infinite_hold", false),
        stride: jv_bool(options, "stride", false),
        username: jv_str(options, "username").map(str::to_string),
        date: Some(chrono::Utc::now()),
      },
      handling: Handling {
        arr: jv_f64(handling, "arr", 0.0),
        das: jv_f64(handling, "das", 6.0),
        dcd: jv_f64(handling, "dcd", 0.0),
        sdf: jv_f64(handling, "sdf", 41.0),
        safelock: jv_bool(handling, "safelock", false),
        cancel: jv_bool(handling, "cancel", false),
        may20g: jv_bool(handling, "may20g", false),
        irs: parse_buffering(jv_str(handling, "irs").unwrap_or("tap")),
        ihs: parse_buffering(jv_str(handling, "ihs").unwrap_or("tap")),
      },
    })
  }

  pub fn snapshot_from_state(
    frame: u64,
    config: &EngineInitParams,
    state: &Value,
    undo_redo: bool,
  ) -> EngineSnapshot {
    let null = Value::Null;

    let falling = state.get("falling").unwrap_or(&null);
    let flags = jv_u64(falling, "flags", 0) as u32;
    let stats = state.get("stats").unwrap_or(&null);
    let garb_stats = stats.get("garbage").unwrap_or(&null);
    let spike = state.get("spike").unwrap_or(&null);
    let time = state.get("time").unwrap_or(&null);
    let acks = state.get("garbageacknowledgements").unwrap_or(&null);
    let acks_incoming = acks.get("incoming").unwrap_or(&null);
    let acks_outgoing = acks.get("outgoing").unwrap_or(&null);
    let waiting: Vec<Value> = state
      .get("waitingframes")
      .and_then(Value::as_array)
      .cloned()
      .unwrap_or_default();
    let impending: Vec<&Value> = state
      .get("impendingdamage")
      .and_then(Value::as_array)
      .map(|a| a.iter().collect())
      .unwrap_or_default();
    let oth = state.get("otherstates");

    let full_height = config.board.height + config.board.buffer;

    let board: Vec<Vec<Option<Tile>>> =
      if let Some(server_board) = state.get("board").and_then(Value::as_array) {
        let mut rows: Vec<Vec<Option<Tile>>> = server_board
          .iter()
          .rev()
          .map(|row| {
            row
              .as_array()
              .map(|cells| {
                cells
                  .iter()
                  .map(|cell| {
                    if cell.is_null() {
                      None
                    } else {
                      parse_mino(cell).map(|mino| Tile {
                        mino,
                        connections: CONN_ALL,
                      })
                    }
                  })
                  .collect()
              })
              .unwrap_or_else(|| vec![None; config.board.width])
          })
          .collect();
        while rows.len() < full_height {
          rows.push(vec![None; config.board.width]);
        }
        rows
      } else {
        vec![vec![None; config.board.width]; full_height]
      };

    let seed = jv_i64(state, "rng", 0);
    let queue_snapshot = QueueSnapshot {
      value: parse_mino_array(state.get("bag").unwrap_or(&null)),
      bag: BagSnapshot {
        rng: seed,
        id: jv_u64(state, "bagid", 0),
        extra: parse_mino_array(state.get("bagex").unwrap_or(&null)),
        last_generated: state.get("lastGenerated").and_then(parse_mino),
      },
    };

    let garbage_snapshot = GarbageQueueSnapshot {
      seed: jv_i64(state, "rngex", 0),
      last_tank_time: jv_u64(state, "lasttanktime", 0),
      last_column: state
        .get("lastcolumn")
        .filter(|v| !v.is_null())
        .and_then(Value::as_u64)
        .map(|v| v as usize),
      sent: garb_stats.get("sent").and_then(Value::as_u64).unwrap_or(0) as u32,
      has_changed_column: jv_bool(state, "haschangedcolumn", false),
      last_received_count: jv_u64(state, "lastreceivedcount", 0),
      queue: impending
        .iter()
        .map(|g| {
          let id = g.get("id").and_then(Value::as_u64).unwrap_or(0);
          let confirmed_entry = waiting.iter().find(|wf| {
            wf.get("type").and_then(Value::as_str) == Some("incoming-attack-hit")
              && wf.get("data").and_then(Value::as_u64) == Some(id)
          });
          let confirmed = confirmed_entry.is_some();
          let frame_val = confirmed_entry
            .and_then(|wf| wf.get("target").and_then(Value::as_u64))
            .unwrap_or(u64::MAX - config.garbage.garbage.speed);
          IncomingGarbage {
            amount: g.get("amt").and_then(Value::as_u64).unwrap_or(0) as u32,
            cid: g.get("cid").and_then(Value::as_u64).unwrap_or(0),
            gameid: g.get("gameid").and_then(Value::as_u64).unwrap_or(0),
            size: g.get("size").and_then(Value::as_u64).unwrap_or(1) as usize,
            confirmed,
            frame: frame_val,
          }
        })
        .collect(),
    };

    let mut all_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(obj) = acks_incoming.as_object() {
      all_ids.extend(obj.keys().cloned());
    }
    if let Some(obj) = acks_outgoing.as_object() {
      all_ids.extend(obj.keys().cloned());
    }
    let ige_players: HashMap<u64, PlayerData> = all_ids
      .into_iter()
      .filter_map(|k| {
        k.parse::<u64>().ok().map(|pid| {
          let incoming = acks_incoming.get(&k).and_then(Value::as_u64).unwrap_or(0);
          let outgoing = acks_outgoing
            .get(&k)
            .and_then(Value::as_array)
            .map(|arr| {
              arr
                .iter()
                .map(|o| GarbageRecord {
                  iid: o.get("iid").and_then(Value::as_u64).unwrap_or(0),
                  amount: o.get("amt").and_then(Value::as_u64).unwrap_or(0) as u32,
                })
                .collect()
            })
            .unwrap_or_default();
          (pid, PlayerData { incoming, outgoing })
        })
      })
      .collect();

    let l_shift = state.get("lShift").unwrap_or(&null);
    let r_shift = state.get("rShift").unwrap_or(&null);
    let input = InputState {
      l_shift: ShiftState {
        held: jv_bool(l_shift, "held", false),
        arr: jv_f64(l_shift, "arr", 0.0),
        das: jv_f64(l_shift, "das", 0.0),
        dir: jv_i64(l_shift, "dir", -1) as i8,
      },
      r_shift: ShiftState {
        held: jv_bool(r_shift, "held", false),
        arr: jv_f64(r_shift, "arr", 0.0),
        das: jv_f64(r_shift, "das", 0.0),
        dir: jv_i64(r_shift, "dir", 1) as i8,
      },
      last_shift: jv_i64(state, "lastshift", -1) as i8,
      keys: InputKeys {
        hold: jv_bool(state, "inputHold", false),
        rotate_180: jv_bool(state, "inputRotate180", false),
        rotate_ccw: jv_bool(state, "inputRotateCCW", false),
        rotate_cw: jv_bool(state, "inputRotateCW", false),
        soft_drop: jv_bool(state, "inputSoftdrop", false),
      },
      first_input_time: jv_f64(state, "firstInputTime", -1.0),
      time: InputTime {
        start: jv_f64(time, "start", 0.0),
        zero: jv_bool(time, "zero", true),
        locked: jv_bool(time, "locked", false),
        prev: jv_f64(time, "prev", 0.0),
      },
      last_piece_time: jv_f64(state, "lastpiecetime", 0.0),
    };

    let last_spin = if flags & ROTATION_SPIN != 0 {
      Some(Spin::Normal)
    } else if (!flags) & (ROTATION_SPIN | ROTATION_MINI) == 0 {
      Some(Spin::Mini)
    } else {
      None
    };

    let board_full_height = config.board.height + config.board.buffer;
    let falling_snapshot = TetrominoSnapshot {
      symbol: falling.get("type").and_then(parse_mino).unwrap_or(Mino::T),
      location: [
        jv_f64(falling, "x", 0.0),
        board_full_height as f64 - jv_f64(falling, "y", 0.0),
      ],
      locking: jv_f64(falling, "locking", 0.0),
      lock_resets: jv_u64(falling, "lockresets", 0) as u32,
      rot_resets: jv_u64(falling, "rotresets", 0) as u32,
      safe_lock: jv_u64(falling, "safelock", 0) as u32,
      highest_y: board_full_height as f64 - jv_f64(falling, "hy", 0.0),
      rotation: jv_u64(falling, "r", 0) as u8,
      falling_rotations: 0,
      total_rotations: jv_u64(state, "totalRotations", 0) as u32,
      irs: jv_i64(falling, "irs", 0) as i8,
      ihs: false,
      aox: 0,
      aoy: 0,
      keys: jv_u64(falling, "keys", 0) as u32,
    };

    let practice = PracticeState {
      undo: oth
        .and_then(|o| o.get("undo"))
        .and_then(Value::as_array)
        .map(|arr| {
          arr
            .iter()
            .map(|s| Game::snapshot_from_state(frame, config, s, true))
            .collect()
        })
        .unwrap_or_default(),
      redo: oth
        .and_then(|o| o.get("redo"))
        .and_then(Value::as_array)
        .map(|arr| {
          arr
            .iter()
            .map(|s| Game::snapshot_from_state(frame, config, s, true))
            .collect()
        })
        .unwrap_or_default(),
      last_piece: oth
        .and_then(|o| o.get("lastpiece"))
        .filter(|v| !v.is_null())
        .map(|s| Box::new(Game::snapshot_from_state(frame, config, s, true))),
      retry: jv_bool(state, "retry", false),
      retry_iter: jv_u64(state, "retryiter", 0) as u32,
    };

    EngineSnapshot {
      is_undo_redo: undo_redo,
      board,
      falling: falling_snapshot,
      frame,
      garbage: garbage_snapshot,
      hold: state
        .get("hold")
        .filter(|v| !v.is_null())
        .and_then(parse_mino),
      hold_locked: jv_bool(state, "holdlocked", false),
      last_spin,
      last_was_clear: jv_bool(state, "lastwasclear", false),
      queue: queue_snapshot.clone(),
      __internal_queue: queue_snapshot,
      input,
      subframe: jv_f64(state, "subframe", 0.0),
      targets: state
        .get("targets")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_u64).collect()),
      stats: EngineStats {
        garbage_sent: garb_stats.get("sent").and_then(Value::as_u64).unwrap_or(0),
        garbage_attack: garb_stats
          .get("attack")
          .and_then(Value::as_u64)
          .unwrap_or(0),
        garbage_receive: garb_stats
          .get("received")
          .and_then(Value::as_u64)
          .unwrap_or(0),
        garbage_cleared: garb_stats
          .get("cleared")
          .and_then(Value::as_u64)
          .unwrap_or(0),
        b2b: stats.get("btb").and_then(Value::as_i64).unwrap_or(-1) as i32,
        combo: stats.get("combo").and_then(Value::as_i64).unwrap_or(-1) as i32,
        lines: stats.get("lines").and_then(Value::as_u64).unwrap_or(0),
        pieces: stats
          .get("piecesplaced")
          .and_then(Value::as_u64)
          .unwrap_or(0) as u32,
      },
      glock: jv_f64(state, "glock", 0.0),
      stock: jv_u64(state, "stock", 0),
      state: flags,
      spike: SpikeState {
        count: spike.get("count").and_then(Value::as_u64).unwrap_or(0) as u32,
        timer: spike.get("timer").and_then(Value::as_u64).unwrap_or(0) as u32,
      },
      time_frame_offset: time.get("frameoffset").and_then(Value::as_u64).unwrap_or(0),
      res_cache: ResCache {
        pieces: 0,
        garbage_sent: Vec::new(),
        garbage_received: Vec::new(),
        keys: Vec::new(),
        last_lock: 0.0,
      },
      practice,
      ige: IgeHandlerSnapshot {
        iid: jv_u64(state, "interactionid", 0),
        players: ige_players,
      },
    }
  }

  pub async fn spectate(&self, target: SpectateTarget) -> Result<(), Vec<usize>> {
    let players = {
      let state = self.state.lock();
      state.players.clone()
    };

    let to_spectate: Vec<Option<Player>> = match &target {
      SpectateTarget::All => players.iter().map(|p| Some(p.clone())).collect(),
      SpectateTarget::GameIds(ids) => {
        if ids.is_empty() {
          return Ok(());
        }
        ids
          .iter()
          .map(|id| players.iter().find(|p| p.gameid == *id).cloned())
          .collect()
      }
      SpectateTarget::UserIds(uids) => {
        if uids.is_empty() {
          return Ok(());
        }
        uids
          .iter()
          .map(|uid| players.iter().find(|p| p.userid == *uid).cloned())
          .collect()
      }
    };

    let mut invalid = Vec::new();
    for (i, player_opt) in to_spectate.into_iter().enumerate() {
      match player_opt {
        None => invalid.push(i),
        Some(player) => {
          if player.spectate().await.is_err() {
            invalid.push(i);
          }
        }
      }
    }

    if invalid.is_empty() {
      Ok(())
    } else {
      Err(invalid)
    }
  }

  pub async fn unspectate(&self, target: SpectateTarget) -> Result<(), Vec<usize>> {
    let players = {
      let state = self.state.lock();
      state.players.clone()
    };

    let to_unspectate: Vec<Option<Player>> = match &target {
      SpectateTarget::All => players.iter().map(|p| Some(p.clone())).collect(),
      SpectateTarget::GameIds(ids) => {
        if ids.is_empty() {
          return Ok(());
        }
        ids
          .iter()
          .map(|id| players.iter().find(|p| p.gameid == *id).cloned())
          .collect()
      }
      SpectateTarget::UserIds(uids) => {
        if uids.is_empty() {
          return Ok(());
        }
        uids
          .iter()
          .map(|uid| players.iter().find(|p| p.userid == *uid).cloned())
          .collect()
      }
    };

    let mut invalid = Vec::new();
    for (i, player_opt) in to_unspectate.into_iter().enumerate() {
      match player_opt {
        None => invalid.push(i),
        Some(player) => player.unspectate().await,
      }
    }

    if invalid.is_empty() {
      Ok(())
    } else {
      Err(invalid)
    }
  }

  pub async fn _register_ticker(
    &self,
    func: impl Fn(tick::In) -> Pin<Box<dyn Future<Output = tick::Out> + Send + 'static>>
    + Send
    + Sync
    + 'static,
  ) -> Result<(), ()> {
    if let Some(me) = &self.me {
      me.tick.inject(func).await;
      Ok(())
    } else {
      Err(())
    }
  }

  pub async fn destroy(&mut self) {
    if let Some(mut me) = self.me.take() {
      me.destroy().await;
      self.me = None;
    }

    let players = self.state.lock().players.clone();

    for mut player in players {
      player.destroy().await;
    }

    self.ribbon.set_faster_ping(false).await;

    self
      .state
      .lock()
      .await
      .spectating_loop_handle
      .lock()
      .await
      .take()
      .map(|h| h.abort());

    self.hook.destroy().await;
  }
}
