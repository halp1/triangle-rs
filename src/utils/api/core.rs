use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Transport {
  JSON,
}

impl Transport {
  pub fn mime(&self) -> &'static str {
    match self {
      Transport::JSON => "application/json",
    }
  }

  pub fn encode(&self, data: serde_json::Value) -> Vec<u8> {
    match self {
      Transport::JSON => data.to_string().into_bytes(),
    }
  }

  pub fn decode(&self, data: &[u8]) -> Result<serde_json::Value, ApiError> {
    let s = String::from_utf8_lossy(data);
    serde_json::from_str(&s).map_err(|e| {
      ApiError::Parse(format!(
        "failed to decode transport data: {}\nRaw data: {:?}\nError: {}",
        s, data, e
      ))
    })
  }
}

fn parse<T: serde::de::DeserializeOwned>(
  data: serde_json::Value,
  uri: &str,
) -> Result<T, ApiError> {
  if data["success"].as_bool() == Some(false) {
    let msg = data["error"]["msg"]
      .as_str()
      .unwrap_or("unknown error")
      .to_string();
    return Err(ApiError::Server(msg));
  }

  let mut data = data;

  // remove success field
  if let Some(obj) = data.as_object_mut() {
    obj.remove("success");
  }

  serde_json::from_value(data.clone()).map_err(|e| {
    ApiError::Parse(format!(
      "deserialize error on {uri}: {e}\n  body: {}",
      serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string())
    ))
  })
}

pub struct Request {
  pub token: String,
  pub user_agent: String,
  pub transport: Transport,
  pub uri: String,
}

#[derive(Debug)]
pub enum ApiError {
  Request(reqwest::Error),
  Parse(String),
  Server(String),
  Alternate(String),
}

const BASE_URL: &str = "https://tetr.io/api";

pub async fn get<T: DeserializeOwned>(req: Request) -> Result<T, ApiError> {
  let client = Client::new();

  let res = client
    .get(&format!("{}/{}", BASE_URL, req.uri))
    .header("Accept", req.transport.mime())
    .header("User-Agent", &req.user_agent)
    .header("Authorization", format!("Bearer {}", req.token))
    .send()
    .await
    .map_err(|e| ApiError::Request(e))?;

  let raw_bytes = res.bytes().await.map_err(|e| ApiError::Request(e))?;

  let data = req.transport.decode(&raw_bytes)?;
  parse::<T>(data, &req.uri)
}

pub async fn post<T: DeserializeOwned, K: Serialize>(req: Request, body: K) -> Result<T, ApiError> {
  let client = Client::new();

  let res = client
    .post(&format!("{}/{}", BASE_URL, req.uri))
    .header("Accept", req.transport.mime())
    .header("Content-Type", req.transport.mime())
    .header("User-Agent", &req.user_agent)
    .header("Authorization", format!("Bearer {}", req.token))
    .body(
      req.transport.encode(
        serde_json::to_value(body)
          .map_err(|e| ApiError::Parse(format!("Failed to serialize request body: {}", e)))?,
      ),
    )
    .send()
    .await
    .map_err(|e| ApiError::Request(e))?;

  let raw_bytes = res.bytes().await.map_err(|e| ApiError::Request(e))?;

  let data = req.transport.decode(&raw_bytes)?;
  parse::<T>(data, &req.uri)
}

#[derive(Debug, Clone)]
pub struct ApiDefaults {
  pub token: String,
  pub user_agent: String,
  pub turnstile: Option<String>,
}

pub trait RequestSet {
  fn set_params(&mut self, token: String, user_agent: String, transport: Transport);
}
