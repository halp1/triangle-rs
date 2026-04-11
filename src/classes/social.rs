use std::time::Duration;

use serde_json::{Value, json};

use crate::{
  error::{Result, TriangleError},
  types::social::{
    Blocked, Config as SocialConfig, Dm, Notification, Relationship, RelationshipType, Status,
  },
};

use super::client::Client;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationshipEntry {
  pub id: String,
  pub relationship_id: String,
  pub username: String,
  pub avatar: Option<u64>,
}

pub struct Social {
  pub online: u64,
  pub friends: Vec<RelationshipEntry>,
  pub other: Vec<RelationshipEntry>,
  pub blocked: Vec<Blocked>,
  pub notifications: Vec<Notification>,
  pub config: SocialConfig,
}

#[derive(Debug, Clone)]
pub enum RelationshipLookup<'a> {
  Id(&'a str),
  Username(&'a str),
  Any(&'a str),
}

impl Social {
  pub fn new(user: &super::client::ClientUser, config: SocialConfig, init_data: &Value) -> Self {
    let social = &init_data["social"];
    let self_id = &user.id;

    let total_online = social["total_online"].as_u64().unwrap_or(0);
    let notifications: Vec<Notification> = social["notifications"]
      .as_array()
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| serde_json::from_value(v.clone()).ok())
          .collect()
      })
      .unwrap_or_default();

    let relationships: Vec<Relationship> = social["relationships"]
      .as_array()
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| serde_json::from_value(v.clone()).ok())
          .collect()
      })
      .unwrap_or_default();

    let process = |r: &Relationship| -> RelationshipEntry {
      let (id, username, avatar) = if r.from.id == *self_id {
        (r.to.id.clone(), r.to.username.clone(), r.to.avatar_revision)
      } else {
        (
          r.from.id.clone(),
          r.from.username.clone(),
          r.from.avatar_revision,
        )
      };
      RelationshipEntry {
        id,
        relationship_id: r.id.clone(),
        username,
        avatar,
      }
    };

    let friends: Vec<RelationshipEntry> = relationships
      .iter()
      .filter(|r| r.relationship_type == RelationshipType::Friend)
      .map(process)
      .collect();

    let other: Vec<RelationshipEntry> = relationships
      .iter()
      .filter(|r| r.relationship_type == RelationshipType::Pending)
      .map(process)
      .collect();

    let blocked: Vec<Blocked> = relationships
      .iter()
      .filter(|r| r.relationship_type == RelationshipType::Block)
      .map(|r| {
        let entry = process(r);
        Blocked {
          id: entry.id,
          username: entry.username,
          avatar: entry.avatar,
        }
      })
      .collect();

    Self {
      online: total_online,
      friends,
      other,
      blocked,
      notifications,
      config,
    }
  }

  pub fn mark_notifications_as_read(&mut self, client: &Client) {
    client.emit("social.notification.ack", Value::Null);
    for notification in &mut self.notifications {
      notification.seen = true;
    }
  }

  pub fn get(&self, target: &str) -> Option<&RelationshipEntry> {
    self.get_with(RelationshipLookup::Any(target))
  }

  pub fn get_with(&self, lookup: RelationshipLookup<'_>) -> Option<&RelationshipEntry> {
    let matches = |r: &RelationshipEntry| match lookup {
      RelationshipLookup::Id(id) => r.id == id,
      RelationshipLookup::Username(username) => r.username == username,
      RelationshipLookup::Any(value) => r.id == value || r.username == value,
    };

    self
      .friends
      .iter()
      .find(|r| matches(r))
      .or_else(|| self.other.iter().find(|r| matches(r)))
  }

  pub fn snapshot(&self) -> Value {
    json!({
      "online": self.online,
      "friends": self.friends,
      "others": self.other,
      "blocked": self.blocked,
      "notifications": self.notifications,
      "config": self.config,
    })
  }

  pub async fn resolve(&self, client: &Client, username: &str) -> Result<String> {
    client.api.resolve_user(username).await
  }

  pub async fn dms_with_user(&self, client: &Client, user_id: &str) -> Result<Vec<Dm>> {
    client.api.dms(user_id).await
  }

  pub async fn dms_with(&self, client: &Client, lookup: RelationshipLookup<'_>) -> Result<Vec<Dm>> {
    let target = self
      .get_with(lookup)
      .ok_or_else(|| TriangleError::InvalidArgument("relationship target not found".to_string()))?;
    self.dms_with_user(client, &target.id).await
  }

  pub async fn who(&self, client: &Client, id_or_name: &str) -> Result<crate::types::user::User> {
    client.api.get_user(id_or_name).await
  }

  pub async fn dm(&self, client: &Client, user_id: &str, message: &str) -> Result<Value> {
    if user_id == client.user.id {
      return Err(TriangleError::InvalidArgument(
        "you can't DM yourself".to_string(),
      ));
    }

    match client
      .wrap(
        "social.dm",
        json!({ "recipient": user_id, "msg": message }),
        "social.dm",
      )
      .await
    {
      Ok(v) => Ok(v),
      Err(e) if self.config.suppress_dm_errors => Ok(json!({ "error": format!("{e}") })),
      Err(e) => Err(e),
    }
  }

  pub async fn friend(&mut self, client: &Client, user_id: &str) -> Result<bool> {
    if self.friends.iter().any(|r| r.id == user_id) {
      return Ok(false);
    }

    let ok = client.api.friend_user(user_id).await?;
    if !ok {
      return Ok(false);
    }

    let user = client.api.get_user(user_id).await?;
    self.friends.push(RelationshipEntry {
      id: user.id,
      relationship_id: String::new(),
      username: user.username,
      avatar: user.avatar_revision,
    });
    self.other.retain(|r| r.id != user_id);
    Ok(true)
  }

  pub async fn unfriend(&mut self, client: &Client, user_id: &str) -> Result<bool> {
    if !self.friends.iter().any(|r| r.id == user_id) {
      return Ok(false);
    }

    client.api.remove_relationship(user_id).await?;
    self.friends.retain(|r| r.id != user_id);
    Ok(true)
  }

  pub async fn block(&mut self, client: &Client, user_id: &str) -> Result<bool> {
    if self.blocked.iter().any(|r| r.id == user_id) {
      return Ok(false);
    }

    let ok = client.api.block_user(user_id).await?;
    if !ok {
      return Ok(false);
    }

    let user = client.api.get_user(user_id).await?;
    self.blocked.push(Blocked {
      id: user.id.clone(),
      username: user.username,
      avatar: user.avatar_revision,
    });

    self.friends.retain(|r| r.id != user_id);
    self.other.retain(|r| r.id != user_id);
    Ok(true)
  }

  pub async fn unblock(&mut self, client: &Client, user_id: &str) -> Result<bool> {
    if !self.blocked.iter().any(|r| r.id == user_id) {
      return Ok(false);
    }

    client.api.remove_relationship(user_id).await?;
    self.blocked.retain(|r| r.id != user_id);
    Ok(true)
  }

  pub async fn invite(&self, client: &Client, user_id: &str) -> Result<()> {
    client.emit("social.invite", Value::String(user_id.to_string()));
    let mut rx = client.ribbon.emitter.subscribe();

    let timeout = tokio::time::sleep(Duration::from_millis(100));
    tokio::pin!(timeout);

    loop {
      tokio::select! {
          _ = &mut timeout => return Ok(()),
          msg = rx.recv() => {
              match msg {
                  Ok((cmd, data)) if cmd == "client.error" => {
                      let err = data.as_str().unwrap_or("invite failed").to_string();
                      return Err(TriangleError::Ribbon(err));
                  }
                  Ok(_) => {}
                  Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                  Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
              }
          }
      }
    }
  }

  pub fn status(&self, client: &Client, status: Status, detail: Option<&str>) {
    let status_value = match status {
      Status::Online => "online",
      Status::Away => "away",
      Status::Busy => "busy",
      Status::Offline => "offline",
    };

    client.emit(
      "social.presence",
      json!({ "status": status_value, "detail": detail.unwrap_or("") }),
    );
  }
}
