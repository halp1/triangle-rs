use tokio::select;

use crate::{
  classes::{Ribbon, ribbon::WrapError},
  types::{
    events::{recv, send},
    social::{dm, relationship as rel},
  },
  utils::api::core::ApiError,
};

#[derive(Debug, Clone)]
pub struct ProcessedRelationship {
  pub original: rel::Relationship,
  pub user_id: String,
  pub user_username: String,
  pub user_avatar: Option<u64>,
}

#[derive(Debug)]
pub struct Relationship {
  ribbon: Ribbon,

  pub user_id: String,
  pub relationship_id: String,
  pub username: String,
  pub avatar: Option<u64>,
  pub dms: Vec<dm::DM>,

  pub dms_loaded: bool,
}

pub enum DMError {
  Error(String),
  SendFail(String),
  Spam,
}

pub async fn send_dm(
  ribbon: &mut Ribbon,
  recipient_id: &str,
  content: &str,
) -> Result<recv::social::DM, DMError> {
  match ribbon
    .wrap_with_error::<recv::social::DM>(
      send::social::DM {
        recipient: recipient_id.into(),
        msg: content.into(),
      },
      &["social.dm.fail", "staff.spam", "client.error"],
    )
    .await
  {
    Ok(dm) => Ok(dm),
    Err(e) => match e {
      WrapError::ParseError => Err(DMError::Error(format!("Failed to parse DM response"))),
      WrapError::ServerError => Err(DMError::Error(format!("Connection error while sending DM"))),
      WrapError::Error(event, data) => match event.as_str() {
        "social.dm.fail" => Err(DMError::SendFail(
          data.as_str().unwrap_or("Unknown error").to_string(),
        )),
        "staff.spam" => Err(DMError::Spam),
        _ => Err(DMError::Error(
          data
            .as_str()
            .unwrap_or(format!("Unknown error: {}", event).as_str())
            .to_string(),
        )),
      },
    },
  }
}

pub async fn invite(ribbon: &mut Ribbon, recipient_id: &str) -> Result<(), String> {
  ribbon.emit(send::social::Invite(recipient_id.into())).await;

  select! {
    biased;

    err = ribbon.wait::<recv::client::Error>() => Err(if let Some(err) = err { err.0 } else { "Server error while sending invite".to_string() }),
    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => Ok(()),
  }
}

// TODO: non lazy load dms

impl Relationship {
  pub fn new(ribbon: Ribbon, relationship: ProcessedRelationship) -> Self {
    Self {
      ribbon,
      user_id: relationship.user_id.clone(),
      relationship_id: relationship.original.id.clone(),
      username: relationship.user_username.clone(),
      avatar: relationship.user_avatar,
      dms: Vec::new(),
      dms_loaded: false,
    }
  }

  pub async fn dm(&mut self, content: &str) -> Result<recv::social::DM, DMError> {
    send_dm(&mut self.ribbon, &self.user_id, content).await
  }

  pub async fn mark_as_read(&mut self) {
    self.ribbon.emit(send::social::relation::Ack {}).await;
  }

  pub async fn load_dms(&mut self) -> Result<Vec<dm::DM>, ApiError> {
    let dms = self.ribbon.api.social.dms(&self.user_id).await?;
    self.dms = dms.iter().rev().cloned().collect();
    self.dms_loaded = true;
    Ok(dms)
  }

  pub async fn invite(&mut self) -> Result<(), String> {
    invite(&mut self.ribbon, &self.user_id).await
  }
}
