use crate::utils::{api::core::RequestSet, constants};

pub mod core;
pub mod server;
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

pub struct Api {
  pub config: Config,

  pub server: server::Server,
  pub users: users::Users,
}

impl Api {
  pub fn new(config: Config) -> Self {
    let cloned = config.clone();
    let mut s = Self {
      server: server::Server::new(),
      users: users::Users::new(),
      config,
    };

    s.update(cloned);

    s
  }

  pub fn update(&mut self, config: Config) {
    self.config = config;

    self.server.set_params(
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
