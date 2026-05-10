use serde_json::{Value, json};
/// originally `Self` in Triangle.js. changed to `Me` to avoid confusion with `Self` in Rust.
use std::{
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::sync::{Mutex, oneshot};

use crate::{
  Engine,
  classes::{ClientUser, Ribbon, game::FRAMES_PER_SECOND, ribbon::Hook},
  types::{
    events::{recv, send},
    game::{
      ReadyPlayer, TargetingStrategy, ige,
      replay::{self, Frame, FrameData, Keypress},
      tick::{self, Out},
    },
  },
  utils::Logger,
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
  players: Vec<ReadyPlayer>,
  is_practice: bool,
  over: bool,

  pub engine: Engine,
  pub gameid: u64,
  pub options: Value, // TODO: swap out
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
  logger: Logger,

  me: ClientUser,

  start_hook: Arc<Mutex<Option<oneshot::Sender<()>>>>,

  handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
  pub state: Arc<Mutex<MeState>>,
  pub tick: tick::Ticker,
}


impl Me {
  pub fn new(ribbon: Ribbon, me: ClientUser, players: Vec<ReadyPlayer>) -> Self {
    let self_player = players
      .iter()
      .find(|p| p.userid == me.id)
      .expect("Me not found in players")
      .clone();

    let (start_hook_tx, start_hook_rx) = oneshot::channel();

    let s = Self {
      ribbon: ribbon.clone(),
      hook: ribbon.hook(),
      logger: Logger::new("triangle-rs"),
      me,
      handles: Arc::new(Mutex::new(Vec::new())),
      start_hook: Arc::new(Mutex::new(Some(start_hook_tx))),
      tick: tick::Ticker(Arc::new(Mutex::new(Box::new(|_| {
        Box::pin(async {
          Out {
            keys: vec![],
            run_after: vec![],
          }
        })
      })))),

      state: Arc::new(Mutex::new(MeState {
        frame_queue: Vec::new(),
        incoming_garbage: Vec::new(),
        target: TargetingStrategy::Even,
        pause_iges: false,
        force_pause_iges: false,
        ige_queue: Vec::new(),
        slow_tick_warning: false,
        players,
        is_practice: false,
        over: false,

        // engine: Game::create_engine(self_player.options, self_player.gameid, players.clone()),
        engine: unimplemented!(), // TODO: implement
        gameid: self_player.gameid,
        options: self_player.options,
        server_targets: Vec::new(),
        enemies: Vec::new(),
        key_queue: Vec::new(),
        can_target: false,
        start_time: None,
        last_ige_flush: Instant::now(),
      })),
    };

    s.start(start_hook_rx);

    s
  }

  pub async fn destroy(&mut self) {
    self.hook.destroy();

    let mut state = self.state.lock().await;

    state.over = true;
    // state.engine.events.clear();
    // TODO: clear engine events

    let mut handles = self.handles.lock().await;
    for handle in handles.drain(..) {
      handle.abort();
    }

    // TODO: clear self from game
  }

  pub async fn init(&mut self) {
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
        me.handles.lock().await.push(tokio::spawn(async move {
          m.start_hook.lock().await.take().map(|tx| tx.send(()).ok());
        }));
      })
      .await;

    let me = self.clone();

    self
      .hook
      .on::<recv::game::Abort>(async move |_| {
        me.handles.lock().await.drain(..).for_each(|h| h.abort());
      })
      .await;
  }

  fn start(&self, receiver: oneshot::Receiver<()>) {
    let me = self.clone();

    tokio::spawn(async move {
      if receiver.await.is_err() {
        return;
      }

      let target = {
        let mut state = me.state.lock().await;
        state.frame_queue.push(replay::Frame {
          frame: 0,
          data: FrameData::Start(replay::Start(json!({}))),
        });
        state.start_time = Some(Instant::now());

        state.target.clone()
      };

      me.set_target(target).await;

      me.ribbon.emit(recv::client::game::round::Start {
        ticker: me.tick.clone(),
        engine: me.state.lock().await.engine.clone(),
      });
    });
  }

  async fn flush_frames(&mut self) -> Vec<replay::Frame> {
    let state = self.state.lock().await;

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

    if !state.options["manual_allowed"].as_bool().unwrap_or(false) {
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
  async fn tick_game(&mut self) -> (bool, Duration) {
    let (snapshot, engine, gameid) = {
      let state = self.state.lock().await;

      if state.over {
        return (false, Duration::from_secs(0));
      }

      let snapshot = state.engine.snapshot();

      (snapshot, state.engine.clone(), state.gameid)
    };

    let res = (self.tick.0.lock().await)(tick::In { engine, gameid }).await;

    {
      let mut state = self.state.lock().await;
      state.engine.from_snapshot(&snapshot);

      state.key_queue.extend(res.keys);

      // TODO: verify keys

      if state.over {
        return (false, Duration::from_secs(0));
      }
    }

    self.flush_iges().await;

    let (gameid, frame, start_time) = {
      let mut state = self.state.lock().await;

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

      (state.gameid, state.engine.frame, state.start_time)
    };

    if frame != 0 && frame % FRAMES_PER_MESSAGE == 0 {
      let frames = self.flush_frames().await;
      self
        .ribbon
        .emit(send::game::Replay {
          gameid,
          provisioned: frame,
          frames,
        })
        .await;
    }

    for f in res.run_after {
      f.call().await;
    }

    let target = Duration::from_secs_f64((frame + 1) as f64 / FRAMES_PER_SECOND as f64)
      - start_time.unwrap().elapsed();

    let mut state = self.state.lock().await;

    if target.as_secs_f64() <= 2.0 && !state.slow_tick_warning {
      self.logger.warn("triangle-rs is lagging behind by more than 2 seconds! Your ticker function is likely taking too long to execute.");
      state.slow_tick_warning = true;
    }

    if target.as_secs_f64() <= 0.0 && frame.is_multiple_of(FRAMES_PER_SECOND / 2) {
      return (true, Duration::from_secs(0));
    }

    (true, target.max(Duration::from_micros(50))) // minimum delay of 50µs to prevent runaway loop in case of severe lag
  }

  async fn flush_iges(&mut self) {
    let iges = {
      let mut state = self.state.lock().await;
      if state.force_pause_iges || (state.pause_iges && !state.key_queue.is_empty()) {
        if state.last_ige_flush.elapsed() >= MAX_IGE_TIMEOUT {
          self.logger.warn("Force flushing IGE queue to prevent protocol violation + disconnect/ban. You either left force pause iges on for too long or you have pause iges on and continuously keep inputs queued.");
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

  async fn __internal_handle_ige(&mut self, ige: ige::IGE) {
    let mut state = self.state.lock().await;
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
    let mut state = self.state.lock().await;

    if !state.can_target {
      return Err("Targeting is currently disabled by the server".to_string());
    }
    if !state.options["manual_allowed"].as_bool().unwrap_or(false)
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
      let mut state = self.state.lock().await;
      state.pause_iges = pause;
    };

    self.flush_iges().await;
  }

  pub async fn set_force_pause_iges(&mut self, force_pause: bool) {
    {
      let mut state = self.state.lock().await;
      state.force_pause_iges = force_pause;
    };

    self.flush_iges().await;
  }
}
