use std::{sync::Arc, time::Duration};

use serde_json::Value;
use tokio::sync::Mutex;

use crate::{
  classes::{ClientUser, Ribbon, game::Game, ribbon},
  types::{
    events::{
      recv::{self, room::Update},
      send,
    },
    game::Options as GameOptions,
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
  pub game_start: Option<u64>,
  pub chats: Vec<recv::room::Chat>,
}

#[derive(Debug)]
pub struct Room {
  ribbon: Ribbon,
  hook: ribbon::Hook,
  game: Arc<Mutex<Option<Game>>>,
  me: ClientUser,

  pub state: Arc<Mutex<RoomState>>,
}

impl Room {
  pub fn new(
    ribbon: Ribbon,
    game: Arc<Mutex<Option<Game>>>,
    me: ClientUser,
    update: recv::room::Update,
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
      })),
    };

    Self::handle_update(room.state.clone(), _update);

    room.init();

    room
  }

  async fn handle_update(state: Arc<Mutex<RoomState>>, update: recv::room::Update) {
    let mut state = state.lock().await;
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
    let mut ribbon = self.ribbon.clone();
    let state = self.state.clone();

    self
      .hook
      .on::<recv::room::update::Host>(async move |event| {
        self.state.lock().await.owner = event.0;

        ribbon
          .emit(send::client::room::Players(
            state.lock().await.players.clone(),
          ))
          .await;
      })
      .await;

    self
      .hook
      .on::<recv::room::update::Auto>(async move |event| {
        state.lock().await.auto = event;
      })
      .await;

    self
      .hook
      .on::<recv::room::Update>(async move |update| {
        Self::handle_update(state.clone(), update).await;
      })
      .await;

    self
      .hook
      .on::<recv::room::player::Add>(async move |event| {
        state.lock().await.players.push(event.0);

        ribbon
          .emit(send::client::room::Players(
            state.lock().await.players.clone(),
          ))
          .await;
      })
      .await;

    self
      .hook
      .on::<recv::room::player::Remove>(async move |event| {
        state.lock().await.players.retain(|p| p.id != event.0);

        ribbon
          .emit(send::client::room::Players(
            state.lock().await.players.clone(),
          ))
          .await;
      })
      .await;

    // TODO: game hooks

    self
      .hook
      .on::<recv::room::Chat>(async move |event| {
        state.lock().await.chats.push(event);
      })
      .await;

    let hook = self.hook.clone();

    self
      .hook
      .on::<recv::room::Leave>(async move |_| {
        hook.destroy().await;
        if let Some(game) = self.game.lock().await.take() {
          game.destroy().await;
          drop(game);
          ribbon
            .emit(send::client::game::Over::Leave)
            .await;
        }
      })
      .await;

    self
      .hook
      .on::<recv::room::Kick>(async move |_| {
        hook.destroy().await;
        if let Some(game) = self.game.lock().await.take() {
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
    let state = self.state.lock().await.clone();
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
      .hook
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
      .hook
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
      .hook
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
}
