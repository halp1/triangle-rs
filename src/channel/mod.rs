use std::{
  collections::HashMap,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::error::{Result, TriangleError};
use rand::Rng;
use serde::de::DeserializeOwned;

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or(Duration::ZERO)
    .as_millis() as u64
}

/// Generates a random alphanumeric session ID.
pub fn random_session_id(length: usize) -> String {
  const CHARS: &[u8] = b"qwertyuiopasdfghjklzxcvbnmQWERTYUIOPASDFGHJKLZXCVBNM1234567890";
  let mut rng = rand::thread_rng();
  (0..length)
    .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
    .collect()
}

#[derive(Debug, Clone)]
pub struct Config {
  pub session_id: String,
  pub host: String,
  pub caching: bool,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      session_id: random_session_id(20),
      host: "https://ch.tetr.io/api/".to_string(),
      caching: true,
    }
  }
}

struct CacheEntry {
  until: u64,
  data: serde_json::Value,
}

/// HTTP client for the TETR.IO Channel API (`ch.tetr.io/api/`).
pub struct ChannelApi {
  config: Config,
  cache: HashMap<String, CacheEntry>,
  http: reqwest::Client,
}

impl ChannelApi {
  pub fn new() -> Self {
    Self {
      config: Config::default(),
      cache: HashMap::new(),
      http: reqwest::Client::new(),
    }
  }

  pub fn with_config(config: Config) -> Self {
    Self {
      config,
      cache: HashMap::new(),
      http: reqwest::Client::new(),
    }
  }

  pub fn config(&self) -> &Config {
    &self.config
  }

  pub fn set_config(&mut self, partial: PartialConfig) {
    if let Some(sid) = partial.session_id {
      self.config.session_id = sid;
    }
    if let Some(host) = partial.host {
      self.config.host = host;
    }
    if let Some(caching) = partial.caching {
      self.config.caching = caching;
    }
  }

  pub fn clear_cache(&mut self) {
    self.cache.clear();
  }

  /// Perform a GET request against `route`, substituting `:arg` placeholders
  /// with the values in `args`, and appending `query` as query-string
  /// parameters.
  pub async fn get<T: DeserializeOwned>(
    &mut self,
    route: &str,
    args: &[(&str, &str)],
    query: &[(&str, &str)],
    session_id: Option<&str>,
  ) -> Result<T> {
    let mut uri = route.to_string();

    for (key, val) in args {
      let placeholder = format!(":{}", key);
      if !uri.contains(&placeholder as &str) {
        return Err(TriangleError::Api(format!(
          "missing argument {} in route {}",
          key, route
        )));
      }
      uri = uri.replace(&placeholder as &str, val);
    }

    if !query.is_empty() {
      let qs: Vec<String> = query.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
      uri = format!("{}?{}", uri, qs.join("&"));
    }

    if self.config.caching {
      if let Some(entry) = self.cache.get(&uri) {
        if entry.until > now_ms() {
          return serde_json::from_value(entry.data.clone())
            .map_err(|e| TriangleError::Api(e.to_string()));
        } else {
          self.cache.remove(&uri);
        }
      }
    }

    let url = format!("{}{}", self.config.host, uri);
    let sid = session_id.unwrap_or(&self.config.session_id);

    let resp = self
      .http
      .get(&url)
      .header("X-Session-ID", sid)
      .send()
      .await
      .map_err(|e| TriangleError::Connection(e.to_string()))?;

    let raw: serde_json::Value = resp
      .json()
      .await
      .map_err(|e| TriangleError::Api(e.to_string()))?;

    if raw["success"].as_bool() == Some(false) {
      let msg = raw["error"]["msg"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
      return Err(TriangleError::Api(format!("[CH API] {}: {}", route, msg)));
    }

    if self.config.caching {
      let until = raw["cache"]["cached_until"].as_u64().unwrap_or(0);
      self.cache.insert(
        uri,
        CacheEntry {
          until,
          data: raw.clone(),
        },
      );
    }

    serde_json::from_value(raw).map_err(|e| TriangleError::Api(e.to_string()))
  }
}

impl Default for ChannelApi {
  fn default() -> Self {
    Self::new()
  }
}

/// Partial update for `Config`.
#[derive(Debug, Default)]
pub struct PartialConfig {
  pub session_id: Option<String>,
  pub host: Option<String>,
  pub caching: Option<bool>,
}
