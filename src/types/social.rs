use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
  #[default]
  Online,
  Away,
  Busy,
  Offline,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmMessage {
  pub content: String,
  pub user: String,
  pub ts: String,
  pub system: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmUser {
  pub id: String,
  pub username: String,
  pub avatar: Option<u64>,
  pub status: Option<Status>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dm {
  pub id: String,
  pub user: DmUser,
  pub messages: Vec<DmMessage>,
  pub unread: bool,
  pub system: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
  Friend,
  Message,
  #[serde(other)]
  Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
  #[serde(rename = "_id")]
  pub id: String,
  #[serde(rename = "type")]
  pub notification_type: NotificationType,
  pub seen: bool,
  pub ts: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipType {
  Friend,
  Pending,
  Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipUser {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub avatar_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
  #[serde(rename = "_id")]
  pub id: String,
  pub from: RelationshipUser,
  pub to: RelationshipUser,
  #[serde(rename = "type")]
  pub relationship_type: RelationshipType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocked {
  pub id: String,
  pub username: String,
  pub avatar: Option<u64>,
}
