use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::macros::partial;

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GarbageBlocking {
  #[serde(rename = "combo blocking")]
  ComboBlocking,
  #[serde(rename = "limited blocking")]
  LimitedBlocking,
  #[serde(rename = "none")]
  None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GarbageTargetBonus {
  #[serde(rename = "offensive")]
  Offensive,
  #[serde(rename = "defensive")]
  Defensive,
  #[serde(rename = "none")]
  None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Spin {
  #[serde(rename = "none")]
  None,
  #[serde(rename = "mini")]
  Mini,
  #[serde(rename = "normal")]
  Normal,
}

impl Spin {
	pub fn as_str(&self) -> &'static str {
		match self {
			Spin::None => "none",
			Spin::Mini => "mini",
			Spin::Normal => "normal",
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
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
  HundredBattleRoyale,
  #[serde(rename = "classic")]
  Classic,
  #[serde(rename = "arcade")]
  Arcade,
  #[serde(rename = "bombs")]
  Bombs,
  #[serde(rename = "quickplay")]
  Quickplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundingMode {
  Down,
  Rng,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

impl ComboTable {
  pub fn data(&self) -> &[u8] {
    match self {
      ComboTable::None => &[0],
      ComboTable::Multiplier => panic!("Multiplier combo table does not have predefined data"),
      ComboTable::ClassicGuideline => &[0, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5],
      ComboTable::ModernGuideline => &[0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4],
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
  Versus,
  Royale,
  Practice,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Buffering {
  Off,
  Hold,
  Tap,
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
  pub irs: Buffering,
  pub ihs: Buffering,
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
      irs: Buffering::Tap,
      ihs: Buffering::Tap,
    }
  }
}

partial!(Options {
  version: u32,
  seed_random: bool,
  seed: u64,
  g: f64,
  stock: u32,
  countdown: bool,
  countdown_count: u32,
  countdown_interval: f64,
  precountdown: f64,
  prestart: f64,
  hasgarbage: bool,
  bgmnoreset: bool,
  neverstopbgm: bool,
  display_next: bool,
  display_hold: bool,
  infinite_hold: bool,
  gmargin: f64,
  gincrease: f64,
  garbagemultiplier: f64,
  garbagemargin: f64,
  garbageincrease: f64,
  garbagecap: f64,
  garbagecapincrease: f64,
  garbagecapmargin: f64,
  garbagecapmax: f64,
  garbageabsolutecap: f64,
  garbageholesize: u32,
  garbagephase: u32,
  garbagequeue: bool,
  garbageare: u32,
  garbageentry: GarbageEntry,
  garbageblocking: GarbageBlocking,
  garbagetargetbonus: GarbageTargetBonus,
  garbagespecialbonus: bool,
  usebombs: bool,
  bagtype: String,
  spinbonuses: SpinBonuses,
  combotable: ComboTable,
  kickset: String,
  nextcount: u32,
  infinite_movement: bool,
  allow_harddrop: bool,
  display_shadow: bool,
  locktime: u32,
  garbagespeed: f64,
  forfeit_time: f64,
  are: u32,
  lineclear_are: u32,
  lockresets: u32,
  allow180: bool,
  gravitymay20g: bool,
  room_handling: bool,
  room_handling_arr: f64,
  room_handling_das: f64,
  room_handling_sdf: f64,
  handling: Handling,
  manual_allowed: bool,
  b2bchaining: bool,
  b2bcharging: bool,
  b2bcharge_at: u32,
  b2bcharge_base: u32,
  b2bextras: bool,
  allclears: bool,
  allclear_garbage: u32,
  allclear_b2b: u32,
  allclear_b2b_sends: bool,
  allclear_b2b_dupes: bool,
  allclear_charges: bool,
  openerphase: u32,
  garbagearebump: u32,
  roundmode: RoundingMode,
  clutch: bool,
  nolockout: bool,
  passthrough: Passthrough,
  can_undo: bool,
  can_retry: bool,
  retryisclear: bool,
  noextrawidth: bool,
  stride: bool,
  username: String,
  boardwidth: u32,
  boardheight: u32,
  new_payback: bool,
  messiness_change: f64,
  messiness_inner: f64,
  messiness_center: bool,
  messiness_nosame: bool,
  messiness_timeout: f64,
  extra: serde_json::Value,
});

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
  pub alive: Option<bool>,
  pub wins: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectatingStrategy {
  Instant,
  Smooth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayStateState {
  pub frame: u64,
  pub game: Value,
  overrides: Value,
}

#[derive(Debug, Clone)]
pub enum ReplayState {
  Early,
  Wait,
  State(ReplayStateState),
}

impl<'de> Deserialize<'de> for ReplayState {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = Value::deserialize(deserializer)?;
    match value {
      Value::String(v) => match v.as_str() {
        "early" => Ok(ReplayState::Early),
        "wait" => Ok(ReplayState::Wait),
        _ => Err(serde::de::Error::custom(format!(
          "invalid replay state: {}",
          v
        ))),
      },
      _ => serde_json::from_value::<ReplayStateState>(value)
        .map(ReplayState::State)
        .map_err(serde::de::Error::custom),
    }
  }
}

impl Serialize for ReplayState {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match self {
      ReplayState::Early => serializer.serialize_str("early"),
      ReplayState::Wait => serializer.serialize_str("wait"),
      ReplayState::State(state) => state.serialize(serializer),
    }
  }
}

pub mod ige {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Target {
    pub targets: Vec<u64>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct AllowTargeting {
    pub value: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Targeted {
    pub value: bool,
    pub gameid: u64,
    pub frame: u64,
  }

  pub mod interaction {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Garbage {
      pub frame: u64,
      pub gameid: u64,
      pub iid: u64,
      pub cid: u64,
      pub ackiid: u64,
      pub amt: u64,
      pub x: f64,
      pub y: f64,
      pub size: u64,

      #[serde(default)]
      pub zthalt: Option<serde_json::Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Targeted {
      pub value: bool,
      pub gameid: u64,
      pub frame: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum InteractionData {
      #[serde(rename = "garbage")]
      Garbage(Garbage),

      #[serde(rename = "targeted")]
      Targeted(Targeted),
    }
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Player {
    pub gameid: u64,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct KEV {
    pub victim: Player,
    pub killer: Player,
    pub frame: u64,
    pub fire: f64,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "type", content = "data")]
  pub enum IGEData {
    #[serde(rename = "target")]
    Target(Target),

    #[serde(rename = "allow_targeting")]
    AllowTargeting(AllowTargeting),

    #[serde(rename = "targeted")]
    Targeted(Targeted),

    #[serde(rename = "kev")]
    KEV(KEV),

    #[serde(rename = "interaction")]
    Interaction(interaction::InteractionData),

    #[serde(rename = "interaction_confirm")]
    InteractionConfirm(interaction::InteractionData),
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct IGE {
    pub id: u64,
    pub frame: u64,
    #[serde(flatten)]
    pub data: IGEData,
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Killer {
  pub gameid: u64,
  pub r#type: String,
  pubusername: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEndData {
  #[serde(rename = "gameoverreason")]
  pub game_over_reason: GameOverReason,
  pub killer: Killer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
#[serde(into = "u8", try_from = "u8")]
pub enum TargetingStrategy {
  Even = 0,
  Eliminations = 1,
  Random = 2,
  Payback = 3,
  Manual(u64),
}

impl From<TargetingStrategy> for u8 {
  fn from(m: TargetingStrategy) -> Self {
    match m {
      TargetingStrategy::Manual(_) => panic!("Manual targeting strategy cannot be converted to u8"),
      TargetingStrategy::Even => 0,
      TargetingStrategy::Eliminations => 1,
      TargetingStrategy::Random => 2,
      TargetingStrategy::Payback => 3,
    }
  }
}

impl TryFrom<u8> for TargetingStrategy {
  type Error = String;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
    match value {
      0 => Ok(TargetingStrategy::Even),
      1 => Ok(TargetingStrategy::Eliminations),
      2 => Ok(TargetingStrategy::Random),
      3 => Ok(TargetingStrategy::Payback),
      4 => Err("Manual targeting strategy cannot be created from u8".to_string()),
      _ => Err(format!("invalid TargetingStrategy: {}", value)),
    }
  }
}

// | "moveLeft"
// | "moveRight"
// | "rotateCW"
// | "rotateCCW"
// | "rotate180"
// | "softDrop"
// | "hardDrop"
// | "hold"
// | "undo"
// | "redo"
// | "retry"

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
  #[serde(rename = "moveLeft")]
  MoveLeft,
  #[serde(rename = "moveRight")]
  MoveRight,
  #[serde(rename = "rotateCW")]
  RotateCW,
  #[serde(rename = "rotateCCW")]
  RotateCCW,
  #[serde(rename = "rotate180")]
  Rotate180,
  #[serde(rename = "softDrop")]
  SoftDrop,
  #[serde(rename = "hardDrop")]
  HardDrop,
  #[serde(rename = "hold")]
  Hold,
  #[serde(rename = "undo")]
  Undo,
  #[serde(rename = "redo")]
  Redo,
  #[serde(rename = "retry")]
  Retry,
}

impl Key {
  pub fn as_str(&self) -> &'static str {
    match self {
      Key::MoveLeft => "moveLeft",
      Key::MoveRight => "moveRight",
      Key::RotateCW => "rotateCW",
      Key::RotateCCW => "rotateCCW",
      Key::Rotate180 => "rotate180",
      Key::SoftDrop => "softDrop",
      Key::HardDrop => "hardDrop",
      Key::Hold => "hold",
      Key::Undo => "undo",
      Key::Redo => "redo",
      Key::Retry => "retry",
    }
  }
}

pub mod replay {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Start(pub Value);

  // TODO: type
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Full {
    pub game: Value,
    pub stats: Value,
    pub diyusi: u64,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Keypress {
    pub key: Key,
    pub subframe: f64,
    #[serde(default)]
    pub hoisted: bool,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct End(pub Value);

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "type", content = "data")]
  pub enum FrameData {
    #[serde(rename = "start")]
    Start(Start),
    #[serde(rename = "full")]
    Full(Full),
    #[serde(rename = "ige")]
    IGE(ige::IGE),
    #[serde(rename = "keydown")]
    KeyDown(Keypress),
    #[serde(rename = "keyup")]
    KeyUp(Keypress),
    #[serde(rename = "end")]
    End(End),
    #[serde(rename = "strategy")]
    Strategy(TargetingStrategy),
    #[serde(rename = "manual_target")]
    ManualTarget(u64),
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Frame {
    pub frame: u64,
    #[serde(flatten)]
    pub data: FrameData,
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyPlayer {
  pub gameid: u64,
  pub userid: String,
  pub options: Value,
  pub alive: bool,
  pub naturalorder: u32,
}

pub mod tick {
  use std::sync::Arc;

  use futures_util::future::BoxFuture;
  use tokio::sync::Mutex;

  use crate::{Engine, engine};

  #[derive(Debug, Clone, Eq, PartialEq)]
  pub enum KeypressType {
    Keydown,
    Keyup,
  }

  #[derive(Debug, Clone)]
  pub struct KeypressData {
    pub key: super::Key,
    pub subframe: f64,
    pub hoisted: bool,
  }

  #[derive(Debug, Clone)]
  pub struct Keypress {
    pub r#type: KeypressType,
    pub frame: u64,
    pub data: KeypressData,
  }

  #[derive(Debug, Clone)]
  pub struct Garbage {
    pub column: usize,
    pub amount: u32,
  }

  impl From<engine::garbage::OutgoingGarbage> for Garbage {
    fn from(g: engine::garbage::OutgoingGarbage) -> Self {
      Self {
        column: g.column,
        amount: g.amount,
      }
    }
  }

  #[derive(Debug, Clone)]
  pub struct In {
    pub gameid: u64,
    pub engine: Engine,
    pub new_garbage: Vec<Garbage>,
  }

  pub trait RunAfter: Send {
    fn call(self: Box<Self>) -> BoxFuture<'static, ()>;
  }

  impl<F, Fut> RunAfter for F
  where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = ()> + Send + 'static,
  {
    fn call(self: Box<Self>) -> BoxFuture<'static, ()> {
      Box::pin((*self)())
    }
  }

  pub struct Out {
    pub keys: Vec<Keypress>,
    pub run_after: Vec<Box<dyn RunAfter + Send>>,
  }

  #[derive(Clone)]
  pub struct Ticker(pub Arc<Mutex<Box<dyn Fn(In) -> BoxFuture<'static, Out> + Send + Sync>>>);

  impl std::fmt::Debug for Ticker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      f.debug_tuple("Ticker").finish()
    }
  }

  impl Ticker {
    pub async fn inject(
      &self,
      func: impl Fn(In) -> BoxFuture<'static, Out> + Send + Sync + 'static,
    ) {
      let func = Box::new(func);
      let mut ticker = self.0.lock().await;
      *ticker = func;
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpectateTarget {
  GameIds(Vec<u64>),
  UserIds(Vec<String>),
  All,
}
