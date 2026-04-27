use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::classes::game::TargetStrategy;
use crate::engine::Engine;
use crate::types::{
  game::{GameOverReason, Leaderboard, Scoreboard},
  social::{Dm as SocialDm, Notification as SocialNotification, Status as SocialStatus},
  user::Role,
};

/// Marker trait for all strongly-typed events.
/// Every event struct carries a unique `NAME` constant matching its ribbon / client event name.
pub trait TypedEvent: Clone + Send + 'static {
  const NAME: &'static str;
}

macro_rules! impl_typed_event {
  ($ty:ty, $name:expr) => {
    impl TypedEvent for $ty {
      const NAME: &'static str = $name;
    }
  };
}

// ── Tick callback types ───────────────────────────────────────────────────────

/// A single key event queued by the user's tick function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickKeypress {
  #[serde(rename = "type")]
  pub kind: TickKeypressKind,
  pub frame: f64,
  pub data: TickKeypressData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TickKeypressKind {
  Keydown,
  Keyup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickKeypressData {
  pub key: String,
  pub subframe: f64,
}

/// Input given to the user's tick function each game frame.
#[derive(Clone)]
pub struct TickInput {
  pub gameid: u32,
  pub frame: i32,
  pub events: Vec<GameClientEvent>,
  /// Read-only access to the engine (restored after tick returns to prevent mutations from persisting).
  pub engine: Arc<Mutex<Engine>>,
  /// Whether targeting is currently allowed by the server.
  pub can_target: bool,
  /// Gameids the server suggests as targets.
  pub server_targets: Vec<u32>,
  /// Gameids of opponents currently targeting the client.
  pub enemies: Vec<u32>,
  /// Shared key queue — push keys here to queue input for future frames.
  pub key_queue: Arc<Mutex<Vec<TickKeypress>>>,
  /// Shared targeting strategy — write to change the current target.
  pub target: Arc<Mutex<TargetStrategy>>,
  /// Pause the automatic flush of incoming garbage events when key queue is non-empty.
  pub pause_iges: Arc<Mutex<bool>>,
  /// Forcibly halt all incoming garbage regardless of key queue state.
  pub force_pause_iges: Arc<Mutex<bool>>,
}

/// Output returned by the user's tick function.
#[derive(Default)]
pub struct TickOutput {
  /// Keys to press this frame (validated and queued).
  pub keys: Option<Vec<TickKeypress>>,
  /// Callbacks to run after engine.tick().
  pub run_after: Option<Vec<Arc<dyn Fn() + Send + Sync>>>,
}

/// Type-erased tick function stored in Self_.
pub type RawTickFn = Box<
  dyn Fn(TickInput) -> std::pin::Pin<Box<dyn std::future::Future<Output = TickOutput> + Send>>
    + Send
    + Sync,
>;

/// Allows the user to register a tick function via `client.game.round.start`.
#[derive(Clone)]
pub struct TickSetter {
  pub(crate) inner: Arc<Mutex<Option<RawTickFn>>>,
}

impl TickSetter {
  pub(crate) fn new(inner: Arc<Mutex<Option<RawTickFn>>>) -> Self {
    Self { inner }
  }

  /// Register an async tick function.  Receives per-frame inputs and returns any
  /// keys to press this frame.
  pub fn set<F, Fut>(&self, f: F)
  where
    F: Fn(TickInput) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = TickOutput> + Send + 'static,
  {
    let mut lock = self.inner.blocking_lock();
    *lock = Some(Box::new(move |input| Box::pin(f(input))));
  }
}

impl std::fmt::Debug for TickSetter {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TickSetter").finish()
  }
}

// ── Client-level events ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientReady {
  pub endpoint: String,
  pub social: Value,
}
impl_typed_event!(ClientReady, "client.ready");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientFail {
  pub message: String,
}
impl_typed_event!(ClientFail, "client.fail");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientError(pub String);
impl_typed_event!(ClientError, "client.error");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDead(pub String);
impl_typed_event!(ClientDead, "client.dead");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientClose(pub String);
impl_typed_event!(ClientClose, "client.close");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientNotify {
  pub msg: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timeout: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subcolor: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub fcolor: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub color: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub bgcolor: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub icon: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub subicon: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub header: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub classes: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub buttons: Option<Vec<Value>>,
}
impl_typed_event!(ClientNotify, "client.notify");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRoomPlayers(pub Vec<Value>);
impl_typed_event!(ClientRoomPlayers, "client.room.players");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRoomJoin(pub Value);
impl_typed_event!(ClientRoomJoin, "client.room.join");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameStart {
  pub multi: bool,
  pub ft: u32,
  pub wb: u32,
  pub players: Vec<ClientGamePlayer>,
}
impl_typed_event!(ClientGameStart, "client.game.start");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGamePlayer {
  pub id: String,
  pub name: String,
  pub points: u32,
}

/// Fires when a round starts. Contains a `TickSetter` so the user can register
/// a per-frame tick callback, and an `Arc<Mutex<Engine>>` for read access.
#[derive(Clone)]
pub struct ClientGameRoundStart {
  pub setter: TickSetter,
  pub engine: Arc<Mutex<Engine>>,
}
impl_typed_event!(ClientGameRoundStart, "client.game.round.start");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameOver {
  pub reason: GameOverKind,
  pub data: Option<GameReplayEndData>,
}
impl_typed_event!(ClientGameOver, "client.game.over");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameOverKind {
  Finish,
  Abort,
  End,
  Leave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameRoundEnd(pub Option<String>);
impl_typed_event!(ClientGameRoundEnd, "client.game.round.end");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameEnd {
  pub duration: f64,
  pub source: String,
  pub players: Vec<ClientGameEndPlayer>,
}
impl_typed_event!(ClientGameEnd, "client.game.end");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameEndPlayer {
  pub id: String,
  pub name: String,
  pub points: Option<u32>,
  pub won: bool,
  pub lifetime: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameAbort;
impl_typed_event!(ClientGameAbort, "client.game.abort");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRibbonReceive {
  pub command: String,
  #[serde(default)]
  pub data: Value,
}
impl_typed_event!(ClientRibbonReceive, "client.ribbon.receive");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRibbonSend {
  pub command: String,
  #[serde(default)]
  pub data: Value,
}
impl_typed_event!(ClientRibbonSend, "client.ribbon.send");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientFriended {
  pub id: String,
  pub name: String,
  pub avatar: Option<u64>,
}
impl_typed_event!(ClientFriended, "client.friended");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDm {
  pub relationship: Value,
  pub raw: SocialDm,
  pub content: String,
}
impl_typed_event!(ClientDm, "client.dm");

// ── Game events (inbound from ribbon server) ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReadyPlayer {
  pub gameid: u32,
  pub userid: String,
  pub options: crate::types::game::Options,
  pub alive: bool,
  pub naturalorder: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReady {
  pub players: Vec<GameReadyPlayer>,
  #[serde(rename = "isNew", default)]
  pub is_new: bool,
}
impl_typed_event!(GameReady, "game.ready");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameAbort;
impl_typed_event!(GameAbort, "game.abort");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMatch {
  pub gamemode: String,
  pub modename: String,
  pub rb: Value,
  pub rrb: Value,
}
impl_typed_event!(GameMatch, "game.match");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStart;
impl_typed_event!(GameStart, "game.start");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameAdvance {
  pub scoreboard: Vec<Value>,
}
impl_typed_event!(GameAdvance, "game.advance");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameScore {
  pub scoreboard: Vec<Scoreboard>,
  #[serde(rename = "match", default)]
  pub match_data: Value,
}
impl_typed_event!(GameScore, "game.score");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEnd {
  pub leaderboard: Vec<Leaderboard>,
  pub scoreboard: Vec<Scoreboard>,
  #[serde(rename = "xpPerUser", default)]
  pub xp_per_user: f64,
  pub winners: Vec<Value>,
}
impl_typed_event!(GameEnd, "game.end");

/// Inbound `game.replay` — frames for a spectated player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayInbound {
  pub gameid: u32,
  pub provisioned: i32,
  pub frames: Vec<GameReplayFrame>,
}
impl_typed_event!(GameReplayInbound, "game.replay");

/// `game.replay.state` — snapshot of a player's engine state for spectating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayState {
  pub gameid: u32,
  pub data: GameReplayStateData,
}
impl_typed_event!(GameReplayState, "game.replay.state");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GameReplayStateData {
  Simple(String),
  State {
    frame: i32,
    game: Value,
    #[serde(default)]
    overrides: Value,
  },
}

/// A single protocol-level replay frame (key event, IGE, full state, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayFrame {
  #[serde(rename = "type")]
  pub kind: String,
  #[serde(default)]
  pub frame: i32,
  #[serde(default)]
  pub data: Value,
}

/// `game.replay.ige` — garbage events targeting the client's engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayIge {
  pub gameid: u32,
  pub iges: Vec<IgePayload>,
}
impl_typed_event!(GameReplayIge, "game.replay.ige");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgePayload {
  #[serde(rename = "type")]
  pub kind: String,
  #[serde(default)]
  pub data: Value,
}

/// `game.replay.board` — board states from spectated players.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayBoard {
  pub boards: Vec<Value>,
}
impl_typed_event!(GameReplayBoard, "game.replay.board");

/// `game.replay.end` — a player's game has ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayEnd {
  pub gameid: u32,
  pub data: GameReplayEndData,
}
impl_typed_event!(GameReplayEnd, "game.replay.end");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReplayEndData {
  pub gameoverreason: GameOverReason,
  pub killer: Value,
}

/// `game.spectate` — spectate info when joining a game in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSpectate {
  pub players: Vec<GameSpectatePlayer>,
  #[serde(rename = "match", default)]
  pub match_data: Value,
}
impl_typed_event!(GameSpectate, "game.spectate");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSpectatePlayer {
  pub userid: String,
  pub gameid: u32,
  pub alive: bool,
  pub naturalorder: i32,
  pub options: crate::types::game::Options,
}

/// Events that flow through the client's message-queue during gameplay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GameClientEvent {
  Garbage {
    frame: i32,
    amount: i32,
    size: usize,
    id: i32,
    column: usize,
  },
  Frameset {
    provisioned: i32,
    frames: Vec<GameReplayFrame>,
  },
}

// ── Room events (inbound) ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomJoin {
  pub id: String,
  pub banner: Value,
  pub silent: bool,
}
impl_typed_event!(RoomJoin, "room.join");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomLeave(pub String);
impl_typed_event!(RoomLeave, "room.leave");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomKick(pub String);
impl_typed_event!(RoomKick, "room.kick");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomUpdate {
  pub id: String,
  pub public: bool,
  pub name: String,
  pub name_safe: Option<String>,
  #[serde(rename = "type")]
  pub kind: String,
  pub owner: String,
  pub state: String,
  pub options: Value,
  #[serde(rename = "userLimit")]
  pub user_limit: u32,
  #[serde(rename = "autoStart")]
  pub auto_start: u32,
  pub players: Vec<Value>,
  #[serde(flatten)]
  pub extra: Value,
}
impl_typed_event!(RoomUpdate, "room.update");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomUpdateAuto {
  pub enabled: bool,
  pub status: String,
  pub time: i64,
  pub maxtime: i64,
}
impl_typed_event!(RoomUpdateAuto, "room.update.auto");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomPlayerAdd(pub Value);
impl_typed_event!(RoomPlayerAdd, "room.player.add");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomPlayerRemove(pub String);
impl_typed_event!(RoomPlayerRemove, "room.player.remove");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomUpdateHost(pub String);
impl_typed_event!(RoomUpdateHost, "room.update.host");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomUpdateBracket {
  pub uid: String,
  pub bracket: String,
}
impl_typed_event!(RoomUpdateBracket, "room.update.bracket");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomChat {
  pub content: String,
  pub content_safe: Option<String>,
  pub suppressable: Option<bool>,
  pub user: RoomChatUser,
  pub pinned: Option<bool>,
  pub system: bool,
}
impl_typed_event!(RoomChat, "room.chat");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomChatUser {
  pub username: String,
  #[serde(rename = "_id")]
  pub id: Option<String>,
  pub role: Option<Role>,
  pub supporter: Option<bool>,
  pub supporter_tier: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomChatDelete {
  pub uid: String,
  pub purge: String,
}
impl_typed_event!(RoomChatDelete, "room.chat.delete");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomChatClear;
impl_typed_event!(RoomChatClear, "room.chat.clear");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomChatGift {
  pub sender: u64,
  pub target: u64,
  pub months: u32,
}
impl_typed_event!(RoomChatGift, "room.chat.gift");

// ── Social events ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialOnline(pub u32);
impl_typed_event!(SocialOnline, "social.online");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialDmEvent {
  #[serde(flatten)]
  pub dm: Value,
}
impl_typed_event!(SocialDmEvent, "social.dm");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialDmFail(pub String);
impl_typed_event!(SocialDmFail, "social.dm.fail");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialPresence {
  pub user: String,
  pub presence: SocialPresenceData,
}
impl_typed_event!(SocialPresence, "social.presence");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialPresenceData {
  pub status: SocialStatus,
  pub detail: String,
  pub invitable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialRelationRemove(pub String);
impl_typed_event!(SocialRelationRemove, "social.relation.remove");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialRelationAdd {
  #[serde(rename = "_id")]
  pub id: String,
  pub from: Value,
  pub to: Value,
}
impl_typed_event!(SocialRelationAdd, "social.relation.add");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialNotificationEvent(pub SocialNotification);
impl_typed_event!(SocialNotificationEvent, "social.notification");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialInvite {
  pub sender: String,
  pub roomid: String,
  pub roomname: String,
  pub roomname_safe: String,
}
impl_typed_event!(SocialInvite, "social.invite");

// ── Ribbon / server events ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAuthorize {
  pub success: bool,
  pub maintenance: bool,
  pub worker: ServerAuthorizeWorker,
  pub social: Value,
}
impl_typed_event!(ServerAuthorize, "server.authorize");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAuthorizeWorker {
  pub name: String,
  pub flag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMigrate {
  pub endpoint: String,
  pub name: String,
  pub flag: String,
}
impl_typed_event!(ServerMigrate, "server.migrate");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMigrated(pub Value);
impl_typed_event!(ServerMigrated, "server.migrated");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAnnouncement {
  #[serde(rename = "type")]
  pub announcement_type: String,
  pub msg: String,
  pub ts: i64,
  pub reason: Option<String>,
}
impl_typed_event!(ServerAnnouncement, "server.announcement");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMaintenance(pub Value);
impl_typed_event!(ServerMaintenance, "server.maintenance");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibbonKick {
  pub reason: String,
}
impl_typed_event!(RibbonKick, "kick");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibbonNope {
  pub reason: String,
}
impl_typed_event!(RibbonNope, "nope");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibbonNotify {
  #[serde(rename = "type", default)]
  pub kind: String,
  pub msg: Option<String>,
}
impl_typed_event!(RibbonNotify, "notify");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffXrc(pub String);
impl_typed_event!(StaffXrc, "staff.xrc");
