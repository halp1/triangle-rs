use crate::error::{Result, TriangleError};
use crate::types::user::Me;
use reqwest::Client as HttpClient;
use serde::de::DeserializeOwned;
use serde_json::json;

const BASE_URL: &str = "https://tetr.io/api";

/// Parse a TETR.IO API response body: check `success`, then deserialize `T`.
fn parse_response<T: DeserializeOwned>(body: &str, uri: &str) -> Result<T> {
  let v: serde_json::Value = serde_json::from_str(body).map_err(|e| {
    TriangleError::Api(format!(
      "invalid JSON from {uri}: {e}\n  body: {}",
      &body[..body.len().min(512)]
    ))
  })?;

  if v["success"].as_bool() == Some(false) {
    let msg = v["error"]["msg"]
      .as_str()
      .unwrap_or("unknown error")
      .to_string();
    return Err(TriangleError::Api(format!("{uri}: {msg}")));
  }

  serde_json::from_value(v).map_err(|e| {
    TriangleError::Api(format!(
      "deserialize error on {uri}: {e}\n  body: {}",
      &body[..body.len().min(512)]
    ))
  })
}

/// Defaults used for every request.
#[derive(Clone)]
pub struct ApiDefaults {
  pub token: String,
  pub user_agent: String,
  pub turnstile: Option<String>,
}

/// The main TETR.IO HTTP API client.
#[derive(Clone)]
pub struct Api {
  http: HttpClient,
  pub defaults: ApiDefaults,
}

impl Api {
  pub fn new(token: impl Into<String>, user_agent: impl Into<String>) -> Self {
    let http = HttpClient::builder()
      .build()
      .expect("failed to build HTTP client");
    Self {
      http,
      defaults: ApiDefaults {
        token: token.into(),
        user_agent: user_agent.into(),
        turnstile: None,
      },
    }
  }

  pub fn with_turnstile(mut self, token: impl Into<String>) -> Self {
    self.defaults.turnstile = Some(token.into());
    self
  }

  async fn get_json<T: DeserializeOwned>(&self, uri: &str, use_auth: bool) -> Result<T> {
    let url = format!("{}/{}", BASE_URL, uri);
    let mut req = self
      .http
      .get(&url)
      .header("Accept", "application/json")
      .header("User-Agent", &self.defaults.user_agent);

    if use_auth && !self.defaults.token.is_empty() {
      req = req.header("Authorization", format!("Bearer {}", self.defaults.token));
    }

    if let Some(ref cf) = self.defaults.turnstile {
      req = req.header("Cookie", format!("cf_clearance={}", cf));
    }

    let resp = req.send().await?;
    let status = resp.status();

    if status.as_u16() == 429 {
      return Err(TriangleError::Api(format!(
        "rate limited on GET to {}",
        uri
      )));
    }

    let body = resp
      .text()
      .await
      .map_err(|e| TriangleError::Connection(e.to_string()))?;
    parse_response(&body, uri)
  }

  async fn post_json<T: DeserializeOwned>(&self, uri: &str, body: serde_json::Value) -> Result<T> {
    let url = format!("{}/{}", BASE_URL, uri);
    let mut req = self
      .http
      .post(&url)
      .header("Accept", "application/json")
      .header("Content-Type", "application/json")
      .header("User-Agent", &self.defaults.user_agent)
      .json(&body);

    if !self.defaults.token.is_empty() {
      req = req.header("Authorization", format!("Bearer {}", self.defaults.token));
    }

    if let Some(ref cf) = self.defaults.turnstile {
      req = req.header("Cookie", format!("cf_clearance={}", cf));
    }

    let resp = req.send().await?;
    let status = resp.status();

    if status.as_u16() == 429 {
      return Err(TriangleError::Api(format!(
        "rate limited on POST to {}",
        uri
      )));
    }

    let body_text = resp
      .text()
      .await
      .map_err(|e| TriangleError::Connection(e.to_string()))?;
    parse_response(&body_text, uri)
  }

  // ── Users ─────────────────────────────────────────────────────────────

  /// Returns the authenticated user's profile.
  pub async fn me(&self) -> Result<Me> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      user: Me,
    }
    let result: Wrap = self.get_json("users/me", true).await?;
    Ok(result.user)
  }

  /// Returns a user's public profile by ID or username.
  pub async fn get_user(&self, id_or_name: &str) -> Result<crate::types::user::User> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      user: crate::types::user::User,
    }
    let result: Wrap = self
      .get_json(&format!("users/{}", id_or_name), false)
      .await?;
    Ok(result.user)
  }

  /// Resolves a username to a user ID.
  pub async fn resolve_user(&self, username: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      #[serde(rename = "_id")]
      id: String,
    }
    let result: Wrap = self
      .get_json(
        &format!("users/{}/resolve", urlencoding::encode(username.trim())),
        false,
      )
      .await?;
    Ok(result.id)
  }

  /// Checks whether a user exists.
  pub async fn user_exists(&self, username: &str) -> Result<bool> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      exists: bool,
    }
    let result: Wrap = self
      .get_json(&format!("users/{}/exists", username), false)
      .await?;
    Ok(result.exists)
  }

  /// Authenticates with username + password and returns a JWT token.
  pub async fn authenticate(&self, username: &str, password: &str) -> Result<AuthResult> {
    self
      .post_json(
        "users/authenticate",
        json!({ "username": username, "password": password }),
      )
      .await
  }

  // ── Server ────────────────────────────────────────────────────────────

  /// Returns the current server environment (includes the signature used during ribbon auth).
  pub async fn environment(&self) -> Result<Environment> {
    self.get_json("server/environment", false).await
  }

  /// Returns the ribbon spool endpoint.
  pub async fn spool(&self) -> Result<SpoolResult> {
    #[derive(serde::Deserialize)]
    struct SpoolServer {
      host: String,
    }
    #[derive(serde::Deserialize)]
    struct SpoolsPayload {
      token: String,
      spools: Vec<SpoolServer>,
    }
    #[derive(serde::Deserialize)]
    struct Wrap {
      endpoint: String,
      spools: Option<SpoolsPayload>,
    }
    let wrap: Wrap = self.get_json("server/ribbon", true).await?;
    let endpoint = wrap.endpoint.replace("/ribbon/", "");
    match wrap.spools {
      Some(sp) if !sp.spools.is_empty() => Ok(SpoolResult {
        host: sp.spools[0].host.clone(),
        endpoint,
        token: sp.token,
      }),
      _ => Ok(SpoolResult {
        host: "tetr.io".to_string(),
        endpoint,
        token: String::new(),
      }),
    }
  }

  // ── Rooms ─────────────────────────────────────────────────────────────

  /// Lists public rooms.
  pub async fn list_rooms(&self) -> Result<Vec<serde_json::Value>> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      rooms: Vec<serde_json::Value>,
    }
    let result: Wrap = self.get_json("rooms/", true).await?;
    Ok(result.rooms)
  }

  // ── Relationships ─────────────────────────────────────────────────────

  /// Blocks a user.
  pub async fn block_user(&self, id: &str) -> Result<bool> {
    let v: serde_json::Value = self
      .post_json("relationships/block", json!({ "user": id }))
      .await?;
    Ok(v["success"].as_bool().unwrap_or(false))
  }

  /// Unblocks / removes a relationship with a user.
  pub async fn remove_relationship(&self, id: &str) -> Result<bool> {
    let _: serde_json::Value = self
      .post_json("relationships/remove", json!({ "user": id }))
      .await?;
    Ok(true)
  }

  /// Friends a user.
  pub async fn friend_user(&self, id: &str) -> Result<bool> {
    let v: serde_json::Value = self
      .post_json("relationships/friend", json!({ "user": id }))
      .await?;
    Ok(v["success"].as_bool().unwrap_or(false))
  }

  /// Fetches DMs with a user.
  pub async fn dms(&self, id: &str) -> Result<Vec<crate::types::social::Dm>> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      dms: Vec<crate::types::social::Dm>,
    }
    let result: Wrap = self.get_json(&format!("dms/{}", id), true).await?;
    Ok(result.dms)
  }

  // ── Channel (game replays) ────────────────────────────────────────────

  /// Fetches a game replay by ID.
  pub async fn replay(&self, id: &str) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      game: serde_json::Value,
    }
    let result: Wrap = self.get_json(&format!("games/{}", id), true).await?;
    Ok(result.game)
  }
}

// ── Supporting types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthResult {
  pub token: String,
  pub userid: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Environment {
  pub stats: EnvironmentStats,
  pub signature: serde_json::Value,
  pub vx: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct EnvironmentStats {
  pub players: Option<u64>,
  pub users: Option<u64>,
  pub gamesplayed: Option<u64>,
  pub gametime: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SpoolResult {
  pub host: String,
  pub endpoint: String,
  pub token: String,
}
