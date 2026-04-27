use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{Mutex, oneshot};
use tokio::time::Instant as TokioInstant;

use crate::{
  engine::{
    AllowedOptions, B2bCharging, B2bOptions, Engine, EngineInitParams, EngineSnapshot, EngineStats,
    GameOptions as EngineGameOptions, HandlingOptions, IgeData, IgeFrame, IncreasableValue,
    InputKeys, InputState, InputTime, KeyEvent, MiscOptions, MovementOptions, MultiplayerOptions,
    PcOptions, PracticeState, ReplayFrame, ResCache, ShiftState, SpikeState,
    board::{BoardInitParams, CONN_ALL, Tile},
    garbage::{
      GarbageCapParams, GarbageQueueInitParams, GarbageQueueSnapshot, GarbageSpeedParams,
      IncomingGarbage, MessinessParams, MultiplierParams, OutgoingGarbage,
      RoundingMode as GarbageRoundingMode,
    },
    multiplayer::{GarbageRecord, IgeHandlerSnapshot, PlayerData},
    queue::{
      QueueInitParams, QueueSnapshot,
      bag::{BagSnapshot, BagType},
      types::Mino,
    },
    utils::{damage_calc::SpinType, tetromino::TetrominoSnapshot},
  },
  error::Result,
  types::{
    events::wrapper::{
      ClientGameOver, ClientGameRoundStart, GameClientEvent, GameOverKind, GameReplayFrame,
      RawTickFn, TickInput, TickKeypress, TickKeypressKind, TickOutput, TickSetter,
    },
    game::{Options as GameOptions, RoundingMode},
  },
  utils::EventEmitter,
};

pub const FPS: f64 = 60.0;
const FPM: i32 = 12;
const MAX_IGE_TIMEOUT_MS: u128 = 30_000;

pub type RoundStartHandler = Arc<dyn Fn(TickSetter, Arc<Mutex<Engine>>) + Send + Sync>;
pub type RoundStartHandlers = Arc<Mutex<Vec<RoundStartHandler>>>;

// ── Enums / simple structs ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TargetStrategy {
  Even,
  Elims,
  Random,
  Payback,
  Manual(u32),
}

impl Default for TargetStrategy {
  fn default() -> Self {
    Self::Even
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectatingStrategy {
  Smooth,
  Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectatingState {
  Inactive,
  Waiting,
  Active,
}

#[derive(Debug, Clone)]
pub struct RawPlayer {
  pub gameid: u32,
  pub userid: String,
  pub options: GameOptions,
}

// ── Player ────────────────────────────────────────────────────────────────────

struct PlayerInner {
  queue: Vec<GameReplayFrame>,
  state: SpectatingState,
  resolvers: Vec<oneshot::Sender<()>>,
  topped_out: bool,
}

pub struct Player {
  pub name: String,
  pub gameid: u32,
  pub userid: String,
  pub engine: Arc<Mutex<Engine>>,
  inner: Arc<Mutex<PlayerInner>>,
  strategy: Arc<Mutex<SpectatingStrategy>>,
  _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Player {
  pub(crate) fn new(
    raw: &RawPlayer,
    all_players: &[RawPlayer],
    emitter: Arc<EventEmitter>,
    strategy: SpectatingStrategy,
  ) -> Self {
    let engine = Arc::new(Mutex::new(create_engine(
      &raw.options,
      raw.gameid,
      all_players,
    )));
    let inner = Arc::new(Mutex::new(PlayerInner {
      queue: Vec::new(),
      state: SpectatingState::Inactive,
      resolvers: Vec::new(),
      topped_out: false,
    }));
    let strategy_arc = Arc::new(Mutex::new(strategy));
    let gameid = raw.gameid;
    let init_c = engine.blocking_lock().initializer.clone();
    let mut handles = Vec::new();

    // game.replay.state: apply server-side snapshot
    {
      let inner_c = inner.clone();
      let engine_c = engine.clone();
      let handle = emitter.on("game.replay.state", move |data: Value| {
        let ev_gameid = data["gameid"].as_u64().unwrap_or(0) as u32;
        if ev_gameid != gameid {
          return;
        }
        let inner_c = inner_c.clone();
        let engine_c = engine_c.clone();
        let init_c = init_c.clone();
        tokio::spawn(async move {
          let mut lock = inner_c.lock().await;
          let state_data = &data["data"];
          let frame = state_data["frame"].as_i64().unwrap_or(0) as i32;
          let game_val = &state_data["game"];

          for tx in lock.resolvers.drain(..) {
            let _ = tx.send(());
          }
          lock.state = SpectatingState::Active;

          if !game_val.is_null() && game_val.is_object() {
            let snap = snapshot_from_state(frame, &init_c, game_val);
            let mut eng = engine_c.lock().await;
            eng.from_snapshot(&snap, false);
          }
        });
      });
      handles.push(handle);
    }

    // game.replay: queue incoming frames
    {
      let inner_c = inner.clone();
      let handle = emitter.on("game.replay", move |data: Value| {
        let ev_gameid = data["gameid"].as_u64().unwrap_or(0) as u32;
        if ev_gameid != gameid {
          return;
        }
        let frames_arr = data["frames"].as_array().cloned().unwrap_or_default();
        let frames: Vec<GameReplayFrame> = frames_arr
          .iter()
          .filter_map(|f| serde_json::from_value(f.clone()).ok())
          .collect();
        let inner_c = inner_c.clone();
        tokio::spawn(async move {
          let mut lock = inner_c.lock().await;
          if lock.state != SpectatingState::Active || lock.topped_out {
            return;
          }
          lock.queue.extend(frames);
        });
      });
      handles.push(handle);
    }

    Player {
      name: raw.options.username.clone().unwrap_or_default(),
      gameid: raw.gameid,
      userid: raw.userid.clone(),
      engine,
      inner,
      strategy: strategy_arc,
      _handles: handles,
    }
  }

  pub async fn spectate(&self, emitter: &Arc<EventEmitter>) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    let mut lock = self.inner.lock().await;
    if lock.state == SpectatingState::Active {
      let _ = tx.send(());
      return rx;
    }
    lock.resolvers.push(tx);
    if lock.state == SpectatingState::Inactive {
      lock.state = SpectatingState::Waiting;
      emitter.emit("game.scope.start", serde_json::json!(self.gameid));
    }
    rx
  }

  pub async fn unspectate(&self, emitter: &Arc<EventEmitter>) {
    let mut lock = self.inner.lock().await;
    if lock.state == SpectatingState::Inactive {
      return;
    }
    emitter.emit("game.scope.end", serde_json::json!(self.gameid));
    lock.state = SpectatingState::Inactive;
    lock.queue.clear();
  }

  pub(crate) async fn _tick(&self) {
    let mut lock = self.inner.lock().await;
    if lock.state != SpectatingState::Active || lock.topped_out {
      return;
    }
    let strategy = self.strategy.lock().await.clone();
    let mut eng = self.engine.lock().await;

    let tick_once = |queue: &mut Vec<GameReplayFrame>, eng: &mut Engine| {
      let engine_frame = eng.frame;
      let mut frames: Vec<ReplayFrame> = Vec::new();
      while queue
        .first()
        .map(|f| f.frame <= engine_frame)
        .unwrap_or(false)
      {
        let f = queue.remove(0);
        if let Some(rf) = grf_to_replay_frame(&f) {
          frames.push(rf);
        }
      }
      let res = eng.tick(&frames);
      let topped = eng.events.iter().any(|e| {
        if let crate::engine::EngineEvent::FallingLock(lr) = e {
          lr.topout
        } else {
          false
        }
      });
      topped
    };

    match strategy {
      SpectatingStrategy::Instant => {
        while lock.queue.iter().any(|f| f.frame > eng.frame) {
          if tick_once(&mut lock.queue, &mut eng) {
            lock.topped_out = true;
            break;
          }
        }
      }
      SpectatingStrategy::Smooth => {
        if lock.queue.is_empty() {
          return;
        }
        let last_frame = lock.queue.last().map(|f| f.frame).unwrap_or(0);
        while lock.queue.iter().any(|f| f.frame > eng.frame) && eng.frame < last_frame - 20 {
          if tick_once(&mut lock.queue, &mut eng) {
            lock.topped_out = true;
            return;
          }
        }
        if lock.queue.iter().any(|f| f.frame > eng.frame) {
          tick_once(&mut lock.queue, &mut eng);
        }
      }
    }
  }

  pub async fn set_strategy(&self, strategy: SpectatingStrategy) {
    *self.strategy.lock().await = strategy;
  }
}

fn grf_to_replay_frame(f: &GameReplayFrame) -> Option<ReplayFrame> {
  let subframe = f.data["subframe"].as_f64().unwrap_or(0.0);
  match f.kind.as_str() {
    "keydown" => {
      let key = f.data["key"].as_str().unwrap_or("").to_string();
      Some(ReplayFrame::Keydown(KeyEvent {
        subframe,
        key,
        hoisted: false,
      }))
    }
    "keyup" => {
      let key = f.data["key"].as_str().unwrap_or("").to_string();
      Some(ReplayFrame::Keyup(KeyEvent {
        subframe,
        key,
        hoisted: false,
      }))
    }
    "ige" => parse_ige_frame(&f.data, subframe).map(ReplayFrame::Ige),
    _ => None,
  }
}

// ── Self_ ─────────────────────────────────────────────────────────────────────

pub struct Self_ {
  pub gameid: u32,
  pub engine: Arc<Mutex<Engine>>,
  pub options: GameOptions,
  pub can_target: Arc<Mutex<bool>>,
  pub server_targets: Arc<Mutex<Vec<u32>>>,
  pub enemies: Arc<Mutex<Vec<u32>>>,
  pub target: Arc<Mutex<TargetStrategy>>,
  pub pause_iges: Arc<Mutex<bool>>,
  pub force_pause_iges: Arc<Mutex<bool>>,
  pub key_queue: Arc<Mutex<Vec<TickKeypress>>>,
  ige_queue: Arc<Mutex<Vec<Value>>>,
  tick_fn: Arc<Mutex<Option<RawTickFn>>>,
  send_fn: Arc<dyn Fn(String, Value) + Send + Sync>,
  emitter: Arc<EventEmitter>,
  round_start_handlers: RoundStartHandlers,
  _handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Self_ {
  pub(crate) fn new(
    raw: &RawPlayer,
    all_players: &[RawPlayer],
    send_fn: Arc<dyn Fn(String, Value) + Send + Sync>,
    emitter: Arc<EventEmitter>,
    round_start_handlers: RoundStartHandlers,
  ) -> Self {
    let engine = Arc::new(Mutex::new(create_engine(
      &raw.options,
      raw.gameid,
      all_players,
    )));
    let gameid = raw.gameid;
    let ige_queue: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let can_target = Arc::new(Mutex::new(true));
    let server_targets: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    // game.replay.ige: collect server IGEs
    {
      let ige_queue_c = ige_queue.clone();
      let can_target_c = can_target.clone();
      let server_targets_c = server_targets.clone();
      let handle = emitter.on("game.replay.ige", move |data: Value| {
        let ev_gameid = data["gameid"].as_u64().unwrap_or(0) as u32;
        if ev_gameid != gameid {
          return;
        }
        let iges = data["iges"].as_array().cloned().unwrap_or_default();
        let ige_queue_c = ige_queue_c.clone();
        let can_target_c = can_target_c.clone();
        let server_targets_c = server_targets_c.clone();
        tokio::spawn(async move {
          let mut q = ige_queue_c.lock().await;
          for ige in &iges {
            match ige["type"].as_str().unwrap_or("") {
              "allow_targeting" => {
                *can_target_c.lock().await = ige["data"]["value"].as_bool().unwrap_or(true);
              }
              "target" => {
                let targets: Vec<u32> = ige["data"]["targets"]
                  .as_array()
                  .map(|a| {
                    a.iter()
                      .filter_map(|v| v.as_u64().map(|n| n as u32))
                      .collect()
                  })
                  .unwrap_or_default();
                *server_targets_c.lock().await = targets;
              }
              _ => {}
            }
            q.push(ige.clone());
          }
        });
      });
      handles.push(handle);
    }

    Self_ {
      gameid,
      engine,
      options: raw.options.clone(),
      can_target,
      server_targets,
      enemies: Arc::new(Mutex::new(Vec::new())),
      target: Arc::new(Mutex::new(TargetStrategy::Even)),
      pause_iges: Arc::new(Mutex::new(false)),
      force_pause_iges: Arc::new(Mutex::new(false)),
      key_queue: Arc::new(Mutex::new(Vec::new())),
      ige_queue,
      tick_fn: Arc::new(Mutex::new(None)),
      send_fn,
      emitter,
      round_start_handlers,
      _handles: handles,
    }
  }

  pub(crate) fn init(&self) {
    let engine = self.engine.clone();
    let tick_fn = self.tick_fn.clone();
    let send_fn = self.send_fn.clone();
    let ige_queue = self.ige_queue.clone();
    let key_queue = self.key_queue.clone();
    let can_target = self.can_target.clone();
    let server_targets = self.server_targets.clone();
    let enemies = self.enemies.clone();
    let target = self.target.clone();
    let pause_iges = self.pause_iges.clone();
    let force_pause_iges = self.force_pause_iges.clone();
    let round_start_handlers = self.round_start_handlers.clone();
    let emitter = self.emitter.clone();
    let options = self.options.clone();
    let gameid = self.gameid;

    let emitter_c = emitter.clone();
    let handle = emitter.on("game.start", move |_: Value| {
      let engine = engine.clone();
      let tick_fn = tick_fn.clone();
      let send_fn = send_fn.clone();
      let ige_queue = ige_queue.clone();
      let key_queue = key_queue.clone();
      let can_target = can_target.clone();
      let server_targets = server_targets.clone();
      let enemies = enemies.clone();
      let target = target.clone();
      let pause_iges = pause_iges.clone();
      let force_pause_iges = force_pause_iges.clone();
      let round_start_handlers = round_start_handlers.clone();
      let emitter = emitter_c.clone();
      let options = options.clone();
      tokio::spawn(async move {
        let delay_ms = options.countdown_count.unwrap_or(5) as u64
          * options.countdown_interval.unwrap_or(600.0) as u64
          + options.precountdown.unwrap_or(750.0) as u64
          + options.prestart.unwrap_or(0.0) as u64;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        // notify round-start handlers
        let setter = TickSetter::new(tick_fn.clone());
        let handlers = round_start_handlers.lock().await.clone();
        for h in &handlers { h(setter.clone(), engine.clone()); }
        emitter.emit("client.game.round.start", serde_json::json!("round_start"));

        let mut frame_queue: Vec<Value> = Vec::new();
        let mut message_queue: Vec<GameClientEvent> = Vec::new();
        let mut incoming_ige_frames: Vec<ReplayFrame> = Vec::new();
        let start_time = TokioInstant::now();
        let mut last_ige_flush = TokioInstant::now();
        let mut slow_tick_warned = false;
        let mut over = false;

        frame_queue.push(serde_json::json!({ "type": "start", "frame": 0, "data": {} }));
        frame_queue.push(make_full_frame(&options));

        loop {
          if over { break; }

          // ── User tick ──────────────────────────────────────────────────
          let snapshot = { engine.lock().await.snapshot(false) };
          let engine_frame = { engine.lock().await.frame };
          let events: Vec<GameClientEvent> = message_queue.drain(..).collect();
          let c_can_target = *can_target.lock().await;
          let c_server_targets = server_targets.lock().await.clone();
          let c_enemies = enemies.lock().await.clone();

          let tick_input = TickInput {
            gameid,
            frame: engine_frame,
            events,
            engine: engine.clone(),
            can_target: c_can_target,
            server_targets: c_server_targets,
            enemies: c_enemies,
            key_queue: key_queue.clone(),
            target: target.clone(),
            pause_iges: pause_iges.clone(),
            force_pause_iges: force_pause_iges.clone(),
          };

          let out = {
            let guard = tick_fn.lock().await;
            if let Some(f) = guard.as_ref() {
              let fut = f(tick_input);
              drop(guard);
              fut.await
            } else {
              drop(guard);
              TickOutput::default()
            }
          };

          // restore engine (user may have mutated)
          { engine.lock().await.from_snapshot(&snapshot, false); }

          let run_after = out.run_after.unwrap_or_default();
          if let Some(keys) = out.keys {
            let mut kq = key_queue.lock().await;
            for k in keys {
              if is_valid_key(&k, engine_frame) { kq.push(k); }
            }
          }

          if over { break; }

          // ── First IGE flush ────────────────────────────────────────────
          {
            let kq_len = key_queue.lock().await.len();
            let p = *pause_iges.lock().await;
            let fp = *force_pause_iges.lock().await;
            let subframe = engine.lock().await.subframe;
            do_flush_iges(&ige_queue, &mut incoming_ige_frames, subframe, p, fp, kq_len, &mut last_ige_flush).await;
          }

          // ── Collect keys for this frame ────────────────────────────────
          let engine_frame = engine.lock().await.frame;
          let frame_keys: Vec<TickKeypress> = {
            let mut kq = key_queue.lock().await;
            let mut out_keys = Vec::new();
            let mut i = kq.len();
            while i > 0 {
              i -= 1;
              if kq[i].frame.floor() as i32 == engine_frame {
                out_keys.push(kq.remove(i));
              }
            }
            out_keys.reverse();
            out_keys
          };

          // ── Engine tick ────────────────────────────────────────────────
          let ige_frames: Vec<ReplayFrame> = incoming_ige_frames.drain(..).collect();
          let mut replay_frames: Vec<ReplayFrame> = ige_frames;
          for k in &frame_keys {
            let event = KeyEvent { subframe: k.data.subframe, key: k.data.key.clone(), hoisted: false };
            replay_frames.push(match k.kind { TickKeypressKind::Keydown => ReplayFrame::Keydown(event), TickKeypressKind::Keyup => ReplayFrame::Keyup(event) });
          }

          let res = { engine.lock().await.tick(&replay_frames) };

          // topout?
          {
            let eng = engine.lock().await;
            let topped = eng.events.iter().any(|e| {
              if let crate::engine::EngineEvent::FallingLock(lr) = e { lr.topout } else { false }
            });
            if topped { over = true; }
          }

          // push garbage events
          for g in &res.garbage_received {
            message_queue.push(GameClientEvent::Garbage { frame: g.frame, amount: g.amount, size: g.size, id: g.id, column: g.column });
          }

          // add keys to frame_queue
          for k in &frame_keys {
            let kind_str = match k.kind { TickKeypressKind::Keydown => "keydown", TickKeypressKind::Keyup => "keyup" };
            frame_queue.push(serde_json::json!({ "type": kind_str, "frame": engine_frame, "data": { "key": k.data.key, "subframe": k.data.subframe } }));
          }

          let engine_frame_post = engine.lock().await.frame;

          // ── Every FPM frames: send game.replay ────────────────────────
          if engine_frame_post != 0 && engine_frame_post % FPM == 0 {
            let manual_allowed = options.manual_allowed.unwrap_or(true);
            let c_can = *can_target.lock().await;
            let frames = flush_frame_queue(&mut frame_queue, engine_frame_post, c_can, manual_allowed);
            if !frames.is_empty() {
              send_fn("game.replay".to_string(), serde_json::json!({ "gameid": gameid, "provisioned": engine_frame_post, "frames": frames.clone() }));
              let grf_frames: Vec<GameReplayFrame> = frames.into_iter().filter_map(|f| serde_json::from_value(f).ok()).collect();
              message_queue.push(GameClientEvent::Frameset { provisioned: engine_frame_post, frames: grf_frames });
            }
          }

          // ── run_after ──────────────────────────────────────────────────
          for f in &run_after { f(); }

          // ── Second IGE flush ───────────────────────────────────────────
          {
            let kq_len = key_queue.lock().await.len();
            let p = *pause_iges.lock().await;
            let fp = *force_pause_iges.lock().await;
            let subframe = engine.lock().await.subframe;
            do_flush_iges(&ige_queue, &mut incoming_ige_frames, subframe, p, fp, kq_len, &mut last_ige_flush).await;
          }

          if over { break; }

          // ── Timing ────────────────────────────────────────────────────
          let elapsed_ms = start_time.elapsed().as_millis() as f64;
          let target_ms = ((engine_frame_post + 1) as f64 / FPS) * 1000.0 - elapsed_ms;
          if target_ms <= -2000.0 && !slow_tick_warned {
            eprintln!("[triangle-rs] WARNING: Game tick lagging by more than 2 seconds!");
            slow_tick_warned = true;
          }
          if target_ms <= 0.0 && engine_frame_post % (FPS as i32 / 2) != 0 {
            tokio::task::yield_now().await;
            continue;
          }
          if target_ms > 0.0 {
            tokio::time::sleep(Duration::from_millis(target_ms as u64)).await;
          } else {
            tokio::task::yield_now().await;
          }
        } // end loop
      });
    });
    std::mem::forget(handle);
  }
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn do_flush_iges(
  ige_queue: &Arc<Mutex<Vec<Value>>>,
  incoming: &mut Vec<ReplayFrame>,
  subframe: f64,
  pause: bool,
  force_pause: bool,
  kq_len: usize,
  last_flush: &mut TokioInstant,
) {
  let paused = force_pause || (pause && kq_len > 0);
  if paused && last_flush.elapsed().as_millis() < MAX_IGE_TIMEOUT_MS {
    return;
  }
  let iges: Vec<Value> = ige_queue.lock().await.drain(..).collect();
  for ige in iges {
    if let Some(frame) = parse_ige_frame(&ige, subframe) {
      incoming.push(ReplayFrame::Ige(frame));
    }
  }
  *last_flush = TokioInstant::now();
}

fn flush_frame_queue(
  frame_queue: &mut Vec<Value>,
  engine_frame: i32,
  can_target: bool,
  manual_allowed: bool,
) -> Vec<Value> {
  let mut frames: Vec<Value> = frame_queue
    .drain(..)
    .filter(|f| f["frame"].as_i64().unwrap_or(0) <= engine_frame as i64)
    .collect();
  if !can_target {
    frames.retain(|f| {
      let t = f["type"].as_str().unwrap_or("");
      t != "strategy" && t != "manual_target"
    });
  }
  if !manual_allowed {
    frames.retain(|f| f["type"].as_str().unwrap_or("") != "manual_target");
  }
  if let Some(idx) = frames
    .iter()
    .position(|f| f["type"].as_str() == Some("full"))
  {
    let full = frames.remove(idx);
    frames.insert(0, full);
  }
  if let Some(idx) = frames
    .iter()
    .position(|f| f["type"].as_str() == Some("start"))
  {
    let start = frames.remove(idx);
    frames.insert(0, start);
  }
  frames
}

fn make_full_frame(options: &GameOptions) -> Value {
  let bw = options.boardwidth.unwrap_or(10) as usize;
  let bh = options.boardheight.unwrap_or(20) as usize;
  serde_json::json!({
    "type": "full", "frame": 0,
    "data": { "game": {
      "board": (0..bh + 20).map(|_| (0..bw).map(|_| Value::Null).collect::<Vec<_>>()).collect::<Vec<_>>(),
      "g": options.g.unwrap_or(0.02),
      "playing": true
    }}
  })
}

fn is_valid_key(k: &TickKeypress, engine_frame: i32) -> bool {
  const VALID_KEYS: &[&str] = &[
    "moveLeft",
    "moveRight",
    "hardDrop",
    "hold",
    "softDrop",
    "rotateCW",
    "rotate180",
    "rotateCCW",
    "undo",
    "redo",
    "retry",
  ];
  k.frame >= engine_frame as f64
    && VALID_KEYS.contains(&k.data.key.as_str())
    && k.data.subframe >= 0.0
    && k.data.subframe.is_finite()
    && k.frame.is_finite()
}

fn parse_ige_frame(data: &Value, subframe: f64) -> Option<IgeFrame> {
  let ige_type = data["type"].as_str()?;
  let d = &data["data"];
  let inner = match ige_type {
    "target" => {
      let targets: Vec<i32> = d["targets"]
        .as_array()
        .map(|a| {
          a.iter()
            .filter_map(|v| v.as_i64().map(|n| n as i32))
            .collect()
        })
        .unwrap_or_default();
      IgeData::Target { targets }
    }
    "interaction" | "interaction_confirm" => IgeData::GarbageInteraction {
      gameid: d["gameid"].as_i64().unwrap_or(0) as i32,
      ackiid: d["ackiid"].as_i64().unwrap_or(0) as i32,
      iid: d["iid"].as_i64().unwrap_or(0) as i32,
      amt: d["amt"].as_i64().unwrap_or(0) as i32,
      size: d["size"].as_u64().unwrap_or(0) as usize,
    },
    _ => return None,
  };
  Some(IgeFrame {
    subframe,
    data: inner,
  })
}

// ── Game ───────────────────────────────────────────────────────────────────────

pub struct Game {
  pub players: Vec<Player>,
  pub raw_players: Vec<RawPlayer>,
  pub self_: Option<Self_>,
  pub spectating_strategy: SpectatingStrategy,
  emitter: Arc<EventEmitter>,
  round_start_handlers: RoundStartHandlers,
  _spectate_handle: tokio::task::JoinHandle<()>,
}

impl Game {
  pub fn new(
    emitter: Arc<EventEmitter>,
    raw_players: Vec<RawPlayer>,
    self_userid: &str,
    spectating_strategy: SpectatingStrategy,
    send_fn: Arc<dyn Fn(String, Value) + Send + Sync>,
  ) -> Self {
    let round_start_handlers: RoundStartHandlers = Arc::new(Mutex::new(Vec::new()));

    let self_ = raw_players
      .iter()
      .find(|p| p.userid == self_userid)
      .map(|p| {
        let s = Self_::new(
          p,
          &raw_players,
          send_fn,
          emitter.clone(),
          round_start_handlers.clone(),
        );
        s.init();
        s
      });

    let players: Vec<Player> = raw_players
      .iter()
      .map(|p| {
        Player::new(
          p,
          &raw_players,
          emitter.clone(),
          spectating_strategy.clone(),
        )
      })
      .collect();

    let player_arcs: Vec<_> = players
      .iter()
      .map(|p| (p.inner.clone(), p.engine.clone(), p.strategy.clone()))
      .collect();

    let spectate_handle = {
      let interval = Duration::from_micros((1_000_000.0 / FPS) as u64);
      tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
          ticker.tick().await;
          for (inner, engine, strategy) in &player_arcs {
            let state = { inner.lock().await.state.clone() };
            if state != SpectatingState::Active {
              continue;
            }
            let topped_out = { inner.lock().await.topped_out };
            if topped_out {
              continue;
            }
            let strategy_val = strategy.lock().await.clone();
            let mut lock = inner.lock().await;
            let mut eng = engine.lock().await;

            let tick_once_fn = |queue: &mut Vec<GameReplayFrame>, eng: &mut Engine| -> bool {
              let engine_frame = eng.frame;
              let mut frames: Vec<ReplayFrame> = Vec::new();
              while queue
                .first()
                .map(|f| f.frame <= engine_frame)
                .unwrap_or(false)
              {
                let f = queue.remove(0);
                if let Some(rf) = grf_to_replay_frame(&f) {
                  frames.push(rf);
                }
              }
              let _ = eng.tick(&frames);
              eng.events.iter().any(|e| {
                if let crate::engine::EngineEvent::FallingLock(lr) = e {
                  lr.topout
                } else {
                  false
                }
              })
            };

            match strategy_val {
              SpectatingStrategy::Instant => {
                while lock.queue.iter().any(|f| f.frame > eng.frame) {
                  if tick_once_fn(&mut lock.queue, &mut eng) {
                    lock.topped_out = true;
                    break;
                  }
                }
              }
              SpectatingStrategy::Smooth => {
                if lock.queue.is_empty() {
                  continue;
                }
                let last_frame = lock.queue.last().map(|f| f.frame).unwrap_or(0);
                while lock.queue.iter().any(|f| f.frame > eng.frame) && eng.frame < last_frame - 20
                {
                  if tick_once_fn(&mut lock.queue, &mut eng) {
                    lock.topped_out = true;
                    break;
                  }
                }
                if !lock.topped_out && lock.queue.iter().any(|f| f.frame > eng.frame) {
                  tick_once_fn(&mut lock.queue, &mut eng);
                }
              }
            }
          }
        }
      })
    };

    Game {
      players,
      raw_players,
      self_,
      spectating_strategy,
      emitter,
      round_start_handlers,
      _spectate_handle: spectate_handle,
    }
  }

  pub fn on_round_start<F>(&self, f: F)
  where
    F: Fn(TickSetter, Arc<Mutex<Engine>>) + Send + Sync + 'static,
  {
    self.round_start_handlers.blocking_lock().push(Arc::new(f));
  }

  pub fn opponents<'a>(&'a self, self_userid: &str) -> Vec<&'a Player> {
    self
      .players
      .iter()
      .filter(|p| p.userid != self_userid)
      .collect()
  }

  pub fn spectate(&self, game_ids: &[u32]) {
    let ids: Vec<Value> = game_ids
      .iter()
      .map(|id| Value::Number((*id).into()))
      .collect();
    self
      .emitter
      .emit("game.scope.start", serde_json::json!({ "scopes": ids }));
  }

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

  pub fn unspectate(&self, game_ids: &[u32]) {
    let ids: Vec<Value> = game_ids
      .iter()
      .map(|id| Value::Number((*id).into()))
      .collect();
    self
      .emitter
      .emit("game.scope.end", serde_json::json!({ "scopes": ids }));
  }

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

  pub fn spectate_userids(&self, user_ids: &[String]) {
    let ids: Vec<u32> = self
      .players
      .iter()
      .filter(|p| user_ids.iter().any(|uid| uid == &p.userid))
      .map(|p| p.gameid)
      .collect();
    self.spectate(&ids);
  }

  pub fn unspectate_userids(&self, user_ids: &[String]) {
    let ids: Vec<u32> = self
      .players
      .iter()
      .filter(|p| user_ids.iter().any(|uid| uid == &p.userid))
      .map(|p| p.gameid)
      .collect();
    self.unspectate(&ids);
  }

  pub fn set_spectating_strategy(&mut self, strategy: SpectatingStrategy) {
    self.spectating_strategy = strategy;
  }

  pub fn deliver_replay_frame(&mut self, _gameid: u32, _frame: Value) {}

  pub fn set_player_spectating(&mut self, _gameid: u32, _active: bool) {}
}

// ── snapshotFromState ─────────────────────────────────────────────────────────

pub fn snapshot_from_state(frame: i32, init: &EngineInitParams, state: &Value) -> EngineSnapshot {
  let board_height = init.board.height;
  let full_height = (board_height + 20) as f64;

  // board (server: bottom-to-top; engine: top-to-bottom)
  let board: Vec<Vec<Option<Tile>>> = state["board"]
    .as_array()
    .cloned()
    .unwrap_or_default()
    .into_iter()
    .rev()
    .map(|row| {
      row
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|sq| {
          sq.as_str().and_then(parse_mino_str).map(|mino| Tile {
            mino,
            connections: CONN_ALL,
          })
        })
        .collect()
    })
    .collect();

  let falling = &state["falling"];
  let fl_flags = falling["flags"].as_u64().unwrap_or(0) as u32;
  let fl_x = falling["x"].as_f64().unwrap_or(0.0);
  let fl_y = falling["y"].as_f64().unwrap_or(0.0);
  let fl_hy = falling["hy"].as_f64().unwrap_or(0.0);
  let fl_type = parse_mino_str(falling["type"].as_str().unwrap_or("")).unwrap_or(Mino::I);
  let fl_r = falling["r"].as_i64().unwrap_or(0) as u8;

  use crate::engine::constants::{ROTATION_MINI, ROTATION_SPIN};
  let last_spin = if fl_flags & ROTATION_SPIN != 0 {
    Some(SpinType::Normal)
  } else if !(fl_flags as i32) & (ROTATION_SPIN | ROTATION_MINI) as i32 == 0 {
    Some(SpinType::Mini)
  } else {
    None
  };

  let queue_pieces: Vec<Mino> = state["bag"]
    .as_array()
    .cloned()
    .unwrap_or_default()
    .iter()
    .filter_map(|m| parse_mino_str(m.as_str().unwrap_or("")))
    .collect();
  let bag_extra: Vec<Mino> = state["bagex"]
    .as_array()
    .cloned()
    .unwrap_or_default()
    .iter()
    .filter_map(|m| parse_mino_str(m.as_str().unwrap_or("")))
    .collect();
  let bag_snap = BagSnapshot {
    rng: state["rng"].as_i64().unwrap_or(0),
    id: state["bagid"].as_u64().unwrap_or(0),
    extra: bag_extra,
    last_generated: state["lastGenerated"].as_str().and_then(parse_mino_str),
  };
  let queue_snap = QueueSnapshot {
    value: queue_pieces,
    bag: bag_snap,
  };

  let garbage_speed = init.garbage.garbage.speed;
  let impending = state["impendingdamage"]
    .as_array()
    .cloned()
    .unwrap_or_default();
  let waiting_frames = state["waitingframes"]
    .as_array()
    .cloned()
    .unwrap_or_default();
  let garbage_queue: Vec<IncomingGarbage> = impending
    .iter()
    .map(|g| {
      let g_id = g["id"].as_i64().unwrap_or(0);
      let confirmed_frame = waiting_frames.iter().find_map(|wf| {
        if wf["type"].as_str() == Some("incoming-attack-hit") && wf["data"].as_i64() == Some(g_id) {
          wf["target"].as_i64().map(|t| t as i32)
        } else {
          None
        }
      });
      IncomingGarbage {
        frame: confirmed_frame.unwrap_or(i32::MAX - garbage_speed),
        amount: g["amt"].as_i64().unwrap_or(0) as i32,
        size: g["size"].as_u64().unwrap_or(0) as usize,
        cid: g["cid"].as_i64().unwrap_or(0) as i32,
        gameid: g["gameid"].as_i64().unwrap_or(0) as i32,
        confirmed: confirmed_frame.is_some(),
      }
    })
    .collect();

  let garbage_snap = GarbageQueueSnapshot {
    seed: state["rngex"].as_i64().unwrap_or(0),
    last_tank_time: state["lasttanktime"].as_i64().unwrap_or(0) as i32,
    last_column: state["lastcolumn"].as_i64().map(|v| v as i32),
    sent: state["stats"]["garbage"]["sent"].as_i64().unwrap_or(0) as i32,
    has_changed_column: state["haschangedcolumn"].as_bool().unwrap_or(false),
    last_received_count: state["lastreceivedcount"].as_i64().unwrap_or(0) as i32,
    queue: garbage_queue,
  };

  let ack = &state["garbageacknowledgements"];
  let inc_ack = ack["incoming"].as_object().cloned().unwrap_or_default();
  let out_ack = ack["outgoing"].as_object().cloned().unwrap_or_default();
  let all_pids: std::collections::HashSet<String> =
    inc_ack.keys().chain(out_ack.keys()).cloned().collect();
  let mut ige_players: HashMap<i32, PlayerData> = HashMap::new();
  for pid_str in &all_pids {
    let pid: i32 = pid_str.parse().unwrap_or(0);
    let inc = inc_ack.get(pid_str).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let outgoing: Vec<GarbageRecord> = out_ack
      .get(pid_str)
      .and_then(|v| v.as_array())
      .cloned()
      .unwrap_or_default()
      .iter()
      .map(|o| GarbageRecord {
        iid: o["iid"].as_i64().unwrap_or(0) as i32,
        amount: o["amt"].as_i64().unwrap_or(0) as i32,
      })
      .collect();
    ige_players.insert(
      pid,
      PlayerData {
        incoming: inc,
        outgoing,
      },
    );
  }

  let ls = &state["lShift"];
  let rs = &state["rShift"];
  let time_obj = &state["time"];
  let input = InputState {
    l_shift: ShiftState {
      held: ls["held"].as_bool().unwrap_or(false),
      arr: ls["arr"].as_f64().unwrap_or(0.0),
      das: ls["das"].as_f64().unwrap_or(0.0),
      dir: ls["dir"].as_i64().unwrap_or(-1) as i32,
    },
    r_shift: ShiftState {
      held: rs["held"].as_bool().unwrap_or(false),
      arr: rs["arr"].as_f64().unwrap_or(0.0),
      das: rs["das"].as_f64().unwrap_or(0.0),
      dir: rs["dir"].as_i64().unwrap_or(1) as i32,
    },
    last_shift: state["lastshift"].as_i64().unwrap_or(-1) as i32,
    keys: InputKeys {
      soft_drop: state["inputSoftdrop"].as_bool().unwrap_or(false),
      rotate_ccw: state["inputRotateCCW"].as_bool().unwrap_or(false),
      rotate_cw: state["inputRotateCW"].as_bool().unwrap_or(false),
      rotate_180: state["inputRotate180"].as_bool().unwrap_or(false),
      hold: state["inputHold"].as_bool().unwrap_or(false),
    },
    first_input_time: state["firstInputTime"].as_f64().unwrap_or(0.0),
    time: crate::engine::InputTime {
      start: time_obj["start"].as_f64().unwrap_or(0.0),
      zero: time_obj["zero"].as_bool().unwrap_or(false),
      locked: time_obj["locked"].as_bool().unwrap_or(false),
      prev: time_obj["prev"].as_f64().unwrap_or(0.0),
    },
    last_piece_time: state["lastpiecetime"].as_f64().unwrap_or(0.0),
  };

  let stats_v = &state["stats"];
  let spike_v = &state["spike"];
  let targets = state["targets"].as_array().map(|a| {
    a.iter()
      .filter_map(|v| v.as_i64().map(|n| n as i32))
      .collect()
  });
  let hold = state["hold"].as_str().and_then(parse_mino_str);

  EngineSnapshot {
    is_undo_redo: false,
    board,
    falling: TetrominoSnapshot {
      symbol: fl_type,
      location: [fl_x, full_height - fl_y],
      locking: falling["locking"].as_f64().unwrap_or(0.0),
      lock_resets: falling["lockresets"].as_i64().unwrap_or(0) as i32,
      rot_resets: falling["rotresets"].as_i64().unwrap_or(0) as i32,
      safe_lock: falling["safelock"].as_i64().unwrap_or(0) as i32,
      highest_y: full_height - fl_hy,
      rotation: fl_r,
      falling_rotations: 0,
      total_rotations: state["totalRotations"].as_i64().unwrap_or(0) as i32,
      irs: falling["irs"].as_i64().unwrap_or(0) as i32,
      ihs: false,
      aox: 0,
      aoy: 0,
      keys: falling["keys"].as_i64().unwrap_or(0) as i32,
    },
    frame,
    garbage: garbage_snap,
    hold,
    hold_locked: state["holdlocked"].as_bool().unwrap_or(false),
    last_spin,
    last_was_clear: state["lastwasclear"].as_bool().unwrap_or(false),
    queue: queue_snap.clone(),
    inner_queue: queue_snap,
    input,
    subframe: state["subframe"].as_f64().unwrap_or(0.0),
    targets,
    stats: EngineStats {
      garbage_sent: stats_v["garbage"]["sent"].as_i64().unwrap_or(0) as i32,
      garbage_attack: stats_v["garbage"]["attack"].as_i64().unwrap_or(0) as i32,
      garbage_receive: stats_v["garbage"]["received"].as_i64().unwrap_or(0) as i32,
      garbage_cleared: stats_v["garbage"]["cleared"].as_i64().unwrap_or(0) as i32,
      combo: stats_v["combo"].as_i64().unwrap_or(0) as i32,
      b2b: stats_v["btb"].as_i64().unwrap_or(0) as i32,
      pieces: stats_v["piecesplaced"].as_i64().unwrap_or(0) as i32,
      lines: stats_v["lines"].as_i64().unwrap_or(0) as i32,
    },
    glock: state["glock"].as_f64().unwrap_or(0.0),
    stock: state["stock"].as_i64().unwrap_or(0) as i32,
    state: fl_flags,
    spike: SpikeState {
      count: spike_v["count"].as_i64().unwrap_or(0) as i32,
      timer: spike_v["timer"].as_i64().unwrap_or(0) as i32,
    },
    time_frame_offset: state["time"]["frameoffset"].as_i64().unwrap_or(0) as i32,
    res_cache: ResCache {
      pieces: 0,
      garbage_sent: Vec::new(),
      garbage_received: Vec::new(),
      keys: Vec::new(),
      last_lock: 0.0,
    },
    practice: PracticeState {
      undo: Vec::new(),
      redo: Vec::new(),
      retry: false,
      retry_iter: 0,
      last_piece: None,
    },
    ige: IgeHandlerSnapshot {
      iid: state["interactionid"].as_i64().unwrap_or(0) as i32,
      players: ige_players,
    },
  }
}

fn parse_mino_str(s: &str) -> Option<Mino> {
  match s.to_uppercase().as_str() {
    "I" => Some(Mino::I),
    "J" => Some(Mino::J),
    "L" => Some(Mino::L),
    "O" => Some(Mino::O),
    "S" => Some(Mino::S),
    "T" => Some(Mino::T),
    "Z" => Some(Mino::Z),
    _ => None,
  }
}

// ── Engine builder ─────────────────────────────────────────────────────────────

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
