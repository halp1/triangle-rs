use std::sync::Arc;

use serde_json::Value;
use tokio::{select, sync::Mutex};

use crate::{
  classes::ribbon,
  error::{Result, TriangleError},
  types::{
    events::{recv, send},
    game::Handling,
    social::{Config as SocialConfig, Status},
    user::Me,
  },
  utils::{
    api::{self, Api},
    constants,
  },
};

use super::{
  game::{Game, SpectatingStrategy, parse_raw_players},
  ribbon::Ribbon,
  room::Room,
  social::Social,
};

#[derive(Debug, Clone)]
pub struct ClientUser {
  pub id: String,
  pub username: String,
  pub role: crate::types::user::Role,
  pub session_id: String,
  pub user_agent: String,
}

#[derive(Debug, Clone)]
pub enum TokenOrCredentials {
  Token(String),
  Credentials { username: String, password: String },
}

pub type RibbonOptions = super::ribbon::OptionalParams;

#[derive(Debug, Clone)]
pub struct ClientOptions {
  pub token: TokenOrCredentials,
  pub game: Option<GameOptions>,
  pub user_agent: Option<String>,
  pub social: Option<SocialConfig>,
  pub ribbon: Option<RibbonOptions>,
}

impl ClientOptions {
  pub fn with_token(token: impl Into<String>) -> Self {
    Self {
      token: TokenOrCredentials::Token(token.into()),
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
  pub social: Option<Social>,
  pub room: Option<Room>,
  pub game: Option<Game>,
  pub api: Arc<Api>,
  handling: Handling,
  spectating_strategy: SpectatingStrategy,
}

impl Client {
  pub async fn new(options: ClientOptions) -> Result<Self> {
    let user_agent = options
      .user_agent
      .clone()
      .unwrap_or_else(|| constants::USER_AGENT.to_string());

    let token = match &options.token {
      TokenOrCredentials::Token(t) => t.clone(),
      TokenOrCredentials::Credentials { username, password } => {
        let bootstrap_api = Api::new(api::Config {
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
        });
        let auth = bootstrap_api.users.authenticate(username, password).await?;
        auth.token
      }
    };

    let api = Api::new(api::Config {
      token,
      user_agent: user_agent.clone(),
      transport: match options
        .ribbon
        .unwrap_or_default()
        .transport
        .unwrap_or_default()
      {
        super::ribbon::Transport::JSON => api::Transport::JSON,
      },
    });

    let api = Arc::new(api);

    let me = api.users.me().await?;
    let env = api.server.environment().await?;
    let signature = env.signature.clone();
    let spool = api
      .server
      .spool(
        options
          .ribbon
          .clone()
          .unwrap_or_default()
          .options
          .unwrap_or_default()
          .spooling,
      )
      .await?;

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

    let session_id = crate::channel::random_session_id(20);
    let ribbon = Ribbon::new(options.ribbon.unwrap_or_default().into()).await?;

    ribbon.open();

    let mut res: Arc<Mutex<Option<std::result::Result<recv::client::Ready, String>>>> =
      Arc::new(Mutex::new(None));
    let mut r1 = res.clone();
    let mut r2 = res.clone();

    select! {
      biased;
      ready = ribbon.wait::<recv::client::Ready>() => {
        r2.lock().await.replace(ready.map_or_else(|| Err(format!("Failed to connect: server disconnected")), |v| Ok(v)));
      }

      _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
        let mut lock = r1.lock().await;
        *lock = Some(Err("Failed to connect: Connection timeout".to_string()));
      }
    }

    let res = res
      .lock()
      .await
      .take()
      .unwrap_or_else(|| Err("Failed to connect: unknown error".to_string()));

    let ready = res.map_err(|e| TriangleError::Ribbon(e))?;

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
      ribbon,
      social: options
        .social
        .map(|cfg| Social::new(user.clone(), cfg, ready)),
      room: None,
      game: None,
      api,
      handling,
      spectating_strategy,
      disconnected: false,
    };

    client.init().await;

    Ok(client)
  }

  async fn init(&self) {
    let mut ribbon = self.ribbon.clone();
    self
      .ribbon
      .on::<recv::room::Join>(async move |data| {
        ribbon.wait::<recv::room::Update>().await;
        // TODO: set client.room idk how to do that
        ribbon.emit::<send::client::room::Join>().await;
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
}
