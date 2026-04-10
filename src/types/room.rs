use serde::{Deserialize, Serialize};

use super::user::Badge;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Type {
  #[default]
  Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
  Ingame,
  Lobby,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bracket {
  Player,
  Spectator,
  Observer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRecord {
  pub games: u32,
  pub wins: u32,
  pub streak: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub avatar_revision: Option<u64>,
  pub ready: bool,
  pub anon: bool,
  pub bot: bool,
  pub role: String,
  pub xp: f64,
  pub badges: Option<Vec<Badge>>,
  pub record: PlayerRecord,
  pub bracket: Bracket,
  pub supporter: bool,
  pub verified: Option<bool>,
  pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Autostart {
  pub enabled: bool,
  pub status: String,
  pub time: f64,
  pub maxtime: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
  pub gamemode: String,
  pub modename: String,
  pub ft: u32,
  pub wb: u32,
  pub gp: u32,
  pub record_replays: bool,
  pub stats: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetConfigItem {
  pub index: String,
  pub value: serde_json::Value,
}
