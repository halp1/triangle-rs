use std::{pin::Pin, sync::Arc};

use parking_lot::Mutex;
use tokio::select;

use crate::{
  classes::{
    game::Game,
    ribbon::{self, WrapError},
    room::Room,
    social,
  },
  types::{
    events::{recv, send},
    game::{Handling, SpectatingStrategy, tick},
    social::Config as SocialConfig,
  },
  utils::{
    api::{self, Api, core::ApiError},
    constants,
    events::{AsyncCallback, Event},
  },
};

use super::{ribbon::Ribbon, social::Social};

#[derive(Debug, Clone)]
pub struct ClientUser {
  pub id: String,
  pub username: String,
  pub role: crate::types::user::Role,
  pub session_id: String,
  pub user_agent: String,
}

#[derive(Debug, Clone)]
pub enum Credentials {
  Token(String),
  Credentials { username: String, password: String },
}

pub type RibbonOptions = super::ribbon::OptionalParams;

#[derive(Debug, Clone)]
pub struct ClientOptions {
  pub token: Credentials,
  pub game: Option<GameOptions>,
  pub user_agent: Option<String>,
  pub social: Option<SocialConfig>,
  pub ribbon: Option<RibbonOptions>,
}

impl ClientOptions {
  pub fn with_token(token: impl Into<String>) -> Self {
    Self {
      token: Credentials::Token(token.into()),
      game: None,
      user_agent: None,
      social: None,
      ribbon: None,
    }
  }

  pub fn with_token_and_handling(token: impl Into<String>, handling: Handling) -> Self {
    Self {
      token: Credentials::Token(token.into()),
      game: Some(GameOptions {
        handling: Some(handling),
        spectating_strategy: None,
      }),
      user_agent: None,
      social: None,
      ribbon: None,
    }
  }
}

#[derive(Debug, Clone)]
pub struct GameOptions {
  pub handling: Option<Handling>,
  pub spectating_strategy: Option<SpectatingStrategy>,
}

impl Default for GameOptions {
  fn default() -> Self {
    Self {
      handling: None,
      spectating_strategy: Some(SpectatingStrategy::Instant),
    }
  }
}

#[derive(Debug, Clone)]
pub struct ClientState {
  pub disconnected: bool,
  pub handling: Handling,
  pub spectating_strategy: SpectatingStrategy,
}

#[derive(Debug, Clone)]
struct RibbonConfig {
  options: ribbon::Options,
  transport: ribbon::Transport,
  user_agent: String,
}

#[derive(Debug, Clone)]
pub struct Client {
  pub user: ClientUser,
  pub token: String,
  pub ribbon: Ribbon,
  pub social: Social,
  pub room: Arc<Mutex<Option<Room>>>,
  pub game: Arc<Mutex<Option<Game>>>,
  pub api: Arc<Api>,
  pub state: Arc<Mutex<ClientState>>,
  ribbon_config: RibbonConfig,
}

impl Client {
  pub async fn new(options: ClientOptions) -> Result<Self, ApiError> {
    let user_agent = options
      .user_agent
      .clone()
      .unwrap_or_else(|| constants::USER_AGENT.to_string());

    let mut api_config = api::Config {
      token: "".into(),
      user_agent: user_agent.clone(),
      transport: match options
        .ribbon
        .clone()
        .unwrap_or_default()
        .transport
        .unwrap_or_default()
      {
        super::ribbon::Transport::JSON => api::Transport::Binary,
      },
    };

    let mut api = Api::new(api_config.clone());

    api_config.token = match &options.token {
      Credentials::Token(t) => t.clone(),
      Credentials::Credentials { username, password } => {
        api.users.authenticate(username, password).await?.token
      }
    };

    api.update(api_config);

    let api = Arc::new(api);

    let me = api.users.me().await?;

    let handling = options
      .game
      .as_ref()
      .and_then(|g| g.handling.clone())
      .unwrap_or_default();

    let spectating_strategy = options
      .game
      .as_ref()
      .and_then(|g| g.spectating_strategy.clone())
      .unwrap_or(SpectatingStrategy::Instant);

    let ribbon_config = RibbonConfig {
      options: options
        .ribbon
        .clone()
        .unwrap_or_default()
        .options
        .unwrap_or_default(),
      transport: options
        .ribbon
        .clone()
        .unwrap_or_default()
        .transport
        .unwrap_or_default(),
      user_agent: user_agent.clone(),
    };

    let session_id = format!("SESS-{}", rand::random::<u64>());
    let ribbon = Ribbon::new(ribbon::Params {
      handling: handling.clone(),
      options: ribbon_config.options.clone(),
      token: api.config.token.clone(),
      transport: ribbon_config.transport.clone(),
      user_agent: ribbon_config.user_agent.clone(),
    })
    .await?;

    ribbon.open();

    let res: Arc<Mutex<Option<std::result::Result<recv::client::Ready, String>>>> =
      Arc::new(Mutex::new(None));
    select! {
      biased;
      ready = ribbon.wait::<recv::client::Ready>() => {
        res.lock().replace(ready.map_or_else(|| Err(format!("Failed to connect: server disconnected")), |v| Ok(v)));
      }

      _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
        res.lock().replace(Err("Failed to connect: Connection timeout".to_string()));
      }
    }

    let res = res
      .lock()
      .take()
      .unwrap_or_else(|| Err("Failed to connect: unknown error".to_string()));

    let ready = res.map_err(|e| ApiError::Server(e))?;

    let user = ClientUser {
      id: me.id,
      username: me.username,
      role: me.role,
      session_id: session_id.clone(),
      user_agent: user_agent.clone(),
    };

    let client = Self {
      user: user.clone(),
      token: api.config.token.clone(),
      ribbon: ribbon.clone(),
      ribbon_config: ribbon_config.clone(),
      social: Social::new(
        ribbon.clone(),
        user.clone(),
        options.social.clone().unwrap_or_default(),
        ready.social,
      )
      .await,
      api,
      state: Arc::new(Mutex::new(ClientState {
        handling,
        spectating_strategy,
        disconnected: false,
      })),
      room: Arc::new(Mutex::new(None)),
      game: Arc::new(Mutex::new(None)),
    };

    client.init().await;

    Ok(client)
  }

  async fn init(&self) {
    let ribbon = self.ribbon.clone();
    let room = self.room.clone();
    let me = self.user.clone();
    let game = self.game.clone();
    let strategy = self.spectating_strategy();
    self.ribbon.on::<recv::room::Join>(async move |_| {
      let update = ribbon.wait::<recv::room::Update>().await;
      if let Some(update) = update {
        let r = Room::new(ribbon.clone(), game.clone(), me.clone(), update, strategy).await;
        room.lock().replace(r);
        ribbon.emit(send::client::room::Join {}).await.ok();
      }
    });

    // self
    //   .ribbon
    //   .on("server.announcement", move |data: Value| {
    //     let msg = data["msg"].as_str().unwrap_or("").to_string();
    //     let announcement_type = data["type"].as_str().unwrap_or("");
    //     let reason = data["reason"].as_str().map(str::to_string);

    //     let color = if announcement_type == "maintenance" {
    //       "#FF8A00"
    //     } else {
    //       "#FFCC00"
    //     };

    //     emitter.emit(
    //       "client.notify",
    //       serde_json::json!({
    //         "msg": msg,
    //         "color": color,
    //         "icon": "announcement",
    //         "type": announcement_type,
    //         "reason": reason,
    //       }),
    //     );
    //   });

    // let emitter = self.ribbon.emitter.clone();
    // self.ribbon.emitter.on("notify", move |data: Value| {
    //   if data.is_string() {
    //     emitter.emit("client.notify", serde_json::json!({ "msg": data }));
    //   } else if let Some(t) = data["type"].as_str() {
    //     let msg = data["msg"].as_str().unwrap_or("").to_string();
    //     match t {
    //       "err" => {
    //         emitter.emit("client.error", serde_json::json!(msg.clone()));
    //         emitter.emit(
    //           "client.notify",
    //           serde_json::json!({ "msg": msg, "color": "#FF4200", "icon": "error" }),
    //         );
    //       }
    //       "deny" => emitter.emit(
    //         "client.notify",
    //         serde_json::json!({ "msg": msg, "color": "#FF2200", "icon": "denied" }),
    //       ),
    //       "warn" => emitter.emit(
    //         "client.notify",
    //         serde_json::json!({ "msg": msg, "color": "#FFF43C", "icon": "warning" }),
    //       ),
    //       "announce" => emitter.emit(
    //         "client.notify",
    //         serde_json::json!({
    //           "msg": msg,
    //           "color": "#FFCC00",
    //           "icon": "announcement",
    //           "reason": data["reason"].as_str().map(str::to_string)
    //         }),
    //       ),
    //       "ok" => emitter.emit(
    //         "client.notify",
    //         serde_json::json!({ "msg": msg, "color": "#6AFF3C", "icon": "ok" }),
    //       ),
    //       _ => emitter.emit("client.notify", serde_json::json!({ "msg": msg })),
    //     }
    //   }
    // });

    let state = self.state.clone();

    self.ribbon.on::<recv::client::Dead>(|_| async move {
      state.lock().disconnected = true;
    });
  }

  pub fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> tokio::task::JoinHandle<()> {
    self.ribbon.on(callback)
  }

  pub fn once<T: Event>(
    &self,
    callback: impl Fn(T) + Send + Sync + 'static,
  ) -> tokio::task::JoinHandle<()> {
    self.ribbon.once::<T>(callback)
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    self.ribbon.wait::<T>().await
  }

  pub async fn emit<T: Event>(&mut self, event: T) {
    self.ribbon.emit(event).await.ok();
  }

  pub async fn wrap<T: Event>(&mut self, event: impl Event) -> std::result::Result<T, WrapError> {
    self.ribbon.wrap(event).await
  }

  pub async fn wrap_with_error<T: Event>(
    &mut self,
    event: impl Event,
    error_events: &[&str],
  ) -> std::result::Result<T, WrapError> {
    self.ribbon.wrap_with_error(event, error_events).await
  }

  pub async fn set_spectating_strategy(&mut self, strategy: SpectatingStrategy) {
    self.state.lock().spectating_strategy = strategy;
    self
      .room
      .lock()
      .as_mut()
      .map(|r| r._set_spectating_strategy(strategy));
  }

  pub async fn join_room(&self, room_id: &str) -> Result<(), WrapError> {
    self
      .ribbon
      .wrap::<recv::client::room::Join>(send::room::Join(room_id.to_string()))
      .await
      .map(|_| ())
  }

  pub async fn create_room(&self, public: bool) -> Result<(), WrapError> {
    self
      .ribbon
      .wrap::<recv::client::room::Join>(send::room::Create(public))
      .await
      .map(|_| ())
  }

  pub async fn list_rooms(&self) -> Result<Vec<api::rooms::Room>, ApiError> {
    self.api.rooms.list().await
  }

  pub fn room(&self) -> Option<Room> {
    self.room.lock().clone()
  }

  pub fn game(&self) -> Option<Game> {
    self.game.lock().clone()
  }

  pub fn spectating_strategy(&self) -> SpectatingStrategy {
    self.state.lock().spectating_strategy
  }

  pub fn handling(&self) -> Handling {
    self.state.lock().handling.clone()
  }

  /// Returns ok if successfully registered, error if failed (e.g. not in game)
  pub async fn register_ticker(
    &self,
    func: impl Fn(tick::In) -> Pin<Box<dyn Future<Output = tick::Out> + Send + 'static>>
    + Send
    + Sync
    + 'static,
  ) -> Result<(), ()> {
    let ticker = {
      let game = self.game.lock();
      game
        .as_ref()
        .and_then(|g| g.me.as_ref())
        .map(|me| me.tick.clone())
    };
    if let Some(ticker) = ticker {
      ticker.inject(func).await;
      Ok(())
    } else {
      Err(())
    }
  }

  // /**
  //  * Reconnect the client to TETR.IO.
  //  * @throws {Error} if the client is already connected
  //  */
  // async reconnect() {
  //   if (!this.disconnected) {
  //     throw new Error("Client is not disconnected.");
  //   }

  //   const newRibbon = await this.ribbon.clone();
  //   this.ribbon.destroy();
  //   this.ribbon = newRibbon;

  //   const data = await new Promise<Events.in.Client["client.ready"]>(
  //     (resolve, reject) => {
  //       const t = setTimeout(() => {
  //         newRibbon.destroy();
  //         reject("Failed to connect");
  //       }, 5000);
  //       this.ribbon.emitter.once("client.ready", (d) => {
  //         if (d) {
  //           clearTimeout(t);
  //           resolve(d);
  //         }
  //       });
  //     }
  //   );
  //   delete this.room;
  //   this.social = Social.create(this, this.social.config, data.social);
  // }

  pub async fn reconnect(&mut self) -> Result<(), String> {
    if !self.state.lock().disconnected {
      return Err("Client is not disconnected.".to_string());
    }

    self.social.destroy();

    let new_ribbon = Ribbon::new(ribbon::Params {
      handling: self.handling(),
      options: self.ribbon_config.options.clone(),
      token: self.token.clone(),
      transport: self.ribbon_config.transport.clone(),
      user_agent: self.ribbon_config.user_agent.clone(),
    })
    .await
    .map_err(|e| e.to_string())?;
    new_ribbon.emitter.transfer_from(&self.ribbon.emitter);
    self.ribbon = new_ribbon;

    let res: Arc<Mutex<Option<std::result::Result<recv::client::Ready, String>>>> =
      Arc::new(Mutex::new(None));
    select! {
      biased;
      ready = self.ribbon.wait::<recv::client::Ready>() => {
        res.lock().replace(ready.map_or_else(|| Err(format!("Failed to connect: server disconnected")), |v| Ok(v)));
      }

      _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
        res.lock().replace(Err("Failed to connect: Connection timeout".to_string()));
      }
    }

    let res = res
      .lock()
      .take()
      .unwrap_or_else(|| Err("Failed to connect: unknown error".to_string()));

    let ready = res.map_err(|e| e.to_string())?;

    self.state.lock().disconnected = false;
    *self.room.lock() = None;
    let social_config = self.social.config.lock().clone();
    self.social = Social::new(
      self.ribbon.clone(),
      self.user.clone(),
      social_config,
      ready.social,
    )
    .await;

    Ok(())
  }

  pub async fn destroy(&self) {
    let room = { self.room.lock().take() };
    if let Some(room) = room {
      room.destroy().await;
    }

    self.ribbon.destroy().await;
  }
}
