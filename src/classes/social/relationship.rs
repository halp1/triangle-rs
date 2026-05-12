use std::sync::Arc;

use tokio::{select, sync::Mutex};

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

#[derive(Debug, Clone)]
pub struct Relationship {
  ribbon: Ribbon,

  pub user_id: String,
  pub relationship_id: String,
  pub username: String,
  pub avatar: Option<u64>,
  dms: Arc<Mutex<(bool, Vec<dm::DM>)>>,
}

pub enum DMError {
  Error(String),
  SendFail(String),
  Spam,
}

pub async fn send_dm(
  ribbon: &Ribbon,
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
      WrapError::ParseError(e) => Err(DMError::Error(format!(
        "Failed to parse DM response: {}",
        e
      ))),
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

pub async fn invite(ribbon: &Ribbon, recipient_id: &str) -> Result<(), String> {
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
      dms: Arc::new(Mutex::new((false, Vec::new()))),
    }
  }

  pub async fn dm(&self, content: &str) -> Result<recv::social::DM, DMError> {
    send_dm(&self.ribbon, &self.user_id, content).await
  }

  pub async fn mark_as_read(&self) {
    self.ribbon.emit(send::social::relation::Ack {}).await;
  }

  pub async fn load_dms(&self) -> Result<Vec<dm::DM>, ApiError> {
    let dms = self.ribbon.api.social.dms(&self.user_id).await?;
    let mut dms_lock = self.dms.lock().await;
    dms_lock.1 = dms.iter().rev().cloned().collect();
    dms_lock.0 = true;
    Ok(dms)
  }

	pub async fn dms(&self) -> Vec<dm::DM> {
    let dms_lock = self.dms.lock().await;
    dms_lock.1.clone()
  }

	pub async fn dms_loaded(&self) -> bool {
		let dms_lock = self.dms.lock().await;
		dms_lock.0
	}

	pub async fn _add_dm(&self, dm: dm::DM) {
		let mut dms_lock = self.dms.lock().await;
		dms_lock.1.push(dm);
	}

  pub async fn invite(&self) -> Result<(), String> {
    invite(&self.ribbon, &self.user_id).await
  }
}
