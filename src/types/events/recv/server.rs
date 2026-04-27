use crate::macros::event;
use crate::types::{server, social};
use serde_json::Value;

event!(server.authorize => Authorize {
  success: bool,
  maintenance: bool,
  worker: server::Worker,
  social: social::Summary,
	relationships: Vec<social::relationship::Relationship>,
});

event!(server.migrate => Migrate {
  endpoint: String,
  name: String,
  flag: String,
});

event!(server.migrated => Migrated(Value));

event!(server.maintenance => Maintenance(Value));

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Announcement {
  #[serde(rename = "type")]
  pub announcement_type: String,
  pub msg: String,
  pub ts: u64,
  pub reason: Option<String>,
}

impl crate::utils::events::Event for Announcement {
  const NAME: &'static str = "server.announcement";
}
