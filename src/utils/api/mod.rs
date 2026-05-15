use serde::{Serialize, de::DeserializeOwned};

use crate::utils::{
  api::core::{ApiError, Request, RequestSet},
  constants,
};

pub mod core;
pub mod rooms;
pub mod server;
pub mod social;
pub mod users;

pub use core::Transport;

#[derive(Debug, Clone)]
pub struct Config {
  pub token: String,
  pub user_agent: String,
  pub transport: Transport,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      token: String::new(),
      user_agent: constants::USER_AGENT.to_string(),
      transport: Transport::JSON,
    }
  }
}

#[derive(Debug)]
pub struct Api {
  pub config: Config,

  pub rooms: rooms::Rooms,
  pub server: server::Server,
  pub social: social::Social,
  pub users: users::Users,
}

impl Api {
  pub fn new(config: Config) -> Self {
    let cloned = config.clone();
    let mut s = Self {
      rooms: rooms::Rooms::new(),
      server: server::Server::new(),
      social: social::Social::new(),
      users: users::Users::new(),
      config,
    };

    s.update(cloned);

    s
  }

  pub async fn get<T: DeserializeOwned>(&self, uri: &str) -> Result<T, ApiError> {
    core::get(Request {
      token: self.config.token.clone(),
      user_agent: self.config.user_agent.clone(),
      transport: self.config.transport,
      uri: uri.to_string(),
    })
    .await
  }

  pub async fn post<T: DeserializeOwned>(
    &self,
    uri: &str,
    body: impl Serialize,
  ) -> Result<T, ApiError> {
    core::post(
      Request {
        token: self.config.token.clone(),
        user_agent: self.config.user_agent.clone(),
        transport: self.config.transport,
        uri: uri.to_string(),
      },
      body,
    )
    .await
  }

  pub fn update(&mut self, config: Config) {
    self.config = config;

    self.rooms.set_params(
      self.config.token.clone(),
      self.config.user_agent.clone(),
      self.config.transport,
    );
    self.server.set_params(
      self.config.token.clone(),
      self.config.user_agent.clone(),
      self.config.transport,
    );
    self.social.set_params(
      self.config.token.clone(),
      self.config.user_agent.clone(),
      self.config.transport,
    );
    self.users.set_params(
      self.config.token.clone(),
      self.config.user_agent.clone(),
      self.config.transport,
    );
  }
}
