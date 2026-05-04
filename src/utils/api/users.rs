use serde::{Deserialize, Serialize};

use crate::{
  types::user::{Me, User},
  utils::api::core::{ApiError, Request, Transport, get, post},
};

use super::core::RequestSet;

#[derive(Debug, Clone)]
pub struct Users {
  token: String,
  user_agent: String,
  transport: Transport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MeResponse {
  pub user: Me,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserResponse {
  pub user: User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExistsReponse {
  pub exists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ResolveResponse {
  #[serde(rename = "_id")]
  pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticateResponse {
  pub token: String,
  #[serde(rename = "userid")]
  pub id: String,
}

impl Users {
  pub fn new() -> Self {
    Self {
      token: String::new(),
      user_agent: String::new(),
      transport: Transport::JSON,
    }
  }

  pub async fn exists(&self, username: &str) -> Result<bool, ApiError> {
    Ok(
      get::<ExistsReponse>(Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: format!("users/{username}/exists"),
      })
      .await?
      .exists,
    )
  }

  pub async fn resolve(&self, username: &str) -> Result<String, ApiError> {
    Ok(
      get::<ResolveResponse>(Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: format!("users/{username}/resolve"),
      })
      .await?
      .id,
    )
  }

  pub async fn authenticate(
    &self,
    username: &str,
    password: &str,
  ) -> Result<AuthenticateResponse, ApiError> {
    post(
      Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: "users/authenticate".to_string(),
      },
      serde_json::json!({
        "username": username,
        "password": password,
        "totp": "",
      }),
    )
    .await
  }

  pub async fn me(&self) -> Result<Me, ApiError> {
    Ok(
      get::<MeResponse>(Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: "users/me".to_string(),
      })
      .await?
      .user,
    )
  }

  pub async fn get(&self, id: &str) -> Result<User, ApiError> {
    Ok(
      get::<UserResponse>(Request {
        token: self.token.clone(),
        user_agent: self.user_agent.clone(),
        transport: self.transport,
        uri: format!("users/{id}"),
      })
      .await?
      .user,
    )
  }
}

impl RequestSet for Users {
  fn set_params(&mut self, token: String, user_agent: String, transport: Transport) {
    self.token = token;
    self.user_agent = user_agent;
    self.transport = transport;
  }
}
