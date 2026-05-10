use crate::utils::api::core::{ApiError, Request, Transport, get};

use super::core::RequestSet;


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Room {
	pub id: String,
	pub name: String,
	pub name_safe: String,
	pub r#type: String,
	pub user_limit: u32,
	pub user_rank_limit: String,
	pub state: String,
	pub allow_anonymous: bool,
	pub allow_unranked: bool,
	pub players: u32,
	pub count: u32,
}

#[derive(Debug, Clone)]
pub struct Rooms {
  token: String,
  user_agent: String,
  transport: Transport,
}


impl Rooms {
  pub fn new() -> Self {
    Self {
      token: String::new(),
      user_agent: String::new(),
      transport: Transport::JSON,
    }
  }

	pub async fn list(&self) -> Result<Vec<Room>, ApiError> {
		get(Request {
			token: self.token.clone(),
			user_agent: self.user_agent.clone(),
			transport: self.transport,
			uri: "rooms/".to_string(),
		})
		.await
	}
}

impl RequestSet for Rooms {
  fn set_params(&mut self, token: String, user_agent: String, _transport: Transport) {
    self.token = token;
    self.user_agent = user_agent;
    self.transport = Transport::JSON;
  }
}
