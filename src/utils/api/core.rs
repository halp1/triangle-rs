use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};

use crate::utils::pack;

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Transport {
  JSON,
  Binary,
}

impl Transport {
  pub fn mime(&self) -> &'static str {
    match self {
      Transport::JSON => "application/json",
      Transport::Binary => "application/vnd.osk.theorypack",
    }
  }

  pub fn encode(&self, data: impl Serialize) -> Vec<u8> {
    match self {
      Transport::JSON => serde_json::to_string(&data)
        .unwrap_or_default()
        .into_bytes(),
      Transport::Binary => pack::pack_typed(&data).unwrap_or_default(),
    }
  }

  pub fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<Result<T, String>, ApiError> {
    match self {
      Transport::JSON => {
        let s = String::from_utf8(data.to_vec())
          .map_err(|e| ApiError::Parse(format!("Failed to decode JSON: {e}")))?;
        let mut v: serde_json::Value = serde_json::from_str(&s)
          .map_err(|e| ApiError::Parse(format!("Failed to parse JSON: {e}\n  body: {s}")))?;

        match v["success"].as_bool() {
          Some(true) => {
            if let Some(obj) = v.as_object_mut() {
              obj.remove("success");
            }
            Ok(Ok(serde_json::from_value(v).map_err(|e| {
              ApiError::Parse(format!("Failed to deserialize JSON: {e}\n  body: {s}"))
            })?))
          }
          Some(false) => Ok(Err(
            v["error"]["msg"]
              .as_str()
              .unwrap_or("unknown error")
              .to_string(),
          )),
          None => Err(ApiError::Parse(format!(
            "Missing success field in JSON response\n  body: {s}"
          ))),
        }
      }
      Transport::Binary => {
        let unpacked = pack::unpack(data)
          .map_err(|e| ApiError::Parse(format!("Failed to unpack binary data: {e}")))?;
				println!("unpacked binary response: {unpacked}");
        match unpacked
          .as_map()
          .map(|m| {
            m.iter()
              .find(|(k, _)| k.as_str() == Some("success"))
              .and_then(|(_, v)| v.as_bool())
          })
          .flatten()
        {
          Some(true) => {
            let mut map = unpacked.as_map().unwrap_or_default().to_vec();
            map.retain(|(k, _)| k.as_str() != Some("success"));
            let value = msgpackr::Value::Map(map);
            Ok(Ok(msgpackr::serde::from_value(value.clone()).map_err(
              |e| {
                ApiError::Parse(format!(
                  "Failed to deserialize binary data: {e}\n  body: {value}"
                ))
              },
            )?))
          }
          Some(false) => {
            let error_msg = unpacked
              .as_map()
              .and_then(|m| {
                m.iter()
                  .find(|(k, _)| k.as_str() == Some("error"))
                  .map(|(_, v)| {
                    v.as_map().and_then(|m| {
                      m.iter()
                        .find(|(k, _)| k.as_str() == Some("msg"))
                        .and_then(|(_, v)| v.as_str())
                        .map(|s| s.to_string())
                    })
                  })
                  .flatten()
              })
              .unwrap_or("unknown error".into())
              .to_string();
            Ok(Err(error_msg))
          }
          _ => Err(ApiError::Parse(format!(
            "Missing success field in binary response\n  body: {unpacked}"
          ))),
        }
      }
    }
  }
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

	println!("decoding response from {}", req.uri);
  req
    .transport
    .decode::<T>(&raw_bytes)?
    .map_err(ApiError::Server)
}

pub async fn post<T: DeserializeOwned>(req: Request, body: impl Serialize) -> Result<T, ApiError> {
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

  req
    .transport
    .decode::<T>(&raw_bytes)?
    .map_err(ApiError::Server)
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
