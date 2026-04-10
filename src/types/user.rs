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
  #[serde(rename = "halfmod")]
  HalfMod,
  Mod,
  Admin,
  #[serde(rename = "sysop")]
  SysOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Badge {
  pub id: String,
  pub label: String,
  pub group: Option<String>,
  pub ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct League {
  #[serde(default, deserialize_with = "null_as_default")]
  pub apm: f64,
  #[serde(default)]
  pub decaying: bool,
  #[serde(default, deserialize_with = "null_as_default")]
  pub gamesplayed: u32,
  #[serde(default, deserialize_with = "null_as_default")]
  pub gameswon: u32,
  #[serde(default, deserialize_with = "null_as_default")]
  pub glicko: f64,
  #[serde(default, deserialize_with = "null_as_default")]
  pub gxe: f64,
  #[serde(default, deserialize_with = "null_as_default")]
  pub pps: f64,
  #[serde(default)]
  pub rank: crate::types::game::Rank,
  #[serde(default, deserialize_with = "null_as_default")]
  pub rd: f64,
  #[serde(default, deserialize_with = "null_as_default")]
  pub standing: i32,
  #[serde(default, deserialize_with = "null_as_default")]
  pub standing_local: i32,
  #[serde(default, deserialize_with = "null_as_default")]
  pub tr: f64,
  #[serde(default, deserialize_with = "null_as_default")]
  pub vs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Records {
  #[serde(rename = "40l")]
  pub sprint: Option<serde_json::Value>,
  pub blitz: Option<serde_json::Value>,
}

/// User profile as returned from `/api/users/me`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub role: Role,
  pub ts: Option<String>,
  pub badges: Option<Vec<Badge>>,
  pub xp: Option<f64>,
  pub gamesplayed: Option<i64>,
  pub gameswon: Option<i64>,
  pub gametime: Option<f64>,
  pub country: Option<String>,
  pub geo: Option<serde_json::Value>,
  pub badstanding: Option<bool>,
  pub supporter: Option<bool>,
  pub supporter_tier: Option<u32>,
  pub supporter_expires: Option<i64>,
  pub avatar_revision: Option<u64>,
  pub banner_revision: Option<u64>,
  pub bio: Option<String>,
  pub connections: Option<serde_json::Value>,
  pub friend_count: Option<u32>,
  pub distinguishment: Option<serde_json::Value>,
  pub achievements: Option<Vec<serde_json::Value>>,
  pub league: Option<League>,
  pub verified: Option<bool>,
  pub privacy_showwon: Option<bool>,
  pub privacy_showplayed: Option<bool>,
  pub privacy_showgametime: Option<bool>,
  pub privacy_showcountry: Option<bool>,
  pub privacy_mmchat: Option<bool>,
  pub privacy_privatemode: Option<String>,
  pub privacy_status_shallow: Option<String>,
  pub privacy_status_deep: Option<String>,
  pub privacy_status_exact: Option<String>,
  pub privacy_dm: Option<String>,
  pub privacy_invite: Option<String>,
  pub bannedstatus: Option<String>,
  pub records: Option<Records>,
  pub totp: Option<serde_json::Value>,
  pub email: Option<String>,
  pub total_supported: Option<u32>,
  pub thanked: Option<bool>,
  pub banlist: Option<Vec<serde_json::Value>>,
  pub warnings: Option<Vec<serde_json::Value>>,
}

/// Public user profile as returned from `/api/users/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub role: Role,
  pub ts: Option<String>,
  pub badges: Vec<Badge>,
  pub xp: f64,
  pub gamesplayed: i64,
  pub gameswon: i64,
  pub gametime: f64,
  pub country: Option<String>,
  pub badstanding: Option<bool>,
  pub records: Option<Records>,
  pub supporter: Option<bool>,
  pub supporter_tier: Option<u32>,
  pub verified: Option<bool>,
  pub league: Option<League>,
  pub avatar_revision: Option<u64>,
  pub banner_revision: Option<u64>,
  pub bio: Option<String>,
  pub friend_count: Option<u32>,
  pub friended_you: Option<bool>,
}
