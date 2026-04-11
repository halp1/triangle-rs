use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
  #[serde(rename = "type")]
  pub kind: String,
  #[serde(default)]
  pub data: Value,
}

pub type IncomingEvent = EventEnvelope;
pub type OutgoingEvent = EventEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAnnouncement {
  #[serde(rename = "type")]
  pub announcement_type: String,
  pub msg: String,
  pub ts: i64,
  pub reason: Option<String>,
}
