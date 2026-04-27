use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::queue::bag::BagType;
use crate::types::game::{
  ComboTable, GameMode, GarbageBlocking, GarbageEntry, GarbageTargetBonus, Passthrough, SpinBonuses,
};
use crate::types::user::{Badge, Role};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
  Custom,
  System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
  Ingame,
  Lobby,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bracket {
  Player,
  Spectator,
  Observer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
  #[serde(rename = "_id")]
  pub id: String,
  pub username: String,
  pub avatar_revision: Option<u64>,
  pub ready: bool,
  pub anon: bool,
  pub bot: bool,
  pub role: String,
  pub xp: u32,
  pub badges: Option<Vec<Badge>>,
  pub record: Record,
  pub bracket: Bracket,
  pub supporter: bool,
  pub verified: Option<bool>,
  pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
  pub games: u32,
  pub wins: u32,
  pub streak: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Autostart {
  pub enabled: bool,
  pub status: String,
  pub time: f64,
  pub maxtime: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
  pub gamemode: String,
  pub modename: String,
  pub ft: u32,
  pub wb: u32,
  pub gp: u32,
  pub record_replays: bool,
  pub stats: MatchStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStats {
  pub apm: MatchStat,
  pub pps: MatchStat,
  pub vsscore: MatchStat,
  pub garbagesent: MatchStat,
  pub garbagereceived: MatchStat,
  pub kills: MatchStat,
  pub altitude: MatchStat,
  pub rank: MatchStat,
  pub targetingfactor: MatchStat,
  pub targetinggrace: MatchStat,
  pub btb: MatchStat,
  pub revives: MatchStat,
  pub escapeartist: MatchStat,
  pub blockrationing_app: MatchStat,
  pub blockrationing_final: MatchStat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStat {
  pub key: String,
  #[serde(rename = "type")]
  pub stat_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct SetConfig {
  pub name: String,
  pub options: SetConfigOptions,
  pub user_limit: u32,
  pub auto_start: u32,
  pub allow_anonymous: bool,
  pub allow_unranked: bool,
  pub user_rank_limit: String,
  pub use_best_rank_as_limit: bool,
  pub force_require_xp_to_chat: bool,
  pub gamebgm: String,
  #[serde(rename = "match")]
  pub match_config: SetConfigMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SetConfigOptions {
  pub g: f64,
  pub stock: u32,
  pub display_next: bool,
  pub display_hold: bool,
  pub gmargin: f64,
  pub gincrease: f64,
  pub garbagemultiplier: f64,
  pub garbagemargin: f64,
  pub garbageincrease: f64,
  pub garbagecap: f64,
  pub garbagecapincrease: f64,
  pub garbagecapmax: f64,
  pub garbageattackcap: f64,
  pub garbageabsolutecap: f64,
  pub garbagephase: u32,
  pub garbagequeue: bool,
  pub garbageare: u32,
  pub garbageentry: GarbageEntry,
  pub garbageblocking: GarbageBlocking,
  pub garbagetargetbonus: GarbageTargetBonus,
  pub presets: String,
  pub bagtype: BagType,
  pub spinbonuses: SpinBonuses,
  pub combotable: ComboTable,
  pub kickset: String,
  pub nextcount: u32,
  pub allow_harddrop: bool,
  pub display_shadow: bool,
  pub locktime: u32,
  pub garbagespeed: u32,
  pub are: u32,
  pub lineclear_are: u32,
  pub infinitemovement: bool,
  pub lockresets: u32,
  pub allow180: bool,
  pub room_handling: bool,
  pub room_handling_arr: u32,
  pub room_handling_das: u32,
  pub room_handling_sdf: u32,
  pub manual_allowed: bool,
  pub b2bchaining: bool,
  pub b2bcharging: bool,
  pub openerphase: u32,
  pub allclear_garbage: u32,
  pub allclear_b2b: u32,
  pub garbagespecialbonus: bool,
  pub roundmode: String,
  pub allclears: bool,
  pub clutch: bool,
  pub nolockout: bool,
  pub passthrough: Passthrough,
  pub boardwidth: u32,
  pub boardheight: u32,
  pub messiness_change: u32,
  pub messiness_inner: u32,
  pub messiness_nosame: bool,
  pub messiness_timeout: u32,
  pub usebombs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetConfigMatch {
  pub gamemode: GameMode,
  pub modename: String,
  pub ft: u32,
  pub wb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetConfigItem {
  pub index: String,
  pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
  #[serde(rename = "default")]
  Default,
  #[serde(rename = "tetra league")]
  TetraLeague,
  #[serde(rename = "tetra league (season 1)")]
  TetraLeagueSeason1,
  #[serde(rename = "enforced delays")]
  EnforcedDelays,
  #[serde(rename = "4wide")]
  FourWide,
  #[serde(rename = "100 battle royale")]
  BattleRoyale,
  #[serde(rename = "classic")]
  Classic,
  #[serde(rename = "arcade")]
  Arcade,
  #[serde(rename = "bombs")]
  Bombs,
  #[serde(rename = "quickplay")]
  Quickplay,
}

impl Preset {
  pub fn as_str(&self) -> &'static str {
    match self {
      Preset::Default => "default",
      Preset::TetraLeague => "tetra league",
      Preset::TetraLeagueSeason1 => "tetra league (season 1)",
      Preset::EnforcedDelays => "enforced delays",
      Preset::FourWide => "4wide",
      Preset::BattleRoyale => "100 battle royale",
      Preset::Classic => "classic",
      Preset::Arcade => "arcade",
      Preset::Bombs => "bombs",
      Preset::Quickplay => "quickplay",
    }
  }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChatUser {
  pub username: String,
  #[serde(rename = "_id")]
  pub id: Option<String>,
  pub role: Option<Role>,
  pub supporter: Option<bool>,
  pub supporter_tier: Option<u32>,
}
