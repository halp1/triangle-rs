use std::sync::Arc;

use tokio::{select, sync::Mutex};

use crate::{
  classes::{game::Game, ribbon::{self, WrapError}, room::Room},
  types::{
    events::{recv, send},
    game::{Handling, SpectatingStrategy},
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

pub struct Client {
  pub user: ClientUser,
  pub disconnected: bool,
  pub token: String,
  pub ribbon: Ribbon,
  pub social: Social,
  pub room: Arc<Mutex<Option<Room>>>,
  pub game: Arc<Mutex<Option<Game>>>,
  pub api: Arc<Api>,
  handling: Handling,
  spectating_strategy: SpectatingStrategy,
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
        super::ribbon::Transport::JSON => api::Transport::JSON,
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

    let session_id = format!("SESS-{}", rand::random::<u64>());
    let mut ribbon = Ribbon::new(ribbon::Params {
      handling: handling.clone(),
      options: options
        .ribbon
        .clone()
        .unwrap_or_default()
        .options
        .unwrap_or_default(),
      token: api.config.token.clone(),
      transport: options
        .ribbon
        .clone()
        .unwrap_or_default()
        .transport
        .unwrap_or_default(),
      user_agent: api.config.user_agent.clone(),
    })
    .await?;

    ribbon.open();

    let res: Arc<Mutex<Option<std::result::Result<recv::client::Ready, String>>>> =
      Arc::new(Mutex::new(None));
    select! {
      biased;
      ready = ribbon.wait::<recv::client::Ready>() => {
        res.lock().await.replace(ready.map_or_else(|| Err(format!("Failed to connect: server disconnected")), |v| Ok(v)));
      }

      _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
        res.lock().await.replace(Err("Failed to connect: Connection timeout".to_string()));
      }
    }

    let res = res
      .lock()
      .await
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
      social: Social::new(
        ribbon.clone(),
        user.clone(),
        options.social.clone().unwrap_or_default(),
        ready.social,
      )
      .await,
      api,
      handling,
      spectating_strategy,
      disconnected: false,
      room: Arc::new(Mutex::new(None)),
      game: Arc::new(Mutex::new(None)),
    };

    client.init().await;

    Ok(client)
  }

  async fn init(&self) {
    let mut ribbon = self.ribbon.clone();
    let room = self.room.clone();
    let me = self.user.clone();
    let game = self.game.clone();
    self
      .ribbon
      .on::<recv::room::Join>(async move |_| {
        let update = ribbon.wait::<recv::room::Update>().await;
        if let Some(update) = update {
          room
            .lock()
            .await
            .replace(Room::new(ribbon.clone(), game.clone(), me.clone(), update));
          // TODO: set client.room idk how to do that
          ribbon.emit(send::client::room::Join {}).await;
        }
      })
      .await;

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
  }

  pub async fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> tokio::task::JoinHandle<()> {
    self.ribbon.on(callback).await
  }

  pub async fn once<T: Event>(
    &self,
    callback: impl Fn(T) + Send + Sync + 'static,
  ) -> tokio::task::JoinHandle<()> {
    self.ribbon.once::<T>(callback).await
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    self.ribbon.wait::<T>().await
  }

  pub async fn emit<T: Event>(&mut self, event: T) {
    self.ribbon.emit(event).await;
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
}
