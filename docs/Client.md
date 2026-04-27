# Client

`Client` is the main entry point. It manages the ribbon WebSocket connection, authentication, room state, game state, and social features.

```rust
use triangle_rs::classes::client::{Client, ClientOptions};
```

## Creating a client

```rust
pub async fn Client::create(options: ClientOptions) -> Result<Client>
```

Connects to TETR.IO, authenticates, and waits for the ribbon handshake (`client.ready`) before returning. Fails if authentication or the connection handshake fails within 15 seconds.

### `ClientOptions`

```rust
pub struct ClientOptions {
    /// Authentication — token string or username/password pair.
    pub token: TokenOrCredentials,
    /// Optional game-specific settings (handling, spectating strategy).
    pub game: Option<GameOptions>,
    /// Override the default User-Agent string.
    pub user_agent: Option<String>,
    /// Cloudflare Turnstile clearance cookie value, if required.
    pub turnstile: Option<String>,
    /// Social module configuration.
    pub social: Option<SocialConfig>,
}
```

Shorthand constructor:

```rust
ClientOptions::with_token("TOKEN_STRING")
```

### `TokenOrCredentials`

```rust
pub enum TokenOrCredentials {
    Token(String),
    Credentials { username: String, password: String },
}
```

### `GameOptions` (client-level)

```rust
pub struct GameOptions {
    /// DAS / ARR / SDF etc.  Defaults to TETR.IO defaults.
    pub handling: Option<Handling>,
    /// How opponent replay frames are processed.  Defaults to Instant.
    pub spectating_strategy: Option<SpectatingStrategy>,
}
```

## Public fields

```rust
pub struct Client {
    pub user: ClientUser,          // logged-in user info
    pub disconnected: bool,
    pub token: String,
    pub ribbon: Ribbon,            // underlying WebSocket wrapper
    pub social: Option<Social>,    // social module (friends, DMs)
    pub room: Option<Room>,        // current room (if any)
    pub game: Option<Game>,        // active game state (if any)
    pub api: Arc<Api>,             // HTTP API client
}
```

### `ClientUser`

```rust
pub struct ClientUser {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub session_id: String,
    pub user_agent: String,
}
```

## Event listeners

```rust
// Register a persistent callback — returns a JoinHandle; drop it to cancel.
pub fn on<F>(&self, event: &str, callback: F) -> JoinHandle<()>
where F: Fn(Value) + Send + 'static

// Await the next single occurrence of an event.
pub async fn wait(&self, event: &str) -> Option<Value>
```

For strongly-typed events use `ribbon.emitter` directly — see [Events.md](Events.md).

## Sending raw ribbon commands

```rust
// Fire-and-forget — sends a command to the ribbon server.
pub fn emit(&self, command: &str, data: Value)

// Send a command, then await a specific response event.
// Returns the response payload or a Ribbon error.
pub async fn wrap(&self, command: &str, data: Value, listen_for: &str) -> Result<Value>

// Same, but you can also specify which events are treated as errors.
pub async fn wrap_with_errors(
    &self,
    command: &str,
    data: Value,
    listen_for: &str,
    error_events: &[&str],
) -> Result<Value>
```

## Handling settings

```rust
pub fn handling(&self) -> &Handling

// Fails if the client is currently inside a room.
pub fn set_handling(&mut self, handling: Handling) -> Result<()>
```

`Handling` fields:

| Field      | Type     | Description                          |
| ---------- | -------- | ------------------------------------ |
| `arr`      | `f64`    | Auto-repeat rate (ms)                |
| `das`      | `f64`    | Delayed auto-shift (ms)              |
| `dcd`      | `f64`    | DAS cut delay (ms)                   |
| `sdf`      | `f64`    | Soft-drop factor (`41.0` = instant)  |
| `safelock` | `bool`   | Prevent accidental hard drops        |
| `cancel`   | `bool`   | Cancel DAS on direction change       |
| `may20g`   | `bool`   | Allow 20G on soft-drop               |
| `irs`      | `String` | IRS mode: `"off"`, `"hold"`, `"tap"` |
| `ihs`      | `String` | IHS mode: `"off"`, `"hold"`          |

## Spectating strategy

```rust
pub fn spectating_strategy(&self) -> &SpectatingStrategy
pub fn set_spectating_strategy(&mut self, strategy: SpectatingStrategy)
```

```rust
pub enum SpectatingStrategy {
    Smooth,   // process one frame per tick; catch up if >20 frames behind
    Instant,  // process all buffered frames immediately every tick
}
```

## Room management

```rust
pub async fn list_rooms(&self) -> Result<Vec<Value>>
pub async fn join_room(&mut self, code: &str) -> Result<Room>
pub async fn create_room(&mut self, private: bool) -> Result<Room>
pub async fn leave_room(&mut self) -> Result<()>
```

## Game lifecycle

```rust
// Sends room.start, waits for game.ready, initialises Game state.
// Only call this if the bot is the room host.
// After this returns, client.game is Some(game).
pub async fn start_room_game(&mut self) -> Result<()>

// Spectate an ongoing game inside the current room.
pub async fn spectate_room_game(&mut self) -> Result<Value>

// Stop spectating and clear game state.
pub fn unspectate_game(&mut self)
```

> **Important**: `client.game` is `None` after `join_room()`. It is only set by `start_room_game()` or `spectate_room_game()`. Do **not** call `client.game.as_ref().unwrap()` before one of those methods succeeds.

### Non-host game flow

When the bot is **not** the room host, do not call `start_room_game()`. Instead wait for the host to start the game, then construct `Game` manually:

```rust
use triangle::classes::game::{Game, SpectatingStrategy, parse_raw_players};

// 1. Switch to the playing bracket.
if let Some(room) = &client.room {
    room.switch_bracket(&client, "playing").await?;
}

// 2. Wait for the host's game.ready broadcast.
let game_ready_data = client.wait("game.ready").await.expect("ribbon closed");

// 3. Build the Game struct from the server payload.
let players   = parse_raw_players(&game_ready_data);
let send_fn   = client.ribbon.make_send_fn();
let game      = Game::new(
    client.ribbon.emitter.clone(),
    players,
    &client.user.id,
    SpectatingStrategy::Instant,
    send_fn,
);
client.game = Some(game);

// 4. Register round-start handler NOW (before game.start fires).
client.game.as_ref().unwrap().on_round_start(|setter, _engine| {
    setter.set(|input| async move {
        // your tick logic
        triangle::types::events::wrapper::TickOutput::default()
    });
});

// 5. Keep alive.
let _ = client.wait("client.dead").await;
```

## Reconnection

```rust
pub async fn reconnect(&mut self) -> Result<()>
```

Only valid when `client.disconnected == true`. Re-connects to the ribbon using the same token and handling settings. Room / game state is **not** restored automatically.

## HTTP API

`client.api` exposes the full HTTP API client. See [the API section](Social.md#http-api) for user lookups, DMs, relationship management, and replays.

## Errors

```rust
pub enum TriangleError {
    Http(reqwest::Error),
    WebSocket(tungstenite::Error),
    Json(serde_json::Error),
    InvalidToken,
    Api(String),
    Connection(String),
    Engine(String),
    Adapter(String),
    Channel(String),
    Ribbon(String),
    InvalidArgument(String),
    // …
}

pub type Result<T> = std::result::Result<T, TriangleError>;
```
