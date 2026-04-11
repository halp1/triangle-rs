use std::sync::Arc;

use serde_json::Value;

use crate::{
  error::{Result, TriangleError},
  types::{
    game::Handling,
    social::{Config as SocialConfig, Status},
    user::Me,
  },
  utils::{Api, USER_AGENT},
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

#[derive(Debug, Clone)]
pub struct ClientOptions {
  pub token: TokenOrCredentials,
  pub game: Option<GameOptions>,
  pub user_agent: Option<String>,
  pub turnstile: Option<String>,
  pub social: Option<SocialConfig>,
}

impl ClientOptions {
  pub fn with_token(token: impl Into<String>) -> Self {
    Self {
      token: TokenOrCredentials::Token(token.into()),
      game: None,
      user_agent: None,
      turnstile: None,
      social: None,
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
  pub async fn create(options: ClientOptions) -> Result<Self> {
    let user_agent = options
      .user_agent
      .clone()
      .unwrap_or_else(|| USER_AGENT.to_string());

    let token = match &options.token {
      TokenOrCredentials::Token(t) => t.clone(),
      TokenOrCredentials::Credentials { username, password } => {
        let bootstrap_api = Api::new("", &user_agent);
        let auth = bootstrap_api.authenticate(username, password).await?;
        auth.token
      }
    };

    let mut api = Api::new(&token, &user_agent);
    if let Some(ts) = &options.turnstile {
      api = api.with_turnstile(ts);
    }
    let api = Arc::new(api);

    let me: Me = api.me().await?;
    let env = api.environment().await?;
    let signature = env.signature.clone();
    let spool = api.spool().await?;

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
    let ribbon = Ribbon::connect(
      token.clone(),
      handling.clone(),
      api.clone(),
      spool,
      signature,
    )
    .await?;

    let mut rx = ribbon.emitter.subscribe();
    let social_init_data = tokio::time::timeout(tokio::time::Duration::from_secs(15), async {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == "client.ready" => break Ok(data),
          Ok((cmd, data)) if cmd == "client.fail" => {
            let msg = data["message"]
              .as_str()
              .unwrap_or("ribbon connection failed")
              .to_string();
            break Err(TriangleError::Ribbon(msg));
          }
          Ok((cmd, data)) if cmd == "client.error" => {
            let msg = data.as_str().unwrap_or("authorize failed").to_string();
            break Err(TriangleError::Ribbon(msg));
          }
          Ok(_) => {}
          Err(tokio::sync::broadcast::error::RecvError::Closed) => {
            break Err(TriangleError::Ribbon(
              "ribbon channel closed unexpectedly".to_string(),
            ));
          }
          Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
        }
      }
    })
    .await
    .map_err(|_| TriangleError::Ribbon("connection timeout (15s)".to_string()))??;

    let user = ClientUser {
      id: me.id.clone(),
      username: me.username.clone(),
      role: me.role.clone(),
      session_id,
      user_agent: user_agent.clone(),
    };

    let social_cfg = options.social.unwrap_or_else(SocialConfig::default_config);
    let social = Social::new(&user, social_cfg, &social_init_data);

    let client = Self {
      user,
      disconnected: false,
      token,
      ribbon,
      social: Some(social),
      room: None,
      game: None,
      api,
      handling,
      spectating_strategy,
    };

    client.init();
    Ok(client)
  }

  fn init(&self) {
    let emitter = self.ribbon.emitter.clone();
    self
      .ribbon
      .emitter
      .on("server.announcement", move |data: Value| {
        let msg = data["msg"].as_str().unwrap_or("").to_string();
        let announcement_type = data["type"].as_str().unwrap_or("");
        let reason = data["reason"].as_str().map(str::to_string);

        let color = if announcement_type == "maintenance" {
          "#FF8A00"
        } else {
          "#FFCC00"
        };

        emitter.emit(
          "client.notify",
          serde_json::json!({
            "msg": msg,
            "color": color,
            "icon": "announcement",
            "type": announcement_type,
            "reason": reason,
          }),
        );
      });

    let emitter = self.ribbon.emitter.clone();
    self.ribbon.emitter.on("notify", move |data: Value| {
      if data.is_string() {
        emitter.emit("client.notify", serde_json::json!({ "msg": data }));
      } else if let Some(t) = data["type"].as_str() {
        let msg = data["msg"].as_str().unwrap_or("").to_string();
        match t {
          "err" => {
            emitter.emit("client.error", serde_json::json!(msg.clone()));
            emitter.emit(
              "client.notify",
              serde_json::json!({ "msg": msg, "color": "#FF4200", "icon": "error" }),
            );
          }
          "deny" => emitter.emit(
            "client.notify",
            serde_json::json!({ "msg": msg, "color": "#FF2200", "icon": "denied" }),
          ),
          "warn" => emitter.emit(
            "client.notify",
            serde_json::json!({ "msg": msg, "color": "#FFF43C", "icon": "warning" }),
          ),
          "announce" => emitter.emit(
            "client.notify",
            serde_json::json!({
              "msg": msg,
              "color": "#FFCC00",
              "icon": "announcement",
              "reason": data["reason"].as_str().map(str::to_string)
            }),
          ),
          "ok" => emitter.emit(
            "client.notify",
            serde_json::json!({ "msg": msg, "color": "#6AFF3C", "icon": "ok" }),
          ),
          _ => emitter.emit("client.notify", serde_json::json!({ "msg": msg })),
        }
      }
    });
  }

  pub fn on<F>(&self, event: &str, callback: F) -> tokio::task::JoinHandle<()>
  where
    F: Fn(Value) + Send + 'static,
  {
    self.ribbon.emitter.on(event, callback)
  }

  pub async fn wait(&self, event: &str) -> Option<Value> {
    self.ribbon.emitter.once(event).await
  }

  pub fn emit(&self, command: &str, data: Value) {
    self.ribbon.send(command, data);
  }

  pub async fn wrap(&self, command: &str, data: Value, listen_for: &str) -> Result<Value> {
    self
      .wrap_with_errors(command, data, listen_for, &["client.error"])
      .await
  }

  pub async fn wrap_with_errors(
    &self,
    command: &str,
    data: Value,
    listen_for: &str,
    error_events: &[&str],
  ) -> Result<Value> {
    let mut rx = self.ribbon.emitter.subscribe();
    self.ribbon.send(command, data);
    let listen_for = listen_for.to_string();

    loop {
      match rx.recv().await {
        Ok((cmd, d)) if cmd == listen_for => return Ok(d),
        Ok((cmd, d)) if error_events.iter().any(|e| *e == cmd) => {
          let msg = d
            .as_str()
            .unwrap_or("TETR.IO returned an error")
            .to_string();
          return Err(TriangleError::Ribbon(msg));
        }
        Ok(_) => {}
        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
          return Err(TriangleError::Ribbon("ribbon channel closed".to_string()));
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
      }
    }
  }

  pub fn handling(&self) -> &Handling {
    &self.handling
  }

  pub fn set_handling(&mut self, handling: Handling) -> Result<()> {
    if self.room.is_some() {
      return Err(TriangleError::InvalidArgument(
        "cannot change handling while in a room".to_string(),
      ));
    }
    self.emit("config.handling", serde_json::to_value(&handling)?);
    self.handling = handling;
    Ok(())
  }

  pub fn spectating_strategy(&self) -> &SpectatingStrategy {
    &self.spectating_strategy
  }

  pub fn set_spectating_strategy(&mut self, strategy: SpectatingStrategy) {
    self.spectating_strategy = strategy.clone();
    if let Some(game) = &mut self.game {
      game.spectating_strategy = strategy;
    }
  }

  pub async fn list_rooms(&self) -> Result<Vec<Value>> {
    self.api.list_rooms().await
  }

  pub async fn join_room(&mut self, code: &str) -> Result<Room> {
    let data = self
      .wrap(
        "room.join",
        Value::String(code.to_uppercase()),
        "room.update",
      )
      .await?;

    let room = Room::from_update(&data)
      .ok_or_else(|| TriangleError::Adapter("invalid room.update payload".to_string()))?;
    self.room = Some(room.clone());
    Ok(room)
  }

  pub async fn create_room(&mut self, private: bool) -> Result<Room> {
    let data = self
      .wrap("room.create", Value::Bool(!private), "room.update")
      .await?;

    let room = Room::from_update(&data)
      .ok_or_else(|| TriangleError::Adapter("invalid room.update payload".to_string()))?;
    self.room = Some(room.clone());
    Ok(room)
  }

  pub async fn leave_room(&mut self) -> Result<()> {
    if self.room.is_none() {
      return Ok(());
    }
    let _ = self.wrap("room.leave", Value::Null, "room.leave").await?;
    self.room = None;
    self.game = None;
    Ok(())
  }

  pub async fn start_room_game(&mut self) -> Result<()> {
    if self.room.is_none() {
      return Err(TriangleError::InvalidArgument("not in a room".to_string()));
    }

    let data = self.wrap("room.start", Value::Null, "game.ready").await?;
    let players = parse_raw_players(&data);

    if players.is_empty() {
      return Err(TriangleError::Adapter(
        "game.ready did not include players".to_string(),
      ));
    }

    self.game = Some(Game::new(
      self.ribbon.emitter.clone(),
      players,
      &self.user.id,
      self.spectating_strategy.clone(),
    ));

    Ok(())
  }

  pub async fn spectate_room_game(&mut self) -> Result<Value> {
    let data = self
      .wrap("game.spectate", Value::Null, "game.spectate")
      .await?;

    let players = parse_raw_players(&data);
    if players.is_empty() {
      return Err(TriangleError::Adapter(
        "game.spectate did not include players".to_string(),
      ));
    }

    self.game = Some(Game::new(
      self.ribbon.emitter.clone(),
      players,
      &self.user.id,
      self.spectating_strategy.clone(),
    ));

    Ok(data)
  }

  pub fn unspectate_game(&mut self) {
    if let Some(game) = &self.game {
      game.unspectate_all();
    }
    self.game = None;
  }

  pub async fn reconnect(&mut self) -> Result<()> {
    if !self.disconnected {
      return Err(TriangleError::InvalidArgument(
        "client is not disconnected".to_string(),
      ));
    }

    let env = self.api.environment().await?;
    let spool = self.api.spool().await?;
    let signature = env.signature;

    let new_ribbon = Ribbon::connect(
      self.token.clone(),
      self.handling.clone(),
      self.api.clone(),
      spool,
      signature,
    )
    .await?;

    let mut rx = new_ribbon.emitter.subscribe();
    let ready = tokio::time::timeout(tokio::time::Duration::from_secs(15), async {
      loop {
        match rx.recv().await {
          Ok((cmd, data)) if cmd == "client.ready" => break Ok(data),
          Ok((cmd, data)) if cmd == "client.fail" => {
            let msg = data["message"].as_str().unwrap_or("reconnect failed");
            break Err(TriangleError::Ribbon(msg.to_string()));
          }
          Ok((cmd, data)) if cmd == "client.error" => {
            let msg = data.as_str().unwrap_or("reconnect failed");
            break Err(TriangleError::Ribbon(msg.to_string()));
          }
          Ok(_) => {}
          Err(tokio::sync::broadcast::error::RecvError::Closed) => {
            break Err(TriangleError::Ribbon(
              "ribbon channel closed unexpectedly".to_string(),
            ));
          }
          Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
        }
      }
    })
    .await
    .map_err(|_| TriangleError::Ribbon("connection timeout (15s)".to_string()))??;

    self.ribbon.destroy();
    self.ribbon = new_ribbon;
    self.room = None;
    self.game = None;
    self.disconnected = false;

    if let Some(social) = &self.social {
      let cfg = social.config.clone();
      self.social = Some(Social::new(&self.user, cfg, &ready));
    }

    self.init();
    Ok(())
  }

  pub async fn destroy(&mut self) -> Result<()> {
    self.room = None;
    self.game = None;
    self.ribbon.destroy();
    self.disconnected = true;
    Ok(())
  }

  pub fn social_get(&self, target: &str) -> Option<&super::social::RelationshipEntry> {
    self.social.as_ref().and_then(|s| s.get(target))
  }

  pub async fn social_dm(&self, user_id: &str, message: &str) -> Result<Value> {
    let social = self
      .social
      .as_ref()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.dm(self, user_id, message).await
  }

  pub async fn social_dms_with_user(&self, user_id: &str) -> Result<Vec<crate::types::social::Dm>> {
    let social = self
      .social
      .as_ref()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.dms_with_user(self, user_id).await
  }

  pub async fn social_dms_with(
    &self,
    lookup: super::social::RelationshipLookup<'_>,
  ) -> Result<Vec<crate::types::social::Dm>> {
    let social = self
      .social
      .as_ref()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.dms_with(self, lookup).await
  }

  pub async fn social_friend(&mut self, user_id: &str) -> Result<bool> {
    let mut social = self
      .social
      .take()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    let res = social.friend(self, user_id).await;
    self.social = Some(social);
    res
  }

  pub async fn social_unfriend(&mut self, user_id: &str) -> Result<bool> {
    let mut social = self
      .social
      .take()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    let res = social.unfriend(self, user_id).await;
    self.social = Some(social);
    res
  }

  pub async fn social_block(&mut self, user_id: &str) -> Result<bool> {
    let mut social = self
      .social
      .take()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    let res = social.block(self, user_id).await;
    self.social = Some(social);
    res
  }

  pub async fn social_unblock(&mut self, user_id: &str) -> Result<bool> {
    let mut social = self
      .social
      .take()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    let res = social.unblock(self, user_id).await;
    self.social = Some(social);
    res
  }

  pub async fn social_invite(&self, user_id: &str) -> Result<()> {
    let social = self
      .social
      .as_ref()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.invite(self, user_id).await
  }

  pub fn social_status(&self, status: Status, detail: Option<&str>) -> Result<()> {
    let social = self
      .social
      .as_ref()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.status(self, status, detail);
    Ok(())
  }

  pub fn social_mark_notifications_as_read(&mut self) -> Result<()> {
    let mut social = self
      .social
      .take()
      .ok_or_else(|| TriangleError::Adapter("social is not initialized".to_string()))?;
    social.mark_notifications_as_read(self);
    self.social = Some(social);
    Ok(())
  }

  pub fn snapshot(&self) -> Value {
    let spectating_strategy = match self.spectating_strategy {
      SpectatingStrategy::Smooth => "smooth",
      SpectatingStrategy::Instant => "instant",
    };

    serde_json::json!({
      "user": {
        "id": self.user.id,
        "username": self.user.username,
        "role": self.user.role,
        "session_id": self.user.session_id,
        "user_agent": self.user.user_agent,
      },
      "disconnected": self.disconnected,
      "handling": self.handling,
      "spectating_strategy": spectating_strategy,
      "social": self.social.as_ref().map(|s| s.snapshot()),
      "room": self.room.as_ref().map(|r| r.snapshot()),
      "has_game": self.game.is_some(),
    })
  }
}
