use futures_util::stream::{FuturesUnordered, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
  TriangleError,
  types::server::Environment,
  utils::api::core::{Request, Transport, get},
};

use super::core::RequestSet;

#[derive(Debug, Clone)]
pub struct Server {
  token: String,
  user_agent: String,
  transport: Transport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpoolEntry {
  pub name: String,
  pub host: String,
  pub flag: String,
  pub location: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpoolsWrapper {
  pub token: String,
  pub spools: Vec<SpoolEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RibbonResponse {
  pub endpoint: String,
  pub spools: Option<SpoolsWrapper>,
}

#[derive(Clone, Debug)]
pub struct SpoolResult {
  pub host: String,
  pub endpoint: String,
  pub token: String,
}

struct SpoolFlags {
  avoid_due_to_high_load: bool,
  recently_restarted: bool,
}

struct SpoolData {
  flags: SpoolFlags,
}

fn parse_spool_data(binary: &[u8]) -> crate::Result<SpoolData> {
  if binary.len() < 6 {
    return Err(TriangleError::Api("spool data too short".to_string()));
  }
  Ok(SpoolData {
    flags: SpoolFlags {
      avoid_due_to_high_load: binary[1] & 0b01000000 != 0,
      recently_restarted: binary[1] & 0b00100000 != 0,
    },
  })
}

async fn get_despool(endpoint: &str, index: usize, user_agent: &str) -> crate::Result<SpoolData> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis();
  let random = rand::random::<u32>() % 1_000_000;
  let url = format!("https://{}/spool?{}-{}-{}", endpoint, now, index, random);

  let client = Client::new();
  let res = client
    .get(&url)
    .header("User-Agent", user_agent)
    .send()
    .await
    .map_err(|e| TriangleError::Api(format!("spool fetch failed: {}", e)))?;

  let bytes = res
    .bytes()
    .await
    .map_err(|e| TriangleError::Api(format!("spool read failed: {}", e)))?;

  parse_spool_data(&bytes)
}

impl Server {
  pub fn new() -> Self {
    Self {
      token: String::new(),
      user_agent: String::new(),
      transport: Transport::JSON,
    }
  }

  async fn find_fastest_available_spool(&self, spools: Vec<SpoolEntry>) -> SpoolEntry {
    let user_agent = self.user_agent.clone();
    let mut futures: FuturesUnordered<_> = spools
      .into_iter()
      .enumerate()
      .map(|(index, spool)| {
        let ua = user_agent.clone();
        Box::pin(async move {
          let data = get_despool(&spool.host, index, &ua).await?;
          if data.flags.avoid_due_to_high_load || data.flags.recently_restarted {
            return Err(TriangleError::Api("spool is unstable".to_string()));
          }
          Ok::<SpoolEntry, TriangleError>(spool)
        })
      })
      .collect();

    while let Some(result) = futures.next().await {
      if let Ok(spool) = result {
        return spool;
      }
    }

    SpoolEntry {
      name: "tetr.io".to_string(),
      host: "tetr.io".to_string(),
      flag: "NL".to_string(),
      location: "osk".to_string(),
    }
  }

  pub async fn environment(&self) -> crate::Result<Environment> {
    get(Request {
      token: self.token.clone(),
      user_agent: self.user_agent.clone(),
      transport: Transport::JSON,
      uri: "server/environment".to_string(),
    })
    .await
  }

  pub async fn spool(&self, use_spools: bool) -> crate::Result<SpoolResult> {
    let res = get::<RibbonResponse>(Request {
      token: self.token.clone(),
      user_agent: self.user_agent.clone(),
      transport: Transport::JSON,
      uri: "server/ribbon".to_string(),
    })
    .await?;

    let endpoint = res.endpoint.replace("/ribbon/", "");

    if !use_spools || res.spools.is_none() {
      return Ok(SpoolResult {
        host: "tetr.io".to_string(),
        endpoint,
        token: String::new(),
      });
    }

    let spools_wrapper = res.spools.unwrap();
    let fastest = self
      .find_fastest_available_spool(spools_wrapper.spools)
      .await;

    Ok(SpoolResult {
      host: fastest.host,
      endpoint,
      token: spools_wrapper.token,
    })
  }
}

impl RequestSet for Server {
  fn set_params(&mut self, token: String, user_agent: String, _transport: Transport) {
    self.token = token;
    self.user_agent = user_agent;
    self.transport = Transport::JSON;
  }
}
