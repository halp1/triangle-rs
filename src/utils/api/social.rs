use crate::{
  types::social::dm,
  utils::api::core::{ApiError, Request, Transport, get, post},
};

use super::core::RequestSet;

#[derive(Debug, Clone)]
pub struct Social {
  token: String,
  user_agent: String,
  transport: Transport,
}

impl Social {
  pub fn new() -> Self {
    Self {
      token: String::new(),
      user_agent: String::new(),
      transport: Transport::JSON,
    }
  }

  pub async fn unfriend(&self, user_id: &str) -> Result<(), ApiError> {
    post(
      Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: "relationships/remove".into(),
      },
      serde_json::json!({
        "user": user_id,
      }),
    )
    .await
  }

  pub async fn unblock(&self, user_id: &str) -> Result<(), ApiError> {
    self.unfriend(user_id).await
  }

  pub async fn friend(&self, user_id: &str) -> Result<(), ApiError> {
    post(
      Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: "relationships/friend".into(),
      },
      serde_json::json!({
        "user": user_id,
      }),
    )
    .await
  }

  pub async fn block(&self, user_id: &str) -> Result<(), ApiError> {
    post(
      Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: "relationships/block".into(),
      },
      serde_json::json!({
        "user": user_id,
      }),
    )
    .await
  }

  pub async fn dms(&self, user_id: &str) -> Result<Vec<dm::DM>, ApiError> {
    // get dms/{id}
    get(Request {
      token: self.token.clone(),
      user_agent: self.user_agent.clone(),
      transport: self.transport,
      uri: format!("dms/{}", user_id),
    })
    .await
  }
}

impl RequestSet for Social {
  fn set_params(&mut self, token: String, user_agent: String, _transport: Transport) {
    self.token = token;
    self.user_agent = user_agent;
    self.transport = Transport::JSON;
  }
}
