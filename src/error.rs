use thiserror::Error;

#[derive(Debug, Error)]
pub enum TriangleError {
  #[error("HTTP error: {0}")]
  Http(#[from] reqwest::Error),

  #[error("WebSocket error: {0}")]
  WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),

  #[error("Msgpack encode error: {0}")]
  MsgpackEncode(#[from] rmp_serde::encode::Error),

  #[error("Msgpack decode error: {0}")]
  MsgpackDecode(#[from] rmp_serde::decode::Error),

  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),

  #[error("URL parse error: {0}")]
  Url(#[from] url::ParseError),

  #[error("Base64 decode error: {0}")]
  Base64(#[from] base64::DecodeError),

  #[error("Invalid token")]
  InvalidToken,

  #[error("API error: {0}")]
  Api(String),

  #[error("Connection error: {0}")]
  Connection(String),

  #[error("Engine error: {0}")]
  Engine(String),

  #[error("Adapter error: {0}")]
  Adapter(String),

  #[error("Channel error: {0}")]
  Channel(String),

  #[error("Ribbon error: {0}")]
  Ribbon(String),

  #[error("Invalid argument: {0}")]
  InvalidArgument(String),
}

pub type Result<T> = std::result::Result<T, TriangleError>;
