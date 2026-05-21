pub mod relationship;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::{
  classes::{
    ClientUser, Ribbon,
    ribbon::Hook,
    social::relationship::{DMError, Relationship},
  },
  types::{
    events::{recv, send},
    social::{self, Blocked, Config as SocialConfig, Notification, relationship as rel},
    user::User,
  },
  utils::api::core::ApiError,
};

fn process_relationship(
  r: rel::Relationship,
  self_id: &str,
) -> relationship::ProcessedRelationship {
  let target = if r.from.id == *self_id {
    &r.to
  } else {
    &r.from
  };

  relationship::ProcessedRelationship {
    original: r.clone(),
    user_id: target.id.clone(),
    user_username: target.username.clone(),
    user_avatar: target.avatar_revision,
  }
}

#[derive(Debug, Clone)]
pub struct Social {
  ribbon: Ribbon,
  hook: Hook,

  me: ClientUser,

  online: Arc<Mutex<u32>>,

  pub friends: Arc<Mutex<Vec<Relationship>>>,
  pub other: Arc<Mutex<Vec<Relationship>>>,
  pub blocked: Arc<Mutex<Vec<Blocked>>>,
  pub notifications: Arc<Mutex<Vec<Notification>>>,
  pub config: Arc<Mutex<SocialConfig>>,
}

impl Social {
  pub async fn new(
    ribbon: Ribbon,
    me: ClientUser,
    config: SocialConfig,
    init: social::Summary,
  ) -> Self {
    let relationships = init
      .relationships
      .into_iter()
      .map(|r| process_relationship(r, &me.id))
      .collect::<Vec<_>>();

    let social = Self {
      ribbon: ribbon.clone(),
      hook: ribbon.hook(),
      me,
      online: Arc::new(Mutex::new(init.total_online)),
      friends: Arc::new(Mutex::new(
        relationships
          .iter()
          .filter(|r| matches!(r.original.relationship_type, rel::Type::Friend))
          .map(|r| Relationship::new(ribbon.clone(), (*r).clone()))
          .collect(),
      )),
      other: Arc::new(Mutex::new(
        relationships
          .iter()
          .filter(|r| !matches!(r.original.relationship_type, rel::Type::Friend))
          .map(|r| Relationship::new(ribbon.clone(), (*r).clone()))
          .collect(),
      )),
      blocked: Arc::new(Mutex::new(
        relationships
          .into_iter()
          .filter(|r| matches!(r.original.relationship_type, rel::Type::Block))
          .map(|r| Blocked {
            id: r.user_id.clone(),
            username: r.user_username.clone(),
            avatar: r.user_avatar,
          })
          .collect(),
      )),
      notifications: Arc::new(Mutex::new(init.notifications)),
      config: Arc::new(Mutex::new(config)),
    };

    social.init().await;

    social
  }

  async fn init(&self) {
    let online = self.online.clone();
    let _ = self.hook.on::<recv::social::Online>(async move |data| {
      let mut online = online.lock();
      *online = data.0;
    });

    let notifications = self.notifications.clone();
    let other = self.other.clone();
    let me = self.me.clone();
    let auto_process_notifications = self.config.lock().auto_process_notifications;
    let ribbon = self.ribbon.clone();

    let _ = self
      .ribbon
      .on::<recv::social::Notification>(async move |n| {
        {
          let mut notifications = notifications.lock();
          notifications.insert(0, n.clone());
        }

        if auto_process_notifications {
          if n.notification_type == "friend" {
            if let Some(rel) = n.data.get("relationship") {
              if let Ok(rel) = serde_json::from_value::<rel::Relationship>(rel.clone()) {
                let processed = process_relationship(rel.clone(), &me.id);
                let c = processed.clone();
                ribbon
                  .emit(send::client::Friended {
                    id: c.user_id,
                    name: c.user_username,
                    avatar: c.user_avatar,
                  })
                  .await
                  .ok();

                let mut other = other.lock();
                if !other.iter().any(|r| r.user_id == processed.user_id) {
                  other.push(Relationship::new(
                    ribbon.clone(),
                    process_relationship(rel.clone(), &me.id),
                  ));
                }
              }
            }
          }
        }
      });

    let other = self.other.clone();
    let friends = self.friends.clone();
    let auto_load_dms = self.config.lock().auto_load_dms;
    let me = self.me.clone();
    let ribbon = self.ribbon.clone();

    let _ = self.hook.on::<recv::social::DM>(async move |raw| {
      let mut target = raw.data.user.clone();
      let mut username = "".to_string();

      if target == me.id {
        if let Some(id) = raw.stream.split(':').find(|id| *id != me.id) {
          target = id.to_string();
        } else {
          return;
        }
      }

      let user = {
        let other = other.lock();
        let friends = friends.lock();

        other
          .iter()
          .find(|u| u.user_id == target)
          .or_else(|| friends.iter().find(|u| u.user_id == target))
          .cloned()
      };

      if let Some(user) = user {
        if !user.dms_loaded().await && auto_load_dms {
          user.load_dms().await.ok();
        } else {
          user._add_dm(raw.clone()).await;
        }

        username = user.username.clone();
      } else {
        if let Ok(u) = ribbon.api.users.get(&target.clone()).await {
          let new_rel = Relationship::new(
            ribbon.clone(),
            relationship::ProcessedRelationship {
              original: rel::Relationship {
                unread: 0,
                updated: "".into(),
                id: "".to_string(),
                from: rel::User {
                  id: "".into(),
                  username: "".into(),
                  avatar_revision: None,
                },
                to: rel::User {
                  id: "".into(),
                  username: "".into(),
                  avatar_revision: None,
                },
                relationship_type: rel::Type::Pending,
              },
              user_id: u.id,
              user_username: u.username.clone(),
              user_avatar: u.avatar_revision,
            },
          );

          other.lock().push(new_rel);

          username = u.username;
        } else {
          tracing::warn!("Failed to fetch user information for DM");
        }
      }

      if raw.data.user == me.id {
        return;
      }

      ribbon
        .emit(send::client::DM {
          user_id: target,
          username: username.into(),
          content: raw.data.content.clone(),
          raw,
        })
        .await
        .ok();
    });

    let auto_process_notifications = self.config.lock().auto_process_notifications;

    if auto_process_notifications {
      let notifications = self.notifications.lock().clone();
      for n in &notifications {
        if !n.seen {
          if n.notification_type == "friend" {
            if let Some(rel) = n.data.get("relationship") {
              if let Ok(rel) = serde_json::from_value::<rel::Relationship>(rel.clone()) {
                let processed = process_relationship(rel, &self.me.id);
                self
                  .ribbon
                  .emit(send::client::Friended {
                    id: processed.user_id,
                    name: processed.user_username,
                    avatar: processed.user_avatar,
                  })
                  .await
                  .ok();
              }
            }
          }
        }
      }
      self.mark_notifications_as_read().await;
    }
  }

  /// total number of online players, updated by `social.online` events
  pub async fn online(&self) -> u32 {
    *self.online.lock()
  }

  pub async fn mark_notifications_as_read(&self) {
    self
      .ribbon
      .emit(send::social::notification::Ack {})
      .await
      .ok();

    self
      .notifications
      .lock()
      .iter_mut()
      .for_each(|n| n.seen = true);
  }

  // todo: get. problem: holds mutex lock

  pub async fn resolve(&self, username: &str) -> Result<String, ApiError> {
    self.ribbon.api.users.resolve(username).await
  }

  pub async fn who(&self, user_id: &str) -> Result<User, ApiError> {
    self.ribbon.api.users.get(user_id).await
  }

  pub async fn dm(
    &self,
    user_id: impl ToString,
    content: impl ToString,
  ) -> Result<recv::social::DM, DMError> {
    relationship::send_dm(&self.ribbon, user_id, content).await
  }

  pub async fn friend(&self, user_id: &str) -> Result<bool, ApiError> {
    if self.friends.lock().iter().any(|f| f.user_id == user_id) {
      return Ok(false);
    }
    self.ribbon.api.social.friend(user_id).await?;

    self.friends.lock().push(Relationship::new(
      self.ribbon.clone(),
      relationship::ProcessedRelationship {
        original: rel::Relationship {
          unread: 0,
          updated: "".into(),
          id: "".to_string(),
          from: rel::User {
            id: "".into(),
            username: "".into(),
            avatar_revision: None,
          },
          to: rel::User {
            id: "".into(),
            username: "".into(),
            avatar_revision: None,
          },
          relationship_type: rel::Type::Friend,
        },
        user_id: user_id.into(),
        user_username: user_id.into(),
        user_avatar: None,
      },
    ));

    Ok(true)
  }

  pub async fn unfriend(&self, user_id: &str) -> Result<bool, ApiError> {
    if self.friends.lock().iter().all(|f| f.user_id != user_id) {
      return Ok(false);
    }

    self.ribbon.api.social.unfriend(user_id).await?;

    self.friends.lock().retain(|f| f.user_id != user_id);
    Ok(true)
  }

  pub async fn block(&self, user_id: &str) -> Result<bool, ApiError> {
    if self.blocked.lock().iter().any(|b| b.id == user_id) {
      return Ok(false);
    }

    self.ribbon.api.social.block(user_id).await?;

    self.blocked.lock().push(Blocked {
      id: user_id.into(),
      username: user_id.into(),
      avatar: None,
    });

    // also unfriend if they're a friend
    self.friends.lock().retain(|f| f.user_id != user_id);

    Ok(true)
  }

  pub async fn unblock(&self, user_id: &str) -> Result<bool, ApiError> {
    if self.blocked.lock().iter().all(|b| b.id != user_id) {
      return Ok(false);
    }

    self.ribbon.api.social.unblock(user_id).await?;

    self.blocked.lock().retain(|b| b.id != user_id);

    Ok(true)
  }

  pub async fn invite(&self, user_id: &str) -> Result<(), String> {
    relationship::invite(&self.ribbon, user_id).await
  }

  pub async fn set_status(&self, status: social::Status, detail: social::Detail) {
    self
      .ribbon
      .emit(send::social::Presence { status, detail })
      .await
      .ok();
  }

  pub fn destroy(&self) {
    self.hook.destroy();
  }
}
