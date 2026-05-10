use std::sync::Arc;

use futures_util::future::join_all;
use tokio::sync::Mutex;

use crate::{
  classes::{
    ClientUser,
    game::{me::Me, player::Player},
  },
  types::game::{ReadyPlayer, SpectateTarget, SpectatingStrategy},
  utils::Logger,
};

use super::ribbon::{Hook, Ribbon};

pub mod me;
pub mod player;

pub const FRAMES_PER_SECOND: u64 = 60;

#[derive(Debug, Clone)]
pub struct GameState {
  pub strategy: SpectatingStrategy,
  pub spectating_loop_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>, // very cursed, maybe a better way?
  pub spectate_warning_counter: u64,
  pub players: Vec<Player>,
}

#[derive(Clone, Debug)]
pub struct Game {
  ribbon: Ribbon,
  hook: Hook,
  logger: Logger,

  pub me: Option<Me>,
  pub state: Arc<Mutex<GameState>>,
  pub raw_players: Arc<Vec<ReadyPlayer>>,
}

impl Game {
  pub async fn new(
    ribbon: Ribbon,
    user: ClientUser,
    raw_players: Vec<ReadyPlayer>,
    strategy: SpectatingStrategy,
  ) -> Self {
    let me = raw_players
      .iter()
      .find(|p| p.userid == user.id)
      .map(|_| Me::new(ribbon.clone(), user.clone(), raw_players.clone()));

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
      logger: Logger::new("triangle-rs"),
      me,
      state: Arc::new(Mutex::new(GameState {
        strategy,
        spectating_loop_handle: Arc::new(Mutex::new(None)),
        spectate_warning_counter: 0,
        players,
      })),
      raw_players: Arc::new(raw_players),
    };

    s.start_spectating_loop().await;

    s
  }

  async fn start_spectating_loop(&self) {
    let mut game = self.clone();

    self.state.lock().await.spectating_loop_handle.lock().await.replace(tokio::task::spawn(async move {
			loop {
				let start = std::time::Instant::now();

				for player in game.state.lock().await.players.clone() {
					player._tick().await;
				}

				if start.elapsed() > std::time::Duration::from_millis(1000 / FRAMES_PER_SECOND - 1) {
					let mut state = game.state.lock().await;
					state.spectate_warning_counter += 1;
					if state.spectate_warning_counter == 5 {
						game.logger.warn(
							"Spectating is falling behind! You are spectating too many players. Consider reducing the number of players you are spectating to improve performance."
						);
					}
				} else {
					game.state.lock().await.spectate_warning_counter = 0;
				}

				tokio::time::sleep((std::time::Duration::from_millis(1000 / FRAMES_PER_SECOND) - start.elapsed()).max(std::time::Duration::from_micros(50))).await;
			}
		}));
  }

	pub async fn _set_strategy(&self, strategy: SpectatingStrategy) {
		self.state.lock().await.strategy = strategy;
		for player in self.state.lock().await.players.clone() {
			player._set_strategy(strategy).await;
		}
	}

  pub async fn spectate(&self, target: SpectateTarget) -> Result<(), Vec<usize>> {
    let players = {
      let state = self.state.lock().await;
      state.players.clone()
    };

    let to_spectate: Vec<Option<Player>> = match &target {
      SpectateTarget::All => players.iter().map(|p| Some(p.clone())).collect(),
      SpectateTarget::GameIds(ids) => {
        if ids.is_empty() {
          return Ok(());
        }
        ids.iter()
          .map(|id| players.iter().find(|p| p.gameid == *id).cloned())
          .collect()
      }
      SpectateTarget::UserIds(uids) => {
        if uids.is_empty() {
          return Ok(());
        }
        uids.iter()
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

    if invalid.is_empty() { Ok(()) } else { Err(invalid) }
  }

  pub async fn unspectate(&self, target: SpectateTarget) -> Result<(), Vec<usize>> {
    let players = {
      let state = self.state.lock().await;
      state.players.clone()
    };

    let to_unspectate: Vec<Option<Player>> = match &target {
      SpectateTarget::All => players.iter().map(|p| Some(p.clone())).collect(),
      SpectateTarget::GameIds(ids) => {
        if ids.is_empty() {
          return Ok(());
        }
        ids.iter()
          .map(|id| players.iter().find(|p| p.gameid == *id).cloned())
          .collect()
      }
      SpectateTarget::UserIds(uids) => {
        if uids.is_empty() {
          return Ok(());
        }
        uids.iter()
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

    if invalid.is_empty() { Ok(()) } else { Err(invalid) }
  }

  pub async fn destroy(mut self) {
    if let Some(mut me) = self.me.take() {
      me.destroy().await;
      self.me = None;
    }

    let players = self.state.lock().await.players.clone();

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
