use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{
  engine::{
    AllowedOptions, B2bCharging, B2bOptions, Engine, EngineInitParams,
    GameOptions as EngineGameOptions, HandlingOptions, IgeData, IgeFrame, IncreasableValue,
    KeyEvent, MiscOptions, MovementOptions, MultiplayerOptions, PcOptions, ReplayFrame,
    board::BoardInitParams,
    garbage::{
      GarbageCapParams, GarbageQueueInitParams, GarbageSpeedParams, MessinessParams,
      MultiplierParams, RoundingMode as GarbageRoundingMode,
    },
    queue::{QueueInitParams, bag::BagType},
  },
  error::Result,
  types::game::{Options as GameOptions, RoundingMode},
  utils::EventEmitter,
};

pub const FPS: f64 = 60.0;

#[derive(Debug, Clone)]
pub enum SelfKeyEventType {
  Keydown,
  Keyup,
}

#[derive(Debug, Clone)]
pub struct SelfKeyEvent {
  pub event_type: SelfKeyEventType,
  pub frame: f64,
  pub key: String,
  pub subframe: f64,
}

#[derive(Debug, Clone)]
pub enum TargetStrategy {
  Even,
  Elims,
  Random,
  Payback,
  Manual(u32),
}

/// How to process spectated replay frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectatingStrategy {
  Smooth,
  Instant,
}

/// Spectation state of a player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectatingState {
  Inactive,
  Waiting,
  Active,
}

/// Raw player entry from `game.ready`/`game.spectate` payloads.
#[derive(Debug, Clone)]
pub struct RawPlayer {
  pub gameid: u32,
  pub userid: String,
  pub options: GameOptions,
}

/// A spectated opponent.
pub struct Player {
  pub name: String,
  pub gameid: u32,
  pub userid: String,
  pub engine: Engine,
  pub state: SpectatingState,
  frame_queue: Vec<Value>,
}

impl Player {
  fn new(raw: &RawPlayer, all_players: &[RawPlayer]) -> Self {
    let engine = create_engine(&raw.options, raw.gameid, all_players);
    Self {
      name: raw.options.username.clone().unwrap_or_default(),
      gameid: raw.gameid,
      userid: raw.userid.clone(),
      engine,
      state: SpectatingState::Inactive,
      frame_queue: Vec::new(),
    }
  }

  /// Queue an incoming replay frame for this player.
  pub fn push_frame(&mut self, frame: Value) {
    self.frame_queue.push(frame);
  }

  /// Process queued frames, per strategy.
  pub fn tick(&mut self, strategy: &SpectatingStrategy) {
    if self.state != SpectatingState::Active {
      return;
    }
    match strategy {
      SpectatingStrategy::Smooth => {
        if self.frame_queue.len() > 20 {
          self.frame_queue.clear();
        } else if let Some(frame) = self.frame_queue.first().cloned() {
          self.frame_queue.remove(0);
          self.apply_frame(&frame);
        }
      }
      SpectatingStrategy::Instant => {
        let frames: Vec<Value> = self.frame_queue.drain(..).collect();
        for frame in frames {
          self.apply_frame(&frame);
        }
      }
    }
  }

  fn apply_frame(&mut self, frame: &Value) {
    let replay_frames = parse_replay_frames(frame);
    self.engine.tick(&replay_frames);
  }
}

fn parse_replay_frames(frame: &Value) -> Vec<ReplayFrame> {
  let mut result = Vec::new();
  let frame_type = frame["type"].as_str().unwrap_or("");
  let subframe = frame["data"]["subframe"].as_f64().unwrap_or(0.0);

  match frame_type {
    "keydown" => {
      let key = frame["data"]["key"].as_str().unwrap_or("").to_string();
      result.push(ReplayFrame::Keydown(KeyEvent {
        subframe,
        key,
        hoisted: false,
      }));
    }
    "keyup" => {
      let key = frame["data"]["key"].as_str().unwrap_or("").to_string();
      result.push(ReplayFrame::Keyup(KeyEvent {
        subframe,
        key,
        hoisted: false,
      }));
    }
    "ige" => {
      if let Some(ige_data) = parse_ige_data(frame, subframe) {
        result.push(ReplayFrame::Ige(ige_data));
      }
    }
    _ => {}
  }
  result
}

fn parse_ige_data(frame: &Value, subframe: f64) -> Option<IgeFrame> {
  let ige_type = frame["data"]["type"].as_str()?;
  let data = match ige_type {
    "target" => {
      let targets: Vec<i32> = frame["data"]["data"]["targets"]
        .as_array()
        .map(|a| {
          a.iter()
            .filter_map(|v| v.as_i64().map(|n| n as i32))
            .collect()
        })
        .unwrap_or_default();
      IgeData::Target { targets }
    }
    "interaction" | "interaction_confirm" => {
      let d = &frame["data"]["data"];
      IgeData::GarbageInteraction {
        gameid: d["gameid"].as_i64().unwrap_or(0) as i32,
        ackiid: d["ackiid"].as_i64().unwrap_or(0) as i32,
        iid: d["iid"].as_i64().unwrap_or(0) as i32,
        amt: d["amt"].as_i64().unwrap_or(0) as i32,
        size: d["size"].as_u64().unwrap_or(0) as usize,
      }
    }
    _ => return None,
  };
  Some(IgeFrame { subframe, data })
}

/// The client's own live game state (when the client is playing, not spectating).
pub struct Self_ {
  pub gameid: u32,
  pub engine: Engine,
  pub options: GameOptions,
  pub server_targets: Vec<u32>,
  pub enemies: Vec<u32>,
  pub can_target: bool,
  pub pause_iges: bool,
  pub force_pause_iges: bool,
  pub key_queue: Vec<SelfKeyEvent>,
  pub frame_queue: Vec<Value>,
  pub message_queue: Vec<Value>,
  pub ige_queue: Vec<Value>,
  pub incoming_ige_frames: Vec<ReplayFrame>,
  pub target: TargetStrategy,
  pub start_time: Option<Instant>,
  pub last_ige_flush: Instant,
}

impl Self_ {
  pub fn new(self_player: &RawPlayer, all_players: &[RawPlayer]) -> Self {
    let engine = create_engine(&self_player.options, self_player.gameid, all_players);
    Self_ {
      gameid: self_player.gameid,
      engine,
      options: self_player.options.clone(),
      server_targets: Vec::new(),
      enemies: Vec::new(),
      can_target: true,
      pause_iges: false,
      force_pause_iges: false,
      key_queue: Vec::new(),
      frame_queue: Vec::new(),
      message_queue: Vec::new(),
      ige_queue: Vec::new(),
      incoming_ige_frames: Vec::new(),
      target: TargetStrategy::Even,
      start_time: None,
      last_ige_flush: Instant::now(),
    }
  }

  pub fn init(&mut self) {
    self.start_time = Some(Instant::now());
    self.frame_queue.push(get_full_frame(&self.options));
  }

  pub fn queue_key(&mut self, event: SelfKeyEvent) {
    if event.frame >= self.engine.frame as f64 {
      self.key_queue.push(event);
    }
  }

  pub fn queue_ige(&mut self, ige: Value) {
    self.ige_queue.push(ige);
    self.flush_iges();
  }

  pub fn set_target(&mut self, target: TargetStrategy) -> Result<()> {
    if !self.can_target {
      return Err(crate::error::TriangleError::InvalidArgument(
        "targeting not allowed".to_string(),
      ));
    }

    let frame = self.engine.frame;
    let replay = match target {
      TargetStrategy::Manual(id) => {
        serde_json::json!({ "type": "manual_target", "frame": frame, "data": id })
      }
      TargetStrategy::Even => {
        serde_json::json!({ "type": "strategy", "frame": frame, "data": 0 })
      }
      TargetStrategy::Elims => {
        serde_json::json!({ "type": "strategy", "frame": frame, "data": 1 })
      }
      TargetStrategy::Random => {
        serde_json::json!({ "type": "strategy", "frame": frame, "data": 2 })
      }
      TargetStrategy::Payback => {
        serde_json::json!({ "type": "strategy", "frame": frame, "data": 3 })
      }
    };

    self.target = target;
    self.frame_queue.push(replay);
    Ok(())
  }

  pub fn tick(&mut self) -> Value {
    self.flush_iges();

    let mut replay_frames = self
      .incoming_ige_frames
      .drain(..)
      .collect::<Vec<ReplayFrame>>();

    let mut keys_this_frame = Vec::new();
    for idx in (0..self.key_queue.len()).rev() {
      let key = &self.key_queue[idx];
      if key.frame.floor() as i32 == self.engine.frame {
        keys_this_frame.push(self.key_queue.remove(idx));
      }
    }
    keys_this_frame.reverse();

    for key in &keys_this_frame {
      let frame = match key.event_type {
        SelfKeyEventType::Keydown => ReplayFrame::Keydown(KeyEvent {
          subframe: key.subframe,
          key: key.key.clone(),
          hoisted: false,
        }),
        SelfKeyEventType::Keyup => ReplayFrame::Keyup(KeyEvent {
          subframe: key.subframe,
          key: key.key.clone(),
          hoisted: false,
        }),
      };
      replay_frames.push(frame);
      self.frame_queue.push(serde_json::json!({
                "type": match key.event_type { SelfKeyEventType::Keydown => "keydown", SelfKeyEventType::Keyup => "keyup" },
                "frame": self.engine.frame,
                "data": { "key": key.key, "subframe": key.subframe }
            }));
    }

    let res = self.engine.tick(&replay_frames);

    for g in res.garbage_received {
      self.message_queue.push(serde_json::json!({
          "type": "garbage",
          "frame": g.frame,
          "amount": g.amount,
          "size": g.size,
          "id": g.id,
          "column": g.column
      }));
    }

    let provisioned = self.engine.frame;
    if provisioned != 0 && provisioned % 12 == 0 {
      let mut frames: Vec<Value> = self
        .frame_queue
        .drain(..)
        .filter(|f| f["frame"].as_i64().unwrap_or(0) <= provisioned as i64)
        .collect();

      if let Some(full_idx) = frames
        .iter()
        .position(|f| f["type"].as_str() == Some("full"))
      {
        let full = frames.remove(full_idx);
        frames.insert(0, full);
      }

      if let Some(start_idx) = frames
        .iter()
        .position(|f| f["type"].as_str() == Some("start"))
      {
        let start = frames.remove(start_idx);
        frames.insert(0, start);
      }

      let frameset = serde_json::json!({
          "gameid": self.gameid,
          "provisioned": provisioned,
          "frames": frames
      });

      self.message_queue.push(serde_json::json!({
          "type": "frameset",
          "provisioned": provisioned,
          "frames": frameset["frames"].clone()
      }));

      return frameset;
    }

    serde_json::json!({
        "gameid": self.gameid,
        "provisioned": provisioned,
        "frames": []
    })
  }

  fn iges_paused(&self) -> bool {
    self.force_pause_iges || (self.pause_iges && !self.key_queue.is_empty())
  }

  fn flush_iges(&mut self) {
    if self.iges_paused() && self.last_ige_flush.elapsed() < Duration::from_secs(30) {
      return;
    }

    let iges = self.ige_queue.drain(..).collect::<Vec<Value>>();
    for ige in iges {
      if let Some(frame) = parse_ige_data(&serde_json::json!({ "data": ige }), self.engine.subframe)
      {
        if let IgeData::Target { targets } = &frame.data {
          self.server_targets = targets.iter().map(|x| *x as u32).collect();
        }
        self.incoming_ige_frames.push(ReplayFrame::Ige(frame));
      }
    }

    self.last_ige_flush = Instant::now();
  }
}

fn get_full_frame(options: &GameOptions) -> Value {
  let boardwidth = options.boardwidth.unwrap_or(10) as usize;
  let boardheight = options.boardheight.unwrap_or(20) as usize;
  serde_json::json!({
      "type": "full",
      "frame": 0,
      "data": {
          "game": {
              "board": (0..boardheight + 20).map(|_| (0..boardwidth).map(|_| Value::Null).collect::<Vec<Value>>()).collect::<Vec<Vec<Value>>>(),
              "g": options.g.unwrap_or(0.02),
              "playing": true
          }
      }
  })
}

/// Manages live game spectation / participation.
pub struct Game {
  /// The players in the game (opponents being spectated).
  pub players: Vec<Player>,
  /// The raw player list from the server.
  pub raw_players: Vec<RawPlayer>,
  /// The client's own game state (if the client is a participant).
  pub self_: Option<Self_>,
  /// The spectating strategy for opponent renders.
  pub spectating_strategy: SpectatingStrategy,
  emitter: Arc<EventEmitter>,
  _handle: tokio::task::JoinHandle<()>,
}

impl Game {
  /// Create from a `game.ready` payload.
  pub fn new(
    emitter: Arc<EventEmitter>,
    raw_players: Vec<RawPlayer>,
    self_userid: &str,
    spectating_strategy: SpectatingStrategy,
  ) -> Self {
    let self_ = raw_players
      .iter()
      .find(|p| p.userid == self_userid)
      .map(|p| Self_::new(p, &raw_players));

    let players: Vec<Player> = raw_players
      .iter()
      .filter(|p| p.userid != self_userid)
      .map(|p| Player::new(p, &raw_players))
      .collect();

    let emitter_c = emitter.clone();
    let handle = tokio::spawn(async move {
      let interval = tokio::time::Duration::from_micros((1_000_000.0 / FPS) as u64);
      let mut tick = tokio::time::interval(interval);
      loop {
        tick.tick().await;
        emitter_c.emit("game.tick", Value::Null);
      }
    });

    Self {
      players,
      raw_players,
      self_,
      spectating_strategy,
      emitter,
      _handle: handle,
    }
  }

  /// Get the opponents (players who are not the client).
  pub fn opponents<'a>(&'a self, self_userid: &str) -> Vec<&'a Player> {
    self
      .players
      .iter()
      .filter(|p| p.userid != self_userid)
      .collect()
  }

  /// Tick all players (process queued replay frames).
  pub fn tick(&mut self) {
    let strategy = self.spectating_strategy.clone();
    for player in &mut self.players {
      player.tick(&strategy);
    }
  }

  /// Deliver a `game.replay` event frame to the relevant player(s).
  pub fn deliver_replay_frame(&mut self, gameid: u32, frame: Value) {
    if let Some(player) = self.players.iter_mut().find(|p| p.gameid == gameid) {
      player.push_frame(frame);
    }
  }

  /// Mark a player as actively being spectated.
  pub fn set_player_spectating(&mut self, gameid: u32, active: bool) {
    if let Some(player) = self.players.iter_mut().find(|p| p.gameid == gameid) {
      player.state = if active {
        SpectatingState::Active
      } else {
        SpectatingState::Inactive
      };
    }
  }

  /// Request spectation of specific targets by gameid.
  pub fn spectate(&self, game_ids: &[u32]) {
    let ids: Vec<Value> = game_ids
      .iter()
      .map(|id| Value::Number((*id).into()))
      .collect();
    self
      .emitter
      .emit("game.scope.start", serde_json::json!({ "scopes": ids }));
  }

  /// Request spectation by user ids.
  pub fn spectate_userids(&self, user_ids: &[String]) {
    let ids: Vec<u32> = self
      .players
      .iter()
      .filter(|p| user_ids.iter().any(|uid| uid == &p.userid))
      .map(|p| p.gameid)
      .collect();
    self.spectate(&ids);
  }

  /// Request to spectate all players.
  pub fn spectate_all(&self) {
    let ids: Vec<Value> = self
      .players
      .iter()
      .map(|p| Value::Number(p.gameid.into()))
      .collect();
    self
      .emitter
      .emit("game.scope.start", serde_json::json!({ "scopes": ids }));
  }

  /// Stop spectating specific targets.
  pub fn unspectate(&self, game_ids: &[u32]) {
    let ids: Vec<Value> = game_ids
      .iter()
      .map(|id| Value::Number((*id).into()))
      .collect();
    self
      .emitter
      .emit("game.scope.end", serde_json::json!({ "scopes": ids }));
  }

  /// Stop spectating by user ids.
  pub fn unspectate_userids(&self, user_ids: &[String]) {
    let ids: Vec<u32> = self
      .players
      .iter()
      .filter(|p| user_ids.iter().any(|uid| uid == &p.userid))
      .map(|p| p.gameid)
      .collect();
    self.unspectate(&ids);
  }

  /// Stop spectating all players.
  pub fn unspectate_all(&self) {
    let ids: Vec<Value> = self
      .players
      .iter()
      .map(|p| Value::Number(p.gameid.into()))
      .collect();
    self
      .emitter
      .emit("game.scope.end", serde_json::json!({ "scopes": ids }));
  }
}

/// Build an engine from TETR.IO game options.
pub fn create_engine(options: &GameOptions, gameid: u32, all_players: &[RawPlayer]) -> Engine {
  let board_width = options.boardwidth.unwrap_or(10) as usize;
  let board_height = options.boardheight.unwrap_or(20) as usize;

  let bag_type: BagType = options
    .bagtype
    .as_deref()
    .and_then(|b| match b {
      "7-bag" | "bag7" => Some(BagType::Bag7),
      "14-bag" | "bag14" => Some(BagType::Bag14),
      "classic" => Some(BagType::Classic),
      "pairs" => Some(BagType::Pairs),
      "total mayhem" => Some(BagType::TotalMayhem),
      "7+1" => Some(BagType::Bag7Plus1),
      "7+2" => Some(BagType::Bag7Plus2),
      "7+X" => Some(BagType::Bag7PlusX),
      _ => None,
    })
    .unwrap_or(BagType::Bag7);

  let seed = options.seed.map(|s| s as i64).unwrap_or(0);

  let passthrough_str = match options.passthrough.as_ref() {
    Some(crate::types::game::Passthrough::Zero) => "zero",
    Some(crate::types::game::Passthrough::Limited) => "limited",
    Some(crate::types::game::Passthrough::Consistent) => "consistent",
    Some(crate::types::game::Passthrough::Full) => "full",
    None => "zero",
  };

  let opponents: Vec<i32> = all_players
    .iter()
    .filter(|p| p.gameid != gameid)
    .map(|p| p.gameid as i32)
    .collect();

  let rounding = match options.roundmode.as_ref() {
    Some(RoundingMode::Down) => GarbageRoundingMode::Down,
    _ => GarbageRoundingMode::Rng,
  };

  let combo_table_str = match options.combotable.as_ref() {
    Some(crate::types::game::ComboTable::None) => "none",
    Some(crate::types::game::ComboTable::Multiplier) => "multiplier",
    Some(crate::types::game::ComboTable::ClassicGuideline) => "classic guideline",
    Some(crate::types::game::ComboTable::ModernGuideline) => "modern guideline",
    None => "multiplier",
  };

  let spin_bonuses_str = match options.spinbonuses.as_ref() {
    Some(crate::types::game::SpinBonuses::TSpins) => "T-spins",
    Some(crate::types::game::SpinBonuses::TSpinsPlus) => "T-spins+",
    Some(crate::types::game::SpinBonuses::All) => "all",
    Some(crate::types::game::SpinBonuses::AllPlus) => "all+",
    Some(crate::types::game::SpinBonuses::AllMini) => "all-mini",
    Some(crate::types::game::SpinBonuses::AllMiniPlus) => "all-mini+",
    Some(crate::types::game::SpinBonuses::MiniOnly) => "mini-only",
    Some(crate::types::game::SpinBonuses::Handheld) => "handheld",
    Some(crate::types::game::SpinBonuses::Stupid) => "stupid",
    Some(crate::types::game::SpinBonuses::None) => "none",
    None => "T-spins",
  };

  let garbage_blocking_str = match options.garbageblocking.as_ref() {
    Some(crate::types::game::GarbageBlocking::ComboBlocking) => "combo blocking",
    Some(crate::types::game::GarbageBlocking::LimitedBlocking) => "limited blocking",
    Some(crate::types::game::GarbageBlocking::None) => "none",
    None => "combo blocking",
  };

  let garbage_target_bonus_str = match options.garbagetargetbonus.as_ref() {
    Some(crate::types::game::GarbageTargetBonus::Offensive) => "offensive",
    Some(crate::types::game::GarbageTargetBonus::Defensive) => "defensive",
    _ => "none",
  };

  let h = options.handling.clone().unwrap_or_default();

  let handling = HandlingOptions {
    arr: h.arr,
    das: h.das,
    dcd: h.dcd,
    sdf: h.sdf,
    safelock: h.safelock,
    cancel: h.cancel,
    may20g: h.may20g,
    irs: h.irs,
    ihs: h.ihs,
  };

  let pc_opts = if options.allclears.unwrap_or(false) {
    Some(PcOptions {
      garbage: options.allclear_garbage.unwrap_or(10) as f64,
      b2b: options.allclear_b2b.unwrap_or(0) as i32,
    })
  } else {
    None
  };

  let b2b_charging = if options.b2bcharging.unwrap_or(false) {
    Some(B2bCharging {
      at: options.b2bcharge_at.unwrap_or(0) as i32,
      base: options.b2bcharge_base.unwrap_or(0) as i32,
    })
  } else {
    None
  };

  let params = EngineInitParams {
    queue: QueueInitParams {
      seed,
      kind: bag_type,
      min_length: 31,
    },
    board: BoardInitParams {
      width: board_width,
      height: board_height,
      buffer: 20,
    },
    kick_table: options
      .kickset
      .clone()
      .unwrap_or_else(|| "SRS+".to_string()),
    options: EngineGameOptions {
      spin_bonuses: spin_bonuses_str.to_string(),
      combo_table: combo_table_str.to_string(),
      garbage_target_bonus: garbage_target_bonus_str.to_string(),
      clutch: options.clutch.unwrap_or(true),
      garbage_blocking: garbage_blocking_str.to_string(),
      stock: options.stock.unwrap_or(0) as i32,
    },
    gravity: IncreasableValue {
      value: options.g.unwrap_or(0.02),
      increase: options.gincrease.unwrap_or(0.0),
      margin_time: options.gmargin.unwrap_or(0.0) as i32,
    },
    garbage: GarbageQueueInitParams {
      cap: GarbageCapParams {
        value: options.garbagecap.unwrap_or(8.0),
        margin_time: options.garbagecapmargin.unwrap_or(0.0) as i32,
        increase: options.garbagecapincrease.unwrap_or(0.0),
        absolute: options
          .garbageabsolutecap
          .unwrap_or(i32::MAX as f64)
          .min(i32::MAX as f64) as i32,
        max: options.garbagecapmax.unwrap_or(40.0),
      },
      messiness: MessinessParams {
        change: options.messiness_change.unwrap_or(0.0),
        within: options.messiness_inner.unwrap_or(0.0),
        nosame: options.messiness_nosame.unwrap_or(false),
        timeout: options.messiness_timeout.unwrap_or(0.0) as i32,
        center: false,
      },
      garbage: GarbageSpeedParams {
        speed: options.garbagespeed.unwrap_or(20.0) as i32,
        hole_size: options.garbageholesize.unwrap_or(1) as usize,
      },
      multiplier: MultiplierParams {
        value: options.garbagemultiplier.unwrap_or(1.0),
        increase: options.garbageincrease.unwrap_or(0.0),
        margin_time: options.garbagemargin.unwrap_or(0.0) as i32,
      },
      bombs: options.usebombs.unwrap_or(false),
      seed,
      board_width,
      rounding,
      opener_phase: options.openerphase.unwrap_or(0) as i32,
      special_bonus: options.garbagespecialbonus.unwrap_or(false),
    },
    handling,
    pc: pc_opts,
    b2b: B2bOptions {
      chaining: options.b2bchaining.unwrap_or(true),
      charging: b2b_charging,
    },
    multiplayer: if !opponents.is_empty() {
      Some(MultiplayerOptions {
        opponents,
        passthrough: passthrough_str.to_string(),
      })
    } else {
      None
    },
    misc: MiscOptions {
      movement: MovementOptions {
        infinite: options.infinite_movement.unwrap_or(false),
        lock_resets: options.lockresets.unwrap_or(15) as i32,
        lock_time: options.locktime.unwrap_or(30) as f64,
        may_20g: false,
      },
      allowed: AllowedOptions {
        spin180: options.allow180.unwrap_or(true),
        hard_drop: options.allow_harddrop.unwrap_or(true),
        hold: options.display_hold.unwrap_or(true),
        undo: options.can_undo.unwrap_or(false),
        retry: options.can_retry.unwrap_or(false),
      },
      infinite_hold: options.infinite_hold.unwrap_or(false),
      stride: options.stride.unwrap_or(false),
      username: options.username.clone(),
      date: Some(chrono::Utc::now()),
    },
  };

  Engine::new(params)
}

/// Parse raw players from a `game.ready` or `game.spectate` JSON payload.
pub fn parse_raw_players(data: &Value) -> Vec<RawPlayer> {
  let players_arr = match data["players"].as_array() {
    Some(a) => a,
    None => return Vec::new(),
  };

  players_arr
    .iter()
    .filter_map(|p| {
      let gameid = p["gameid"].as_u64()? as u32;
      let userid = p["userid"].as_str()?.to_string();
      let options: GameOptions = serde_json::from_value(p["options"].clone()).unwrap_or_default();
      Some(RawPlayer {
        gameid,
        userid,
        options,
      })
    })
    .collect()
}
