use serde::{Deserialize, Deserializer, Serialize};

fn null_as_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
  D: Deserializer<'de>,
  T: Default + Deserialize<'de>,
{
  Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
  #[default]
  Anon,
  Banned,
  User,
  Bot,
  Halfmod,
  Mod,
  Admin,
  Sysop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Badge {
  pub id: String,
  pub label: String,
  pub group: Option<String>,
  pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct League {
  pub apm: f64,
  pub decaying: bool,
  pub gamesplayed: u32,
  pub gameswon: u32,
  pub glicko: f64,
  pub gxe: f64,
  pub pps: f64,
  pub rank: String,
  pub rd: f64,
  pub standing: u32,
  pub standing_local: u32,
  pub tr: f64,
  pub vs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordDetail {
  pub record: serde_json::Value,
  pub rank: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Records {
  #[serde(rename = "40l")]
  pub sprint: Option<RecordDetail>,
  pub blitz: Option<RecordDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {}