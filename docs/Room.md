# Room

A `Room` represents a custom room on TETR.IO. `client.room` is `Option<Room>` and is set after joining or creating a room.

```rust
use triangle_rs::classes::room::Room;
use triangle_rs::types::room::{Player, State, Bracket, Match, Autostart, Preset, SetConfigItem};
```

## Joining and creating rooms

These methods are on `Client` and update `client.room` automatically:

```rust
// Join an existing room by its case-insensitive code.
pub async fn join_room(&mut self, code: &str) -> Result<Room>

// Create a new room.  Pass true for private (invite-only).
pub async fn create_room(&mut self, private: bool) -> Result<Room>

// Leave the current room.  Clears game state.
pub async fn leave_room(&mut self) -> Result<()>
```

Listing available public rooms:

```rust
let rooms: Vec<Value> = client.list_rooms().await?;
```

## `Room` fields

```rust
pub struct Room {
    pub id: String,
    pub public: bool,
    pub room_type: String,
    pub name: String,
    pub name_safe: String,
    pub owner: String,        // user ID of the room host
    pub creator: String,
    pub state: State,
    pub auto: Autostart,
    pub match_config: Match,
    pub players: Vec<Player>,
    pub user_limit: u32,
    pub allow_chat: bool,
    pub allow_anonymous: bool,
    pub allow_unranked: bool,
    pub allow_queued: bool,
    pub allow_bots: bool,
    pub user_rank_limit: Option<String>,
    pub use_best_rank_as_limit: bool,
    pub lobbybg: Option<String>,
    pub lobbybgm: Option<String>,
    pub gamebgm: Option<String>,
    pub force_require_xp_to_chat: bool,
    pub options: GameOptions,
    pub game_start: Option<Value>,
    pub chats: Vec<Value>,
}
```

### `State` enum

```rust
pub enum State { Lobby, IGO, Ended }
```

### `Player` (room player)

```rust
pub struct Player {
    pub id: String,
    pub username: String,
    pub anon: bool,
    pub bracket: Bracket,
    pub support: bool,
    pub role: Option<String>,
    pub bot: bool,
}

pub enum Bracket { Playing, Spectator }
```

### `Match` (match config)

```rust
pub struct Match {
    pub ft: u32,   // first-to (wins needed)
    pub wb: u32,   // win-by
    pub record: bool,
}
```

### `Autostart`

```rust
pub struct Autostart {
    pub time: u32,
    pub enabled: bool,
}
```

## Room methods

All methods require a reference to the `Client`:

### Host controls

```rust
// Transfer host to another player (by username or user ID).
pub async fn transfer_host(&self, client: &Client, player: &str) -> Result<Value>

// Reclaim host from another player.
pub async fn take_host(&self, client: &Client) -> Result<Value>

// Check if a user is the host.
pub fn is_host(&self, user_id: &str) -> bool
```

### Game controls

```rust
// Start the game (must be host).
pub async fn start(&self, client: &Client) -> Result<Value>

// Abort an ongoing game (must be host).
pub async fn abort(&self, client: &Client) -> Result<Value>

// Begin spectating the current game.
// Populates client.game with a spectator-mode Game.
pub async fn spectate(&self, client: &mut Client) -> Result<Value>

// Stop spectating.
pub fn unspectate(&self, client: &mut Client)
```

### Player management

```rust
// Kick a player for `duration` seconds.
pub async fn kick(&self, client: &Client, user_id: &str, duration: u32) -> Result<Value>

// Ban a player (kick for 30 days).
pub async fn ban(&self, client: &Client, user_id: &str) -> Result<Value>

// Unban a player by username.
pub fn unban(&self, client: &Client, username: &str)

// Switch your own bracket between Playing and Spectator.
pub async fn switch_bracket(&self, client: &Client, bracket: &str) -> Result<Value>
// bracket: "playing" | "spectator"
```

### Chat

```rust
// Send a chat message.  Set pinned = true to pin it.
pub async fn chat(&self, client: &Client, message: &str, pinned: bool) -> Result<Value>

// Clear all chat messages (must be host).
pub async fn clear_chat(&self, client: &Client) -> Result<Value>
```

### Room settings

```rust
// Change the room code.
pub async fn set_id(&self, client: &Client, id: &str) -> Result<Value>

// Apply a list of config changes directly.
pub async fn update(&self, client: &Client, options: &[SetConfigItem]) -> Result<Value>

// Apply a built-in preset.
pub async fn use_preset(&self, client: &Client, preset: Preset) -> Result<Value>
```

### Helper

```rust
// Get the own Player entry inside the room.
pub fn self_player(&self, user_id: &str) -> Option<&Player>
```

### Leaving

```rust
pub async fn leave(&self, client: &Client) -> Result<()>
```

## Presets

```rust
pub enum Preset {
    Default,
    TetraLeagueSeason1,
    TetraLeague,
    Classic,
    EnforcedDelays,
    Arcade,
}
```

Apply a preset:

```rust
room.use_preset(&client, Preset::TetraLeague).await?;
```

## Configuring room options

Use `SetConfigItem` for granular control:

```rust
pub struct SetConfigItem {
    pub index: String,  // dot-separated config path, e.g. "options.spinbonuses"
    pub value: Value,
}
```

Example — disable B2B:

```rust
room.update(&client, &[
    SetConfigItem {
        index: "options.b2b".to_string(),
        value: Value::Bool(false),
    },
]).await?;
```

Common option paths:

| Path                      | Type     | Description                                      |
| ------------------------- | -------- | ------------------------------------------------ |
| `options.spinbonuses`     | `String` | `"T-spins"`, `"all"`, `"none"`, …                |
| `options.garbageblocking` | `String` | `"combo"`, `"immediate"`, `"limited"`, `"none"`  |
| `options.garbagecap`      | `Number` | Maximum pending garbage                          |
| `options.garbagespeed`    | `Number` | Garbage tank delay                               |
| `options.garbageincrease` | `Number` | Garbage multiplier increase per margin           |
| `options.margintime`      | `Number` | Time before garbage multiplier starts (ms)       |
| `options.comboMinifier`   | `Number` | `0.0`–`1.0` combo damage reduction               |
| `options.clutch`          | `Bool`   | Allow clutch clears to reduce garbage            |
| `options.passthrough`     | `String` | `"minimal"`, `"no-cheese"`, `"garbage"`, `"all"` |
| `options.boardwidth`      | `Number` | Board width (default 10)                         |
| `options.boardheight`     | `Number` | Board height (default 20)                        |
| `options.hasgarbage`      | `Bool`   | Enable garbage                                   |
| `room.ft`                 | `Number` | First-to (wins to win match)                     |
| `room.wb`                 | `Number` | Win-by margin                                    |
| `room.userlimit`          | `Number` | Max players                                      |
| `room.allowchat`          | `Bool`   | Enable chat                                      |
| `room.allowbots`          | `Bool`   | Allow bots                                       |
| `room.allowunranked`      | `Bool`   | Allow unranked players                           |
| `room.usebranklimit`      | `Bool`   | Use best rank as rank limit                      |
| `room.userrankLimit`      | `String` | Rank ceiling (e.g., `"s+"`)                      |

## Snapshot

`room.snapshot()` returns a `serde_json::Value` with all room state — useful for logging or sending to a dashboard.

## Room update lifecycle

The library automatically calls `room.apply_update(data)` whenever a `room.update` event is received, keeping `client.room` in sync. You do not need to manage this manually.
