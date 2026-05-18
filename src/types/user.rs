use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
  pub ts: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct League {
  pub apm: Option<f64>,
  pub decaying: bool,
  pub gamesplayed: u32,
  pub gameswon: u32,
  pub glicko: f64,
  pub gxe: f64,
  pub pps: Option<f64>,
  pub rank: String,
  pub rd: f64,
  pub standing: i32,
  pub standing_local: i32,
  pub tr: f64,
  pub vs: Option<f64>,
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
pub struct Totp {
  pub enabled: Option<bool>,
  pub codes_remaining: u64,
}

#[derive(Debug, Clone)]
pub struct Timestamp(pub SystemTime);

impl<'de> Deserialize<'de> for Timestamp {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
      type Value = Timestamp;

      fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a string or msgpack timestamp")
      }

      fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Timestamp, E> {
        let dt = DateTime::parse_from_rfc3339(s).map_err(serde::de::Error::custom)?;
        Ok(Timestamp(dt.into()))
      }

      fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Timestamp, A::Error> {
        let mut seconds: Option<i64> = None;
        while let Some(key) = map.next_key::<String>()? {
          if key == "seconds" {
            seconds = Some(map.next_value()?);
          } else {
            map.next_value::<serde::de::IgnoredAny>()?;
          }
        }
        let secs = seconds.ok_or_else(|| serde::de::Error::missing_field("seconds"))?;
        let st = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs.max(0) as u64);
        Ok(Timestamp(st))
      }
    }

    deserializer.deserialize_any(Visitor)
  }
}

impl Serialize for Timestamp {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    let dt: DateTime<Utc> = self.0.into();
    serializer.serialize_str(&dt.to_rfc3339())
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub country: Option<String>,
  pub email: Option<String>,
  pub role: Role,
  pub ts: Timestamp,
  pub badges: Vec<Badge>,
  pub xp: u64,
  pub privacy_showwon: bool,
  pub privacy_showplayed: bool,
  pub privacy_showgametime: bool,
  pub privacy_showcountry: bool,
  pub privacy_privatemode: String,
  pub privacy_status_shallow: String,
  pub privacy_status_deep: String,
  pub privacy_status_exact: String,
  pub privacy_dm: String,
  pub privacy_invite: String,
  pub thanked: bool,
  pub banlist: Vec<serde_json::Value>,
  pub warnings: Vec<serde_json::Value>,
  pub bannedstatus: String,
  pub records: Option<Records>,
  pub supporter: bool,
  pub supporter_expires: u64,
  pub total_supported: u64,
  pub league: League,
  pub avatar_revision: Option<u64>,
  pub banner_revision: Option<u64>,
  pub bio: Option<String>,
  pub zen: Option<serde_json::Value>,
  pub distinguishment: Option<serde_json::Value>,
  pub totp: Totp,
  pub connections: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub role: Role,
  pub ts: Timestamp,
  pub badges: Vec<Badge>,
  pub xp: u64,
  pub gamesplayed: u64,
  pub gameswon: u64,
  pub gametime: u64,
  pub country: Option<String>,
  pub badstanding: bool,
  pub records: Option<Records>,
  pub supporter: bool,
  pub supporter_tier: u64,
  pub verified: bool,
  pub league: League,
  pub avatar_revision: Option<u64>,
  pub banner_revision: Option<u64>,
  pub bio: Option<String>,
  pub friend_count: u64,
  pub friended_you: bool,
}
