# Adapters & BotWrapper

triangle-rs ships a lightweight AI/bot integration framework that lets any external solver drive the engine through a well-defined interface.

Two abstractions are involved:

| Type               | Role                                                                              |
| ------------------ | --------------------------------------------------------------------------------- |
| `Adapter<T>` trait | Your bot logic (in-process or via subprocess)                                     |
| `BotWrapper<A, T>` | Timing + pacing layer — converts adapter moves to replay frames at the target PPS |

## `Adapter<T>` trait

```rust
use triangle_rs::utils::adapters::Adapter;
```

```rust
pub trait Adapter<T> {
    /// Called once when the game begins.
    fn initialize(&mut self) -> Result<()>;

    /// Send engine configuration to the bot.
    fn config(&mut self, engine: &Engine, data: &T) -> Result<()>;

    /// Push a new engine state to the bot.
    fn update(&mut self, engine: &Engine, data: &T) -> Result<()>;

    /// Tell the bot about new pieces in the queue.
    fn add_pieces(&mut self, pieces: &[Mino], data: &T) -> Result<()>;

    /// Request the bot's next move.  Returns an `AdapterMove<T>`.
    fn play(&mut self, engine: &Engine, data: &T) -> Result<AdapterMove<T>>;

    /// Called when the game ends.
    fn stop(&mut self) -> Result<()>;
}
```

`T` is an arbitrary shared-data type you pass through every call (e.g., game metadata, configuration overrides).

### `AdapterMove<T>`

```rust
pub struct AdapterMove<T> {
    pub keys: Vec<AdapterKey>,
    pub data: T,
}
```

### `AdapterKey`

Represents a single action the bot wants to perform:

```rust
pub enum AdapterKey {
    MoveLeft,
    MoveRight,
    DasLeft,
    DasRight,
    SoftDrop,
    HardDrop,
    RotateCcw,
    RotateCw,
    Rotate180,
    Hold,
}
```

---

## `AdapterIo<T>` — subprocess adapter

`AdapterIo` launches an external bot process and communicates over stdin/stdout using newline-delimited JSON (NDJSON).

```rust
use triangle_rs::utils::adapters::{AdapterIo, AdapterIoConfig};

let config = AdapterIoConfig::new("/path/to/bot/binary")
    .name("my-bot")           // display name (optional)
    .verbose(false)           // log all messages (optional)
    .env("BOT_LEVEL", "hard") // environment variable (optional)
    .args(vec!["--mode", "vs"]); // command-line args (optional)

let adapter: AdapterIo<()> = AdapterIo::new(config);
```

### `AdapterIoConfig`

```rust
pub struct AdapterIoConfig {
    pub path: String,
    pub name: Option<String>,
    pub verbose: bool,
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

impl AdapterIoConfig {
    pub fn new(path: impl Into<String>) -> Self
    pub fn name(mut self, name: impl Into<String>) -> Self
    pub fn verbose(mut self, verbose: bool) -> Self
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self
    pub fn args(mut self, args: Vec<&str>) -> Self
}
```

### Protocol

The subprocess communicates via NDJSON on stdin/stdout. Each line is a JSON object with a `"type"` field. The library sends:

| Type          | Payload description                     |
| ------------- | --------------------------------------- |
| `"start"`     | Game is starting (`initialize`)         |
| `"config"`    | Engine configuration object (`config`)  |
| `"update"`    | Current engine state (`update`)         |
| `"new_piece"` | Array of new piece names (`add_pieces`) |
| `"play"`      | Request for next action (`play`)        |
| `"stop"`      | Game ended                              |

The subprocess responds with one JSON object per line when a `play` request is received:

```json
{ "move": ["moveLeft", "rotateCW", "hardDrop"] }
```

### Convenience helpers

These build the JSON payloads used by `config`, `update`, and `play`:

```rust
use triangle_rs::utils::adapters::{config_from_engine, state_from_engine, play_from_engine};

let config_payload = config_from_engine(&engine, &data);
let state_payload  = state_from_engine(&engine, &data);
let play_payload   = play_from_engine(&engine, &data);
```

---

## `BotWrapper<A, T>`

`BotWrapper` wraps an `Adapter` and handles pacing: it ensures the bot plays at a target pieces-per-second rate and generates the correct `ReplayFrame` sequence for the engine.

```rust
use triangle_rs::utils::bot_wrapper::{BotWrapper, BotWrapperConfig};

let config = BotWrapperConfig { pps: 3.0 }; // 3 pieces per second
let mut wrapper = BotWrapper::new(adapter, config);
```

### `BotWrapperConfig`

```rust
pub struct BotWrapperConfig {
    pub pps: f64,  // target pieces per second
}
```

### Methods

```rust
// Initialise the bot for a new game.
// Requires ARR = 0.0 and SDF = 41.0 in the engine's handling settings.
pub fn init(&mut self, engine: &Engine, data: &T) -> Result<()>

// Called each tick.  Returns replay frames to feed into Engine::tick().
// has_garbage_event: true if a garbage attack arrived this frame.
pub fn tick(&mut self, engine: &Engine, has_garbage_event: bool, data: &T) -> Result<Vec<ReplayFrame>>

// Shut down the bot process / free resources.
pub fn stop(&mut self) -> Result<()>
```

### Utility functions

```rust
// Calculate the exact frame at which the next piece should be played.
pub fn next_frame(engine: &Engine, target: f64) -> f64

// Convert a sequence of AdapterKey values into ReplayFrames at the given engine frame.
pub fn frames(engine: &Engine, keys: Vec<AdapterKey>) -> Vec<ReplayFrame>
```

---

## End-to-end example

Below is a complete example tying together `Client`, `BotWrapper`, and `AdapterIo`:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use triangle_rs::{
    classes::client::{Client, ClientOptions},
    utils::{
        adapters::{AdapterIo, AdapterIoConfig},
        bot_wrapper::{BotWrapper, BotWrapperConfig},
    },
};

#[tokio::main]
async fn main() -> triangle_rs::Result<()> {
    let mut client = Client::create(ClientOptions::with_token("TOKEN")).await?;

    let adapter = AdapterIo::<()>::new(AdapterIoConfig::new("./mybot"));
    let wrapper = Arc::new(Mutex::new(BotWrapper::new(adapter, BotWrapperConfig { pps: 2.5 })));

    client.join_room("ROOM").await?;

    // start_room_game() populates client.game — register on_round_start after it returns.
    client.start_room_game().await?;

    let game_wrapper = wrapper.clone();
    client.game.as_ref().unwrap().on_round_start(move |setter, engine| {
        let wrapper = game_wrapper.clone();
        setter.set(move |input| {
            let wrapper = wrapper.clone();
            async move {
                let eng = input.engine.lock().await;
                // Check for garbage via input.events
                let has_garbage = input.events.iter().any(|e| {
                    matches!(e, triangle_rs::types::events::wrapper::GameClientEvent::Garbage { .. })
                });

                let mut w = wrapper.lock().await;
                let _frames = w.tick(&eng, has_garbage, &()).unwrap_or_default();
                drop(eng);

                triangle_rs::types::events::wrapper::TickOutput::default()
            }
        });
    });

    let _ = client.wait("client.dead").await;
    Ok(())
}
```

> **Note**: `BotWrapper` is typically used inside the adapter pattern where `ReplayFrame`s are fed back directly to the game loop. For simpler bots, returning `TickOutput { keys: Some(your_keys), .. }` from the tick function is sufficient without `BotWrapper`.

---

## Handling requirements for `BotWrapper::init`

`BotWrapper` validates that:

- `handling.arr == 0.0`
- `handling.sdf == 41.0` (instant soft-drop)

If either condition is not met, `init()` returns `Err(TriangleError::Adapter)`. Set these in `ClientOptions::game.handling` before joining a room:

```rust
use triangle_rs::types::game::Handling;

let options = ClientOptions {
    token: TokenOrCredentials::Token("TOKEN".to_string()),
    game: Some(GameOptions {
        handling: Some(Handling {
            arr: 0.0,
            sdf: 41.0,
            das: 133.0,
            ..Default::default()
        }),
        spectating_strategy: None,
    }),
    ..Default::default()
};
```
