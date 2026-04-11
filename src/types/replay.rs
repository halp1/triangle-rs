use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
  #[serde(flatten)]
  pub value: Value,
}

pub type VersusReplay = Replay;
