# Social & HTTP API

## Social module

`client.social` is `Option<Social>` and is populated when `ClientOptions::social` is set (or when the server sends social init data on connection).

```rust
use triangle_rs::classes::social::{Social, RelationshipEntry, RelationshipLookup};
```

### `Social` fields

```rust
pub struct Social {
    pub online: u64,                       // currently online user count (server-reported)
    pub friends: Vec<RelationshipEntry>,
    pub other: Vec<RelationshipEntry>,     // followers / following
    pub blocked: Vec<RelationshipEntry>,
    pub notifications: Vec<Notification>,
    pub config: Option<Config>,            // social settings
}
```

### `RelationshipEntry`

```rust
pub struct RelationshipEntry {
    pub id: String,
    pub relationship_id: String,
    pub username: String,
    pub avatar: Option<u64>,
}
```

### `RelationshipLookup`

Used to search `friends`, `other`, and `blocked`:

```rust
pub enum RelationshipLookup {
    Id,       // match by user ID
    Username, // match by username
    Any,      // match by either
}
```

### Methods

```rust
// Mark all pending notifications as read.
pub fn mark_notifications_as_read(&self, client: &Client)
```

---

## Social types (`types::social`)

### `Notification`

```rust
pub struct Notification {
    pub id: String,
    pub type_: String,
    pub data: Value,
    pub seen: bool,
    pub ts: DateTime<Utc>,
}
```

### `Dm` and `DmMessage`

```rust
pub struct Dm {
    pub id: String,
    pub name: Option<String>,
    pub messages: Vec<DmMessage>,
    pub users: Vec<DmUser>,
}

pub struct DmMessage {
    pub id: String,
    pub content: String,
    pub author: String,
    pub ts: DateTime<Utc>,
    pub system: bool,
}

pub struct DmUser {
    pub id: String,
    pub username: String,
    pub avatar: Option<u64>,
    pub role: Option<String>,
}
```

### `Relationship`

```rust
pub struct Relationship {
    pub id: String,
    pub type_: String,
    pub from: String,
    pub to: String,
    pub mutual: bool,
}
```

### `Status`

```rust
pub struct Status {
    pub user_id: String,
    pub status: String,    // "online", "away", "offline", "playing", …
    pub detail: String,
    pub game: Option<Value>,
}
```

### Social `Config`

```rust
pub struct Config {
    pub show_invites: bool,
    pub allow_dms: bool,
    pub notify_friend_online: bool,
}
```

---

## HTTP API

`client.api` is `Arc<Api>` and exposes all TETR.IO REST endpoints.

```rust
use triangle_rs::utils::api::Api;
```

### Authentication

```rust
// Authenticate with username + password.  Returns a token.
pub async fn authenticate(&self, username: &str, password: &str) -> Result<AuthResult>

pub struct AuthResult {
    pub token: String,
    pub user_id: String,
}
```

### User lookups

```rust
// Get the currently-authenticated user's full profile.
pub async fn me(&self) -> Result<Me>

// Fetch a user by their ID or username.
pub async fn get_user(&self, id_or_name: &str) -> Result<User>

// Resolve a username to a User (case-insensitive).
pub async fn resolve_user(&self, username: &str) -> Result<User>

// Return true if the username exists.
pub async fn user_exists(&self, username: &str) -> Result<bool>
```

### `Me` and `User`

```rust
pub struct Me {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub email: Option<String>,
    pub ts: Option<DateTime<Utc>>,
    pub avatar: Option<u64>,
    pub banner: Option<u64>,
    pub badges: Vec<Badge>,
    pub xp: f64,
    pub friend_count: u32,
    pub league: League,
    pub records: Records,
    // … additional fields
}

pub struct User {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub ts: Option<DateTime<Utc>>,
    pub avatar: Option<u64>,
    pub banner: Option<u64>,
    pub badges: Vec<Badge>,
    pub xp: f64,
    pub friend_count: u32,
    pub league: League,
    pub records: Records,
}

pub struct League {
    pub gamesplayed: u32,
    pub gameswon: u32,
    pub rating: f64,           // TR (Tetra Rating)
    pub glicko: Option<f64>,
    pub rd: Option<f64>,
    pub rank: Rank,
    pub bestrank: Rank,
    pub apm: Option<f64>,
    pub pps: Option<f64>,
    pub vs: Option<f64>,
    pub standing: Option<i64>,
    pub percentile: Option<f64>,
    pub percentile_rank: Option<Rank>,
}

pub enum Rank {
    D, DPlus, CMinus, C, CPlus, BMinus, B, BPlus, AMinus, A, APlus,
    SMinus, S, SPlus, SS, U, X, XPlus, Z,
}

pub struct Badge {
    pub id: String,
    pub label: String,
    pub ts: Option<DateTime<Utc>>,
}

pub struct Records {
    pub sprint: Option<Value>,
    pub blitz: Option<Value>,
}

pub enum Role { User, Moderator, Admin, Sysop, Bot, Banned, Anon }
```

### Infrastructure

```rust
// Fetch environment metadata (spool list, signature, etc.).
pub async fn environment(&self) -> Result<Environment>

pub struct Environment {
    pub signature: Value,
    pub spools: Vec<SpoolResult>,
}

// Fetch the spool (WebSocket server endpoint).
pub async fn spool(&self) -> Result<SpoolResult>

pub struct SpoolResult {
    pub endpoint: String,
}
```

### Rooms

```rust
// List all public custom rooms.
pub async fn list_rooms(&self) -> Result<Vec<Value>>
```

### Relationships

```rust
// Block a user by their ID.
pub async fn block_user(&self, id: &str) -> Result<()>

// Remove a relationship (unfollow / unfriend / unblock).
pub async fn remove_relationship(&self, id: &str) -> Result<()>

// Follow / friend a user.
pub async fn friend_user(&self, id: &str) -> Result<()>
```

### Direct messages

```rust
// Fetch the DM history with a user.
pub async fn dms(&self, user_id: &str) -> Result<Vec<Dm>>
```

### Replays

```rust
// Download a game replay by ID.
pub async fn replay(&self, replay_id: &str) -> Result<Value>
```

---

## Social event listeners

Listen for real-time social events on `client.ribbon.emitter`:

```rust
use triangle_rs::types::events::wrapper::{ClientFriended, ClientDm};
use triangle_rs::utils::events::TypedEvent;

// Someone friended the bot
client.ribbon.emitter.on_typed::<ClientFriended, _>(|ev| {
    println!("{} followed us", ev.name);
});

// Incoming DM
client.ribbon.emitter.on_typed::<ClientDm, _>(|ev| {
    println!("Message from DM: {}", ev.content);
});
```

See [Events.md](Events.md) for the full event catalogue.
