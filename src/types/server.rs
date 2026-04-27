use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Worker {
  pub name: String,
  pub flag: String,
}

pub mod signature {
  use serde::{Deserialize, Serialize};
  use serde_json::Value;
  use std::collections::HashMap;

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct Signature {
    pub version: String,
    pub countdown: bool,
    pub novault: bool,
    pub noceriad: bool,
    pub norichpresence: bool,
    pub noreplaydispute: bool,
    pub supporter_specialthanks_goal: u64,
    pub xp_multiplier: f64,
    pub catalog: Catalog,
    pub league_mm_roundtime_min: u64,
    pub league_mm_roundtime_max: u64,
    pub league_additional_settings: HashMap<String, Value>,
    pub league_season: LeagueSeason,
    pub zenith_duoisfree: bool,
    pub zenith_freemod: bool,
    pub zenith_cpu_count: u64,
    pub zenith_additional_settings: ZenithAdditionalSettings,
    pub domain: String,
    pub ch_domain: String,
    pub mode: String,
    pub sentry_enabled: bool,
    #[serde(rename = "serverCycle")]
    pub server_cycle: String,
    pub domain_hash: String,
    pub client: Client,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct Catalog {
    pub supporter: CatalogSupporter,
    #[serde(rename = "zenith-tower-ost")]
    pub zenith_tower_ost: CatalogZenithTowerOst,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct CatalogSupporter {
    pub price: u64,
    pub price_bulk: u64,
    pub price_gift: u64,
    pub price_gift_bulk: u64,
    pub bulk_after: u64,
    pub normal_price: u64,
    pub normal_price_bulk: u64,
    pub normal_price_gift: u64,
    pub normal_price_gift_bulk: u64,
    pub normal_bulk_after: u64,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct CatalogZenithTowerOst {
    pub price: u64,
    pub normal_price: u64,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct LeagueSeason {
    pub current: String,
    pub prev: String,
    pub next: Option<String>,
    pub next_at: Option<String>,
    pub ranked: bool,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct ZenithAdditionalSettings {
    #[serde(rename = "TEMP_zenith_grace")]
    pub temp_zenith_grace: String,
    pub messiness_timeout: u64,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct Client {
    pub commit: Commit,
    pub branch: String,
    pub build: Build,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct Commit {
    pub id: String,
    pub time: u64,
  }

  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub struct Build {
    pub id: String,
    pub time: u64,
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
  pub players: u32,
  pub users: u32,
  pub gamesplayed: u32,
  pub gametime: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
  pub stats: Stats,
  pub signature: signature::Signature,
  pub vx: String,
}
