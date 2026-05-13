use std::{sync::Arc, time::Duration};

use serde_json::Value;
use parking_lot::Mutex;
use tokio::sync::Mutex as TMutex;

use crate::{
  classes::{ClientUser, Ribbon, game::Game, ribbon},
  types::{
    events::{
      recv::{self},
      send,
    },
    game::{Options as GameOptions, SpectatingStrategy},
    room::{
      Autostart, Bracket, Match, Player, SetConfigItem, SetConfigItemRaw, SetConfigValue, State,
      Type,
    },
  },
  utils::Partial,
};

#[derive(Debug, Clone)]
pub struct RoomState {
  pub id: String,
  pub public: bool,
  pub room_type: Type,
  pub name: String,
  pub name_safe: Option<String>,
  pub owner: String,
  pub creator: String,
  pub state: State,
  pub auto: Autostart,
  pub match_config: Match,
  pub players: Vec<Player>,
  pub user_limit: u32,
  pub allow_chat: Option<bool>,
  pub allow_anonymous: bool,
  pub allow_unranked: bool,
  pub allow_queued: bool,
  pub allow_bots: bool,
  pub user_rank_limit: crate::types::game::Rank,
  pub use_best_rank_as_limit: bool,
  pub lobbybg: Option<String>,
  pub lobbybgm: String,
  pub gamebgm: String,
  pub force_require_xp_to_chat: bool,
  pub options: GameOptions,
  pub game_start: Option<std::time::Instant>,
  pub chats: Vec<recv::room::Chat>,

  spectating_strategy: SpectatingStrategy,
}

#[derive(Debug, Clone)]
pub struct Room {
  ribbon: Ribbon,
  hook: ribbon::Hook,
  game: Arc<TMutex<Option<Game>>>,
  me: ClientUser,

  pub state: Arc<Mutex<RoomState>>,
}

impl Room {
  pub async fn new(
    ribbon: Ribbon,
    game: Arc<TMutex<Option<Game>>>,
    me: ClientUser,
    update: recv::room::Update,
    spectating_strategy: SpectatingStrategy,
  ) -> Self {
    let _update = update.clone();
    let room = Self {
      ribbon: ribbon.clone(),
      hook: ribbon.hook(),
      game,
      me,
      state: Arc::new(Mutex::new(RoomState {
        id: update.id,
        public: update.public,
        room_type: update.r#type,
        name: update.name,
        name_safe: update.name_safe,
        owner: update.owner,
        creator: update.creator,
        state: update.state,
        auto: update.auto,
        match_config: update.r#match,
        players: update.players,
        user_limit: update.user_limit,
        allow_chat: update.allow_chat,
        allow_anonymous: update.allow_anonymous,
        allow_unranked: update.allow_unranked,
        allow_queued: update.allow_queued,
        allow_bots: update.allow_bots,
        user_rank_limit: update.user_rank_limit,
        use_best_rank_as_limit: update.use_best_rank_as_limit,
        lobbybg: update.lobbybg,
        lobbybgm: update.lobbybgm,
        gamebgm: update.gamebgm,
        force_require_xp_to_chat: update.force_require_xp_to_chat,
        options: GameOptions::default(),
        game_start: None,
        chats: vec![],
        spectating_strategy,
      })),
    };

    Self::handle_update(room.state.clone(), _update).await;

    room.init().await;

    room
  }

  async fn handle_update(state: Arc<Mutex<RoomState>>, update: recv::room::Update) {
    let mut state = state.lock();
    state.id = update.id;
    state.public = update.public;
    state.room_type = update.r#type;
    state.name = update.name;
    state.name_safe = update.name_safe;
    state.owner = update.owner;
    state.creator = update.creator;
    state.state = update.state;
    state.auto = update.auto;
    state.match_config = update.r#match;
    state.players = update.players;
    state.user_limit = update.user_limit;
    state.allow_chat = update.allow_chat;
    state.allow_anonymous = update.allow_anonymous;
    state.allow_unranked = update.allow_unranked;
    state.allow_queued = update.allow_queued;
    state.allow_bots = update.allow_bots;
    state.user_rank_limit = update.user_rank_limit;
    state.use_best_rank_as_limit = update.use_best_rank_as_limit;
    state.lobbybg = update.lobbybg;
    state.lobbybgm = update.lobbybgm;
    state.gamebgm = update.gamebgm;
    state.force_require_xp_to_chat = update.force_require_xp_to_chat;

    state.options = state
      .options
      .clone()
      .merge(update.options.unwrap_or_default());
  }

  async fn init(&self) {
    let ribbon = self.ribbon.clone();
    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::update::Host>(async move |event| {
        state.lock().owner = event.0;

        ribbon
          .emit(send::client::room::Players(
            state.lock().players.clone(),
          ))
          .await;
      })
      .await;

    let state = self.state.clone();
    self
      .hook
      .on::<recv::room::update::Auto>(async move |event| {
        state.lock().auto = event;
      })
      .await;

    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::Update>(async move |update| {
        Self::handle_update(state.clone(), update).await;
      })
      .await;

    let ribbon = self.ribbon.clone();
    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::player::Add>(async move |event| {
        state.lock().players.push(event.0);

        ribbon
          .emit(send::client::room::Players(
            state.lock().players.clone(),
          ))
          .await;
      })
      .await;

    let ribbon = self.ribbon.clone();
    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::player::Remove>(async move |event| {
        state.lock().players.retain(|p| p.id != event.0);

        ribbon
          .emit(send::client::room::Players(
            state.lock().players.clone(),
          ))
          .await;
      })
      .await;

    let ribbon = self.ribbon.clone();
    let state = self.state.clone();
    let game = self.game.clone();
    let me = self.me.clone();

    self
      .hook
      .on::<recv::game::Ready>(async move |data| {
        let spectating_strategy = state.lock().spectating_strategy.clone();
        let g = Game::new(
          ribbon.clone(),
          me,
          data.players.clone(),
          spectating_strategy,
        )
        .await;

        game.lock().replace(g);

        if data.is_new {
          state.lock().game_start = Some(std::time::Instant::now());

          // TODO: replay generator

          let r#match = state.lock().match_config.clone();

          ribbon
            .emit(send::client::game::Start {
              multi: r#match.ft > 1 || r#match.wb > 1,
              first_to: r#match.ft,
              win_by: r#match.wb,
              golden_point: r#match.gp,
              players: data
                .players
                .iter()
                .map(|p| {
                  (
                    p.userid.clone(),
                    p.options["username"]
                      .as_str()
                      .unwrap_or_default()
                      .to_string(),
                  )
                })
                .collect(),
            })
            .await;
        }
      })
      .await;

    // TODO: pipe replay data to replay generator

    let ribbon = self.ribbon.clone();
    let game = self.game.clone();

    self
      .hook
      .on::<recv::game::replay::End>(async move |data| {
        let mut me = {
          // TODO: die in replay generator
          let game = game.lock();
          let gameid = game.as_ref().and_then(|g| g.me.as_ref().map(|m| m.gameid));
          if game.is_none() || gameid.map_or(true, |p| p != data.gameid) {
            return;
          }
          game.as_ref().unwrap().me.as_ref().unwrap().clone()
        };

        me.destroy().await;

        ribbon
          .emit(send::client::game::Over::Finish(data.data))
          .await;
      })
      .await;

    let ribbon = self.ribbon.clone();
    let game = self.game.clone();

    self
      .hook
      .on::<recv::game::Advance>(async move |_| {
        // TODO: end round in replay generator

        if let Some(mut game) = game.lock().take() {
          game.destroy().await;
          ribbon.emit(send::client::game::Over::End).await;
        }
      })
      .await;

    let ribbon = self.ribbon.clone();
    let game = self.game.clone();

    self
      .hook
      .on::<recv::game::Score>(async move |data| {
        if let Some(mut g) = game.lock().take() {
          g.destroy().await;
        }

        ribbon
          .emit(send::client::game::round::End(
            data.scoreboard.first().map(|s| s.id.clone()),
          ))
          .await;
      })
      .await;

    let ribbon = self.ribbon.clone();
    let game = self.game.clone();
    let aborting = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    self
      .hook
      .on::<recv::game::Abort>(async move |_| {
        if aborting.swap(true, std::sync::atomic::Ordering::SeqCst) {
          return;
        }

        ribbon.emit(send::client::game::Abort).await;

        if let Some(mut g) = game.lock().take() {
          g.destroy().await;
          ribbon.emit(send::client::game::Over::Abort).await;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        aborting.store(false, std::sync::atomic::Ordering::SeqCst);
      })
      .await;

    let ribbon = self.ribbon.clone();
    let game = self.game.clone();
    let state = self.state.clone();

    self
      .hook
      .on::<recv::game::End>(async move |data| {
        let (use_scoreboard, game_start, match_config) = {
          let s = state.lock();
          (
            s.match_config.ft == 1 && s.match_config.wb == 1,
            s.game_start,
            s.match_config.clone(),
          )
        };

        let _ = match_config;

        let round_winner = if use_scoreboard {
          data
            .scoreboard
            .as_ref()
            .and_then(|sb| sb.first().map(|s| s.id.clone()))
        } else {
          data
            .leaderboard
            .as_ref()
            .and_then(|lb| lb.first().map(|l| l.id.clone()))
        };

        ribbon
          .emit(send::client::game::round::End(round_winner))
          .await;

        let duration_ms = game_start
          .map(|s| s.elapsed().as_secs_f64() * 1000.0)
          .unwrap_or(0.0);

        let end_event = if use_scoreboard {
          let scoreboard = data.scoreboard.unwrap_or_default();
          send::client::game::End {
            duration_ms,
            source: send::client::game::EndSource::Scoreboard,
            players: scoreboard
              .iter()
              .map(|item| send::client::game::EndPlayer {
                id: item.id.clone(),
                name: item.username.clone(),
                points: if item.alive && item.active { 1 } else { 0 },
                won: item.alive && item.active,
                lifetime: Some(item.lifetime),
                raw: serde_json::to_value(item).unwrap_or_default(),
              })
              .collect(),
          }
        } else {
          let leaderboard = data.leaderboard.unwrap_or_default();
          let max_wins = leaderboard.iter().map(|i| i.wins).max().unwrap_or(0);
          send::client::game::End {
            duration_ms,
            source: send::client::game::EndSource::Leaderboard,
            players: leaderboard
              .iter()
              .map(|item| send::client::game::EndPlayer {
                id: item.id.clone(),
                name: item.username.clone(),
                points: item.wins as i64,
                won: item.wins == max_wins,
                lifetime: None,
                raw: serde_json::to_value(item).unwrap_or_default(),
              })
              .collect(),
          }
        };

        ribbon.emit(end_event).await;

        if let Some(mut g) = game.lock().take() {
          g.destroy().await;
          ribbon.emit(send::client::game::Over::End).await;
        }
      })
      .await;

    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::Chat>(async move |event| {
        state.lock().chats.push(event);
      })
      .await;

    let ribbon = self.ribbon.clone();
    let hook = self.hook.clone();
    let game = self.game.clone();

    self
      .hook
      .on::<recv::room::Leave>(async move |_| {
        hook.destroy().await;
        if let Some(mut game) = game.lock().take() {
          game.destroy().await;
          drop(game);
          ribbon.emit(send::client::game::Over::Leave).await;
        }
      })
      .await;

    let ribbon = self.ribbon.clone();
    let hook = self.hook.clone();
    let game = self.game.clone();

    self
      .hook
      .on::<recv::room::Kick>(async move |_| {
        hook.destroy().await;
        if let Some(mut game) = game.lock().take() {
          game.destroy().await;
          drop(game);
          ribbon.emit(send::client::game::Over::Leave).await;
        }
      })
      .await;
  }

  pub async fn leave(&mut self) {
    self
      .ribbon
      .wrap::<recv::room::Leave>(send::room::Leave {})
      .await
      .ok();
  }

  pub async fn state(&self) -> RoomState {
    let state = self.state.lock().clone();
    state
  }

  pub async fn kick(&mut self, id: &str) -> Result<recv::room::player::Remove, ribbon::WrapError> {
    self.kick_with_duration(id, Duration::from_secs(900)).await
  }

  pub async fn kick_with_duration(
    &mut self,
    id: &str,
    duration: Duration,
  ) -> Result<recv::room::player::Remove, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::player::Remove>(send::room::Kick {
        uid: id.to_string(),
        duration: duration.as_secs_f64(),
      })
      .await
  }

  pub async fn ban(&mut self, id: &str) -> Result<recv::room::player::Remove, ribbon::WrapError> {
    self
      .kick_with_duration(id, Duration::from_secs(2592e3 as u64))
      .await
  }

  pub async fn unban(&mut self, id: &str) {
    self.ribbon.emit(send::room::Unban(id.to_string())).await;
  }

  pub async fn chat(&mut self, message: &str) -> Result<recv::room::Chat, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::Chat>(send::room::Chat {
        content: message.to_string(),
        pinned: false,
      })
      .await
  }

  pub async fn chat_pinned(
    &mut self,
    message: &str,
  ) -> Result<recv::room::Chat, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::Chat>(send::room::Chat {
        content: message.to_string(),
        pinned: true,
      })
      .await
  }

  pub async fn clear_chat(&mut self) -> Result<recv::room::chat::Clear, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::chat::Clear>(send::room::chat::Clear {})
      .await
  }

  pub async fn set_id(&mut self, id: &str) -> Result<recv::room::Update, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::Update>(send::room::SetId(id.to_ascii_uppercase()))
      .await
  }

  pub async fn update(
    &mut self,
    config: Vec<SetConfigItem>,
  ) -> Result<recv::room::Update, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::Update>(send::room::SetConfig(
        config
          .iter()
          .map(|item| SetConfigItemRaw {
            index: item.index.clone(),
            value: match &item.value {
              SetConfigValue::String(s) => Value::String(s.clone()),
              SetConfigValue::Number(n) => Value::String(
                serde_json::Number::from_f64(*n)
                  .unwrap_or_else(|| serde_json::Number::from(0))
                  .to_string(),
              ),
              SetConfigValue::Boolean(b) => Value::Number(if *b {
                serde_json::Number::from(1)
              } else {
                serde_json::Number::from(0)
              }),
            },
          })
          .collect(),
      ))
      .await
  }

  // pub (&mut self, async fn use_preset)
  // TODO: presets

  pub async fn start(&mut self) -> Result<recv::game::Ready, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::game::Ready>(send::room::Start {})
      .await
  }

  pub async fn abort(&mut self) -> Result<recv::game::Abort, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::game::Abort>(send::room::Abort {})
      .await
  }

  // TODO: spectating

  pub async fn transfer_host(
    &mut self,
    id: &str,
  ) -> Result<recv::room::update::Host, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::update::Host>(send::room::owner::Transfer(id.to_string()))
      .await
  }

  pub async fn take_host(&mut self) -> Result<recv::room::update::Host, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::update::Host>(send::room::owner::Revoke {})
      .await
  }

  /// Treats switching to observer as spectator
  pub async fn switch(
    &mut self,
    bracket: Bracket,
  ) -> Result<recv::room::update::Bracket, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::update::Bracket>(send::room::bracket::Switch(match bracket {
        Bracket::Observer => Bracket::Spectator,
        _ => bracket,
      }))
      .await
  }

  pub async fn move_player(
    &mut self,
    id: &str,
    bracket: Bracket,
  ) -> Result<recv::room::update::Bracket, ribbon::WrapError> {
    self
      .ribbon
      .wrap::<recv::room::update::Bracket>(send::room::bracket::Move {
        uid: id.to_string(),
        bracket,
      })
      .await
  }

  pub async fn _set_spectating_strategy(&mut self, strategy: SpectatingStrategy) {
    self.state.lock().spectating_strategy = strategy.clone();
    if let Some(game) = self.game.lock().as_ref().clone() {
      game._set_spectating_strategy(strategy).await;
    }
  }
}
