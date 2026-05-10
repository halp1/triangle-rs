use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard, oneshot};

use crate::{
  Engine,
  classes::{Ribbon, game::Game, ribbon::Hook},
  types::{
    events::{recv, send},
    game::{ReadyPlayer, ReplayState, SpectatingStrategy, replay::Frame},
  }
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectatingState {
  Inactive,
  Waiting,
  Active,
}

#[derive(Debug)]
pub struct PlayerState {
  pub engine: Engine,
  pub state: SpectatingState,
  pub queue: Vec<Frame>,
  pub strategy: SpectatingStrategy,

  resolvers: Vec<oneshot::Sender<Result<(), String>>>,
}

#[derive(Debug, Clone)]
pub struct Player {
  ribbon: Ribbon,
  hook: Hook,

  pub name: String,
  pub gameid: u64,
  pub userid: String,

  pub state: Arc<Mutex<PlayerState>>,
}

impl Player {
  pub async fn new(
    ribbon: Ribbon,
    strategy: SpectatingStrategy,
    me: ReadyPlayer,
    players: Vec<ReadyPlayer>,
  ) -> Self {
    let hook = ribbon.hook();

    let state = Arc::new(Mutex::new(PlayerState {
      engine: Game::create_engine(&me.options, me.gameid, players.as_slice()),
      state: SpectatingState::Inactive,
      queue: Vec::new(),
      strategy,
      resolvers: Vec::new(),
    }));

    let player = Self {
      ribbon,
      hook,
      name: me.options["username"]
        .as_str()
        .unwrap_or_default()
        .to_string(),
      gameid: me.gameid,
      userid: me.userid,
      state,
    };

    let gameid = me.gameid;

    let state = player.state.clone();

    player
      .hook
      .on::<recv::game::replay::State>(async move |event| {
        if event.gameid != me.gameid {
          return;
        }

        let mut state = state.lock().await;

        let initializer = state.engine.initializer.clone();

        state.resolvers.drain(..).for_each(|resolver| {
          resolver.send(Ok(())).ok();
        });
        state.state = SpectatingState::Active;

        match event.data {
          ReplayState::Early => {
            // TODO: what to do here?
          }
          ReplayState::Wait => {
            // TODO: what to do here?
          }
          ReplayState::State(data) => {
            state.engine.from_snapshot(&Game::snapshot_from_state(
              data.frame,
              &initializer,
              &data.game,
              false,
            ));
          }
        }
      })
      .await;

    let state = player.state.clone();

    player
      .hook
      .on::<recv::game::Replay>(async move |event| {
        let mut state = state.lock().await;
        if event.gameid != gameid
          || state.state != SpectatingState::Active
          || state.engine.topped_out()
        {
          return;
        }

        state.queue.append(&mut event.frames.clone());
      })
      .await;

    player
  }

  pub async fn spectate(&self) -> Result<(), String> {
    {
      let mut state = self.state.lock().await;

      if state.state == SpectatingState::Active {
        return Ok(());
      }

      if state.state == SpectatingState::Inactive {
        state.state = SpectatingState::Waiting;
        drop(state);
        self
          .ribbon
          .emit(send::game::scope::Start(self.gameid))
          .await;
      }
    }

    let rx = {
      let (tx, rx) = oneshot::channel();
      self.state.lock().await.resolvers.push(tx);
      rx
    };

    match rx.await {
      Ok(r) => r,
      Err(_) => Err("Failed to receive spectate confirmation".to_string()),
    }
  }

  pub async fn unspectate(&self) {
    {
      let mut state = self.state.lock().await;
      if state.state == SpectatingState::Inactive {
        return;
      }
      state.state = SpectatingState::Inactive;
      state.queue.clear();
    }
    self.ribbon.emit(send::game::scope::End(self.gameid)).await;
  }

  pub async fn destroy(&mut self) {
    {
      let mut state = self.state.lock().await;

      state.resolvers.drain(..).for_each(|resolver| {
        resolver
          .send(Err("Game ended before spectating could begin".to_string()))
          .ok();
      });

      if state.state != SpectatingState::Inactive {
        drop(state);
        self.unspectate().await;
      }
    }

    self.hook.destroy().await;
  }

  fn tick_once(&self, state: &mut MutexGuard<'_, PlayerState>) {
    let mut frames = Vec::new();

    while state.queue.len() > 0 && state.queue[0].frame <= state.engine.frame {
      frames.push(state.queue.remove(0));
    }

    state.engine.tick(&frames);
  }

  pub async fn _tick(&self) {
    let mut state = self.state.lock().await;
    if state.state != SpectatingState::Active {
      return;
    }

    match state.strategy {
      SpectatingStrategy::Instant => {
        while state
          .queue
          .iter()
          .any(|frame| frame.frame > state.engine.frame)
        {
          self.tick_once(&mut state);
        }
      }
      SpectatingStrategy::Smooth => {
        if state.queue.is_empty() {
          return;
        }

        let last_frame = state.queue.last().unwrap().frame;

        if state.engine.frame < last_frame - 20 {
          while state
            .queue
            .iter()
            .any(|frame| frame.frame > state.engine.frame)
            && state.engine.frame < last_frame - 20
          {
            self.tick_once(&mut state);
          }
        }

        if state
          .queue
          .iter()
          .any(|frame| frame.frame > state.engine.frame)
        {
          self.tick_once(&mut state);
        }
      }
    };
  }

  pub async fn _set_spectating_strategy(&self, strategy: SpectatingStrategy) {
    let mut state = self.state.lock().await;
    state.strategy = strategy;
  }
}
