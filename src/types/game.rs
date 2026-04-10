use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
  #[default]
  Z,
  D,
  #[serde(rename = "d+")]
  DPlus,
  #[serde(rename = "c-")]
  CMinus,
  C,
  #[serde(rename = "c+")]
  CPlus,
  #[serde(rename = "b-")]
  BMinus,
  B,
  #[serde(rename = "b+")]
  BPlus,
  #[serde(rename = "a-")]
  AMinus,
  A,
  #[serde(rename = "a+")]
  APlus,
  #[serde(rename = "s-")]
  SMinus,
  S,
  #[serde(rename = "s+")]
  SPlus,
  #[serde(rename = "ss")]
  SS,
  U,
  X,
  #[serde(rename = "x+")]
  XPlus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GarbageEntry {
  Instant,
  Continuous,
  Delayed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GarbageBlocking {
  #[serde(rename = "combo blocking")]
  ComboBlocking,
  #[serde(rename = "limited blocking")]
  LimitedBlocking,
  #[serde(rename = "none")]
  None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GarbageTargetBonus {
  #[serde(rename = "offensive")]
  Offensive,
  #[serde(rename = "defensive")]
  Defensive,
  #[serde(rename = "none")]
  None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Passthrough {
  #[serde(rename = "zero")]
  Zero,
  #[serde(rename = "limited")]
  Limited,
  #[serde(rename = "consistent")]
  Consistent,
  #[serde(rename = "full")]
  Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpinBonuses {
  #[serde(rename = "T-spins")]
  TSpins,
  #[serde(rename = "T-spins+")]
  TSpinsPlus,
  #[serde(rename = "all")]
  All,
  #[serde(rename = "all+")]
  AllPlus,
  #[serde(rename = "all-mini")]
  AllMini,
  #[serde(rename = "all-mini+")]
  AllMiniPlus,
  #[serde(rename = "mini-only")]
  MiniOnly,
  #[serde(rename = "handheld")]
  Handheld,
  #[serde(rename = "stupid")]
  Stupid,
  #[serde(rename = "none")]
  None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundingMode {
  Down,
  Rng,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComboTable {
  #[serde(rename = "none")]
  None,
  #[serde(rename = "multiplier")]
  Multiplier,
  #[serde(rename = "classic guideline")]
  ClassicGuideline,
  #[serde(rename = "modern guideline")]
  ModernGuideline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
  Versus,
  Royale,
  Practice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameOverReason {
  Topout,
  #[serde(rename = "garbagesmash")]
  GarbageSmash,
  Zenith,
  Clear,
  #[serde(rename = "topout_clear")]
  TopoutClear,
  Winner,
  Forfeit,
  Retry,
  Drop,
  #[serde(rename = "dropnow")]
  DropNow,
  Disconnect,
}

/// Handling settings sent within `server.authorize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handling {
  pub arr: f64,
  pub das: f64,
  pub dcd: f64,
  pub sdf: f64,
  pub safelock: bool,
  pub cancel: bool,
  pub may20g: bool,
  /// `"off"` | `"hold"` | `"tap"`
  pub irs: String,
  /// `"off"` | `"hold"` | `"tap"`
  pub ihs: String,
}

impl Default for Handling {
  fn default() -> Self {
    Self {
      arr: 0.0,
      das: 6.0,
      dcd: 0.0,
      sdf: 41.0,
      safelock: false,
      cancel: false,
      may20g: false,
      irs: "off".to_string(),
      ihs: "off".to_string(),
    }
  }
}

/// Full game options object as seen in room configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Options {
  pub version: Option<u32>,
  pub seed_random: Option<bool>,
  pub seed: Option<u64>,
  pub g: Option<f64>,
  pub stock: Option<u32>,
  pub countdown: Option<bool>,
  pub countdown_count: Option<u32>,
  pub countdown_interval: Option<f64>,
  pub precountdown: Option<f64>,
  pub prestart: Option<f64>,
  pub hasgarbage: Option<bool>,
  pub bgmnoreset: Option<bool>,
  pub neverstopbgm: Option<bool>,
  pub display_next: Option<bool>,
  pub display_hold: Option<bool>,
  pub infinite_hold: Option<bool>,
  pub gmargin: Option<f64>,
  pub gincrease: Option<f64>,
  pub garbagemultiplier: Option<f64>,
  pub garbagemargin: Option<f64>,
  pub garbageincrease: Option<f64>,
  pub garbagecap: Option<f64>,
  pub garbagecapincrease: Option<f64>,
  pub garbagecapmargin: Option<f64>,
  pub garbagecapmax: Option<f64>,
  pub garbageabsolutecap: Option<f64>,
  pub garbageholesize: Option<u32>,
  pub garbagephase: Option<u32>,
  pub garbagequeue: Option<bool>,
  pub garbageare: Option<u32>,
  pub garbageentry: Option<GarbageEntry>,
  pub garbageblocking: Option<GarbageBlocking>,
  pub garbagetargetbonus: Option<GarbageTargetBonus>,
  pub garbagespecialbonus: Option<bool>,
  pub usebombs: Option<bool>,
  pub bagtype: Option<String>,
  pub spinbonuses: Option<SpinBonuses>,
  pub combotable: Option<ComboTable>,
  pub kickset: Option<String>,
  pub nextcount: Option<u32>,
  pub infinite_movement: Option<bool>,
  pub allow_harddrop: Option<bool>,
  pub display_shadow: Option<bool>,
  pub locktime: Option<u32>,
  pub garbagespeed: Option<f64>,
  pub forfeit_time: Option<f64>,
  pub are: Option<u32>,
  pub lineclear_are: Option<u32>,
  pub lockresets: Option<u32>,
  pub allow180: Option<bool>,
  pub gravitymay20g: Option<bool>,
  pub room_handling: Option<bool>,
  pub room_handling_arr: Option<f64>,
  pub room_handling_das: Option<f64>,
  pub room_handling_sdf: Option<f64>,
  pub handling: Option<Handling>,
  pub manual_allowed: Option<bool>,
  pub b2bchaining: Option<bool>,
  pub b2bcharging: Option<bool>,
  pub b2bcharge_at: Option<u32>,
  pub b2bcharge_base: Option<u32>,
  pub b2bextras: Option<bool>,
  pub allclears: Option<bool>,
  pub allclear_garbage: Option<u32>,
  pub allclear_b2b: Option<u32>,
  pub allclear_b2b_sends: Option<bool>,
  pub allclear_b2b_dupes: Option<bool>,
  pub allclear_charges: Option<bool>,
  pub openerphase: Option<u32>,
  pub garbagearebump: Option<u32>,
  pub roundmode: Option<RoundingMode>,
  pub clutch: Option<bool>,
  pub nolockout: Option<bool>,
  pub passthrough: Option<Passthrough>,
  pub can_undo: Option<bool>,
  pub can_retry: Option<bool>,
  pub retryisclear: Option<bool>,
  pub noextrawidth: Option<bool>,
  pub stride: Option<bool>,
  pub username: Option<String>,
  pub boardwidth: Option<u32>,
  pub boardheight: Option<u32>,
  pub new_payback: Option<bool>,
  pub messiness_change: Option<f64>,
  pub messiness_inner: Option<f64>,
  pub messiness_center: Option<bool>,
  pub messiness_nosame: Option<bool>,
  pub messiness_timeout: Option<f64>,
  #[serde(flatten)]
  pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ready {
  pub gameid: u32,
  pub options: Options,
  pub players: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaderboard {
  pub id: String,
  pub username: String,
  pub active: bool,
  pub naturalorder: i32,
  pub alive: bool,
  pub lifetime: i64,
  pub stats: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scoreboard {
  pub id: String,
  pub username: String,
  pub active: bool,
  pub naturalorder: i32,
  pub alive: bool,
  pub lifetime: i64,
  pub stats: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchData {
  pub gameid: Option<u32>,
  pub gamemode: Option<String>,
}
