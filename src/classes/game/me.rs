use serde_json::{Value, json};
/// originally `Self` in Triangle.js. changed to `Me` to avoid confusion with `Self` in Rust.
use std::{
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::sync::{Mutex, oneshot};
use parking_lot::Mutex as PMutex;

use crate::engine::queue::{Queue, QueueInitParams, bag::BagType};
use crate::{
  Engine,
  classes::{
    ClientUser, Ribbon,
    game::{FRAMES_PER_SECOND, Game},
    ribbon::Hook,
  },
  types::{
    events::{recv, send},
    game::{
      ReadyPlayer, TargetingStrategy, ige,
      replay::{self, Frame, FrameData, Keypress},
      tick::{self, Out},
    },
  },
};

pub const MAX_IGE_TIMEOUT: Duration = Duration::from_secs(30);
pub const FRAMES_PER_MESSAGE: u64 = 12;

#[derive(Clone, Debug)]
pub struct MeState {
  frame_queue: Vec<replay::Frame>,
  /// (frame, ige)
  incoming_garbage: Vec<(u64, ige::IGE)>,
  target: TargetingStrategy,
  pause_iges: bool,
  force_pause_iges: bool,
  ige_queue: Vec<ige::IGE>,
  slow_tick_warning: bool,
  #[allow(dead_code)]
  is_practice: bool,
  over: bool,
  pub engine: Engine,
  pub server_targets: Vec<u64>,
  pub enemies: Vec<u64>,
  pub key_queue: Vec<tick::Keypress>,
  pub can_target: bool,
  pub start_time: Option<Instant>,
  pub last_ige_flush: Instant,
}

#[derive(Clone, Debug)]
pub struct Me {
  ribbon: Ribbon,
  hook: Hook,

  start_hook: Arc<PMutex<Option<oneshot::Sender<()>>>>,

  handles: Arc<PMutex<Vec<tokio::task::JoinHandle<()>>>>,
  pub state: Arc<PMutex<MeState>>,
  pub gameid: u64,
  pub tick: tick::Ticker,
  pub options: Arc<Value>, // TODO: swap out
}

impl Me {
  pub async fn new(ribbon: Ribbon, me: ClientUser, players: Vec<ReadyPlayer>) -> Self {
    let self_player = players
      .iter()
      .find(|p| p.userid == me.id)
      .expect("Me not found in players")
      .clone();

    let (start_hook_tx, start_hook_rx) = oneshot::channel();

    let s = Self {
      ribbon: ribbon.clone(),
      hook: ribbon.hook(),
      handles: Arc::new(PMutex::new(Vec::new())),
      start_hook: Arc::new(PMutex::new(Some(start_hook_tx))),
      tick: tick::Ticker(Arc::new(Mutex::new(Box::new(|_| {
        Box::pin(async {
          Out {
            keys: vec![],
            run_after: vec![],
          }
        })
      })))),

      state: Arc::new(PMutex::new(MeState {
        frame_queue: Vec::new(),
        incoming_garbage: Vec::new(),
        target: TargetingStrategy::Even,
        pause_iges: false,
        force_pause_iges: false,
        ige_queue: Vec::new(),
        slow_tick_warning: false,
        is_practice: false,
        over: false,

        engine: Game::create_engine(
          &self_player.options,
          self_player.gameid,
          players.clone().as_slice(),
        ),
        server_targets: Vec::new(),
        enemies: Vec::new(),
        key_queue: Vec::new(),
        can_target: false,
        start_time: None,
        last_ige_flush: Instant::now(),
      })),

      options: Arc::new(self_player.options),

      gameid: self_player.gameid,
    };

    s.start(start_hook_rx).await;

    s
  }

  pub async fn destroy(&mut self) {
    self.hook.destroy().await;

    let mut state = self.state.lock();

    state.over = true;
    state.engine.events.destroy();

    let mut handles = self.handles.lock();
    for handle in handles.drain(..) {
      handle.abort();
    }
  }

  pub async fn init(&self) {
    self
      .hook
      .on::<recv::game::Match>(async |_| {
        // maybe do something here idk
      })
      .await;

    let me: Me = self.clone();

    self
      .hook
      .on::<recv::game::Start>(async move |_| {
        let m = me.clone();
        tokio::time::sleep(Duration::from_millis(
          me.options["countdown_count"].as_u64().unwrap_or(0)
            * me.options["countdown_interval"].as_u64().unwrap_or(0)
            + me.options["precountdown"].as_u64().unwrap_or(0)
            + me.options["prestart"].as_u64().unwrap_or(0),
        ))
        .await;
        me.handles.lock().push(tokio::spawn(async move {
          m.start_hook.lock().take().map(|tx| tx.send(()).ok());
        }));
      })
      .await;

    let me = self.clone();

    self
      .hook
      .on::<recv::game::Abort>(async move |_| {
        me.handles.lock().drain(..).for_each(|h| h.abort());
      })
      .await;

    let me = self.clone();

    self
      .hook
      .on::<recv::game::replay::IGE>(async move |ige| {
        me.state
          .lock()
          .await
          .ige_queue
          .extend(ige.iges.iter().cloned());
        me.flush_iges().await;
      })
      .await;
  }

  async fn start(&self, receiver: oneshot::Receiver<()>) {
    let me = self.clone();
    let handles = me.handles.clone();

    handles.lock().push(tokio::spawn(async move {
      if receiver.await.is_err() {
        return;
      }

      let target = {
        let mut state = me.state.lock();
        state.frame_queue.push(replay::Frame {
          frame: 0,
          data: FrameData::Start(replay::Start(json!({}))),
        });
        state.frame_queue.push(me.get_full_frame());
        state.start_time = Some(Instant::now());

        state.target.clone()
      };

      me.set_target(target).await.ok();

      me.ribbon.emit(recv::client::game::round::Start {}).await;

      loop {
        let (continue_game, delay) = me.tick_game().await;
        if !continue_game {
          break;
        }
        if delay.as_secs_f64() > 0.0 {
          tokio::time::sleep(delay).await;
        }
      }
    }));
  }

  async fn flush_frames(&self) -> Vec<replay::Frame> {
    let state = self.state.lock();

    let mut return_frames: Vec<replay::Frame> = state
      .frame_queue
      .iter()
      .filter(|f| f.frame <= state.engine.frame)
      .cloned()
      .collect();

    if !state.can_target {
      return_frames.retain(|f| match &f.data {
        FrameData::Strategy(_) | FrameData::ManualTarget(_) => false,
        _ => true,
      });
    }

    if !self.options["manual_allowed"].as_bool().unwrap_or(false) {
      return_frames.retain(|f| match &f.data {
        FrameData::ManualTarget(_) => false,
        _ => true,
      });
    }

    // move the full frame to the front as a precaution
    if let Some(full_frame_index) = return_frames
      .iter()
      .position(|f| matches!(f.data, FrameData::Full(_)))
    {
      // return_frames.swap(0, full_frame_index);
      let frame = return_frames.remove(full_frame_index);
      return_frames.insert(0, frame);
    }

    // move start frame to front (start -> full at the end)
    if let Some(start_frame_index) = return_frames
      .iter()
      .position(|f| matches!(f.data, FrameData::Start(_)))
    {
      let frame = return_frames.remove(start_frame_index);
      return_frames.insert(0, frame);
    }

    return_frames.clone()
  }

  /// Returns (continue, delay until next tick. 0 = run instantly)
  async fn tick_game(&self) -> (bool, Duration) {
    let (snapshot, engine) = {
      let state = self.state.lock();

      if state.over {
        return (false, Duration::from_secs(0));
      }

      let snapshot = state.engine.snapshot();

      (snapshot, state.engine.clone())
    };

    let res = (self.tick.0.lock().await)(tick::In {
      engine,
      gameid: self.gameid,
    })
    .await;

    {
      let mut state = self.state.lock();
      state.engine.from_snapshot(&snapshot);

      state.key_queue.extend(res.keys);

      // TODO: verify keys

      if state.over {
        return (false, Duration::from_secs(0));
      }
    }

    self.flush_iges().await;

    let (frame, start_time) = {
      let mut state = self.state.lock();

      let mut keys = Vec::new();

      let frame = state.engine.frame;

      state.key_queue.retain(|key| {
        if key.frame == frame {
          keys.push(key.clone());
          return false;
        }
        true
      });

      let key_frames = keys
        .iter()
        .map(|key| {
          let k = Keypress {
            key: key.data.key,
            subframe: key.data.subframe,
            hoisted: key.data.hoisted,
          };
          Frame {
            frame: key.frame,
            data: match key.r#type {
              tick::KeypressType::Keydown => FrameData::KeyDown(k),
              tick::KeypressType::Keyup => FrameData::KeyUp(k),
            },
          }
        })
        .collect::<Vec<_>>();

      let mut all_frames: Vec<Frame> = state
        .incoming_garbage
        .drain(..)
        .map(|(frame, ige)| Frame {
          frame,
          data: FrameData::IGE(ige),
        })
        .collect();
      all_frames.extend(key_frames.iter().cloned());

      state.engine.tick(&all_frames);

      state.frame_queue.extend(key_frames);

      (state.engine.frame, state.start_time)
    };

    if frame != 0 && frame % FRAMES_PER_MESSAGE == 0 {
      let frames = self.flush_frames().await;
      self
        .ribbon
        .emit(send::game::Replay {
          gameid: self.gameid,
          provisioned: frame,
          frames,
        })
        .await;
    }

    for f in res.run_after {
      f.call().await;
    }

    let target = Duration::from_secs_f64((frame + 1) as f64 / FRAMES_PER_SECOND as f64)
      .saturating_sub(start_time.unwrap().elapsed());
    let lag_back = start_time
      .unwrap()
      .elapsed()
      .saturating_sub(Duration::from_secs_f64(
        (frame + 1) as f64 / FRAMES_PER_SECOND as f64,
      ));

    let mut state = self.state.lock();

    if lag_back.as_secs_f64() >= 2.0 && !state.slow_tick_warning {
      tracing::warn!(
        "triangle-rs is lagging behind by more than 2 seconds! Your ticker function is likely taking too long to execute."
      );
      state.slow_tick_warning = true;
    }

    if target.as_secs_f64() <= 0.0 && frame.is_multiple_of(FRAMES_PER_SECOND / 2) {
      return (true, Duration::from_secs(0));
    }

    (true, target.max(Duration::from_micros(50))) // minimum delay of 50µs to prevent runaway loop in case of severe lag
  }

  async fn flush_iges(&self) {
    let iges = {
      let mut state = self.state.lock();
      if state.force_pause_iges || (state.pause_iges && !state.key_queue.is_empty()) {
        if state.last_ige_flush.elapsed() >= MAX_IGE_TIMEOUT {
          tracing::warn!(
            "Force flushing IGE queue to prevent protocol violation + disconnect/ban. You either left force pause iges on for too long or you have pause iges on and continuously keep inputs queued."
          );
        } else {
          return;
        }
      }

      state.last_ige_flush = Instant::now();
      state.ige_queue.drain(..).collect::<Vec<_>>()
    };

    for ige in iges {
      self.__internal_handle_ige(ige).await;
    }
  }

  async fn __internal_handle_ige(&self, ige: ige::IGE) {
    let mut state = self.state.lock();
    let frame = Frame {
      frame: state.engine.frame,
      data: FrameData::IGE(ige.clone()),
    };

    state.frame_queue.push(frame.clone());
    state.incoming_garbage.push((frame.frame, ige.clone()));

    match ige.data {
      ige::IGEData::InteractionConfirm(data) => match data {
        ige::interaction::InteractionData::Targeted(_data) => {
          // TODO: implement
        }
        _ => {}
      },
      ige::IGEData::Target(data) => {
        state.server_targets = data.targets;
      }
      ige::IGEData::AllowTargeting(data) => {
        state.can_target = data.value;
      }
      _ => {}
    }
  }

  pub async fn set_target(&self, target: TargetingStrategy) -> Result<(), String> {
    let mut state = self.state.lock();

    if !state.can_target {
      return Err("Targeting is currently disabled by the server".to_string());
    }
    if !self.options["manual_allowed"].as_bool().unwrap_or(false)
      && matches!(target, TargetingStrategy::Manual(_))
    {
      return Err("Manual targeting is not allowed in this game".to_string());
    }

    let frame = state.engine.frame;

    state.frame_queue.push(match target.clone() {
      TargetingStrategy::Manual(target) => Frame {
        frame: frame,
        data: FrameData::ManualTarget(target),
      },
      _ => Frame {
        frame: frame,
        data: FrameData::Strategy(target),
      },
    });

    Ok(())
  }

  pub async fn set_pause_iges(&mut self, pause: bool) {
    {
      let mut state = self.state.lock();
      state.pause_iges = pause;
    };

    self.flush_iges().await;
  }

  pub async fn set_force_pause_iges(&mut self, force_pause: bool) {
    {
      let mut state = self.state.lock();
      state.force_pause_iges = force_pause;
    };

    self.flush_iges().await;
  }

  pub fn get_full_frame(&self) -> replay::Frame {
    let options = &*self.options;

    let boardheight = options
      .get("boardheight")
      .and_then(Value::as_u64)
      .unwrap_or(20) as usize;
    let boardwidth = options
      .get("boardwidth")
      .and_then(Value::as_u64)
      .unwrap_or(10) as usize;
    let seed = options.get("seed").and_then(Value::as_i64).unwrap_or(0);
    let bagtype_str = options
      .get("bagtype")
      .and_then(Value::as_str)
      .unwrap_or("7-bag");

    let bag_kind = match bagtype_str {
      "7-bag" | "bag7" => BagType::Bag7,
      "14-bag" | "bag14" => BagType::Bag14,
      "classic" => BagType::Classic,
      "pairs" => BagType::Pairs,
      "total mayhem" => BagType::TotalMayhem,
      "7+1" => BagType::Bag7Plus1,
      "7+2" => BagType::Bag7Plus2,
      "7+X" | "7+x" => BagType::Bag7PlusX,
      _ => BagType::Bag7,
    };

    let queue = Queue::new(QueueInitParams {
      seed,
      kind: bag_kind,
      min_length: 7,
    });

    let bag = queue.as_slice();

    let board: Vec<Value> = (0..(boardheight + 20))
      .map(|_| {
        let row: Vec<Value> = (0..boardwidth).map(|_| Value::Null).collect();
        Value::Array(row)
      })
      .collect();

    let handling = options.get("handling").cloned().unwrap_or(json!({}));
    let g = options.get("g").and_then(Value::as_f64).unwrap_or(0.02);

    replay::Frame {
      frame: 0,
      data: FrameData::Full(replay::Full {
        game: json!({
          "board": board,
          "bag": bag,
          "hold": {
            "piece": null,
            "locked": false
          },
          "g": g,
          "controlling": {
            "lShift": {
              "held": false,
              "arr": 0,
              "das": 0,
              "dir": -1
            },
            "rShift": {
              "held": false,
              "arr": 0,
              "das": 0,
              "dir": 1
            },
            "lastshift": -1,
            "inputSoftdrop": false
          },
          "falling": {
            "type": "i",
            "x": 0,
            "y": 0,
            "r": 0,
            "hy": 0,
            "irs": 0,
            "kick": 0,
            "keys": 0,
            "flags": 0,
            "safelock": 0,
            "locking": 0,
            "lockresets": 0,
            "rotresets": 0,
            "skip": []
          },
          "handling": handling,
          "playing": true
        }),
        stats: json!({
          "lines": 0,
          "level_lines": 0,
          "level_lines_needed": 1,
          "inputs": 0,
          "holds": 0,
          "score": 0,
          "zenlevel": 1,
          "zenprogress": 0,
          "level": 1,
          "combo": 0,
          "topcombo": 0,
          "combopower": 0,
          "btb": 0,
          "topbtb": 0,
          "btbpower": 0,
          "tspins": 0,
          "piecesplaced": 0,
          "clears": {
            "singles": 0,
            "doubles": 0,
            "triples": 0,
            "quads": 0,
            "pentas": 0,
            "realtspins": 0,
            "minitspins": 0,
            "minitspinsingles": 0,
            "tspinsingles": 0,
            "minitspindoubles": 0,
            "tspindoubles": 0,
            "minitspintriples": 0,
            "tspintriples": 0,
            "minitspinquads": 0,
            "tspinquads": 0,
            "tspinpentas": 0,
            "allclear": 0
          },
          "garbage": {
            "sent": 0,
            "sent_nomult": 0,
            "maxspike": 0,
            "maxspike_nomult": 0,
            "received": 0,
            "attack": 0,
            "cleared": 0
          },
          "kills": 0,
          "finesse": {
            "combo": 0,
            "faults": 0,
            "perfectpieces": 0
          },
          "zenith": {
            "altitude": 0,
            "rank": 1,
            "peakrank": 1,
            "avgrankpts": 0,
            "floor": 0,
            "targetingfactor": 3,
            "targetinggrace": 0,
            "totalbonus": 0,
            "revives": 0,
            "revivesTotal": 0,
            "revivesMaxOfBoth": 0,
            "speedrun": false,
            "speedrun_seen": false,
            "splits": [0, 0, 0, 0, 0, 0, 0, 0, 0]
          }
        }),
        diyusi: 0,
      }),
    }
  }
}
