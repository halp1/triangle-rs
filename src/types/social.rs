use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
  pub suppress_dm_errors: bool,
  pub auto_load_dms: bool,
  pub auto_process_notifications: bool,
}

impl Config {
  pub fn default_config() -> Self {
    Self {
      suppress_dm_errors: false,
      auto_load_dms: true,
      auto_process_notifications: true,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
  #[default]
  Online,
  Away,
  Busy,
  Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
  Empty,
  Menus,
  FortyL,
  Blitz,
  Zen,
  Custom,
  LobbyEndXQP,
  LobbySpecXQP,
  LobbyIgXQP,
  LobbyXQP,
  LobbyEndXPriv,
  LobbySpecXPriv,
  LobbyIgXPriv,
  LobbyXPriv,
  TlMm,
  Tl,
  TlEnd,
  TlMmComplete,
  Other(String),
}

impl<'de> Deserialize<'de> for Detail {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let s = String::deserialize(deserializer)?;

    Ok(match s.as_str() {
      "" => Detail::Empty,

      "menus" => Detail::Menus,
      "40l" => Detail::FortyL,
      "blitz" => Detail::Blitz,
      "zen" => Detail::Zen,
      "custom" => Detail::Custom,

      "lobby_end:X-QP" => Detail::LobbyEndXQP,
      "lobby_spec:X-QP" => Detail::LobbySpecXQP,
      "lobby_ig:X-QP" => Detail::LobbyIgXQP,
      "lobby:X-QP" => Detail::LobbyXQP,

      "lobby_end:X-PRIV" => Detail::LobbyEndXPriv,
      "lobby_spec:X-PRIV" => Detail::LobbySpecXPriv,
      "lobby_ig:X-PRIV" => Detail::LobbyIgXPriv,
      "lobby:X-PRIV" => Detail::LobbyXPriv,

      "tl_mm" => Detail::TlMm,
      "tl" => Detail::Tl,
      "tl_end" => Detail::TlEnd,
      "tl_mm_complete" => Detail::TlMmComplete,

      other => Detail::Other(other.to_string()),
    })
  }
}

impl Serialize for Detail {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    let s = match self {
      Detail::Empty => "",

      Detail::Menus => "menus",
      Detail::FortyL => "40l",
      Detail::Blitz => "blitz",
      Detail::Zen => "zen",
      Detail::Custom => "custom",

      Detail::LobbyEndXQP => "lobby_end:X-QP",
      Detail::LobbySpecXQP => "lobby_spec:X-QP",
      Detail::LobbyIgXQP => "lobby_ig:X-QP",
      Detail::LobbyXQP => "lobby:X-QP",

      Detail::LobbyEndXPriv => "lobby_end:X-PRIV",
      Detail::LobbySpecXPriv => "lobby_spec:X-PRIV",
      Detail::LobbyIgXPriv => "lobby_ig:X-PRIV",
      Detail::LobbyXPriv => "lobby:X-PRIV",

      Detail::TlMm => "tl_mm",
      Detail::Tl => "tl",
      Detail::TlEnd => "tl_end",
      Detail::TlMmComplete => "tl_mm_complete",

      Detail::Other(s) => s.as_str(),
    };

    serializer.serialize_str(s)
  }
}

pub mod dm {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct DM {
    data: Data,
    stream: String,
    ts: String,
    id: String,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Data {
    pub content: String,
    pub content_safe: String,
    pub user: String,
    pub userdata: UserData,
    pub system: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct UserData {
    pub role: String,
    pub supporter: bool,
    pub supporter_tier: u32,
    pub verified: Option<bool>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum FailReason {
    #[serde(rename = "they.fail")]
    TheyFail,
    #[serde(rename = "you.fail")]
    YouFail,
    #[serde(rename = "they.ban")]
    TheyBan,
    #[serde(rename = "you.ban")]
    YouBan,
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
  #[serde(rename = "_id")]
  pub id: String,
  pub data: serde_json::Value,
  pub seen: bool,
  pub stream: String,
  pub ts: String,
  #[serde(rename = "type")]
  pub notification_type: String,
}

pub mod relationship {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum Type {
    Friend,
    Block,
    Pending,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct User {
    #[serde(rename = "_id")]
    pub id: String,
    pub username: String,
    pub avatar_revision: Option<u64>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Relationship {
    #[serde(rename = "_id")]
    pub id: String,
    pub from: User,
    pub to: User,
    #[serde(rename = "type")]
    pub relationship_type: Type,
    pub unread: u32,
    pub updated: String,
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocked {
  pub id: String,
  pub username: String,
  pub avatar: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
  pub status: Status,
  pub detail: Detail,
  pub invitable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
  pub total_online: u32,
  pub notifications: Vec<Notification>,
  pub presences: HashMap<String, Presence>,
  pub relationships: Vec<relationship::Relationship>,
}
