# Game

This document covers the game loop, the tick function, targeting, in-game events (IGEs), and spectating opponents.

## Overview

When a game starts triangle-rs runs an internal 60 FPS async loop that:

1. Calls your **tick function** once per frame.
2. Converts any key presses returned by your tick function into replay frames.
3. Sends accumulated input every 12 frames to the ribbon server.
4. Applies received IGEs (garbage, etc.) from opponents to the local engine.

The loop starts automatically when `game.ready` is sent by the server. You register your game logic via `Game::on_round_start` **before** calling `client.start_room_game()`.

## Getting a `Game` handle

`client.game` is `Option<Game>` and is populated after `start_room_game()` or `spectate_room_game()`.

```rust
client.start_room_game().await?;
let game = client.game.as_ref().unwrap();
```

## Registering the tick function

```rust
game.on_round_start(Arc::new(|setter: TickSetter, engine: Arc<Mutex<Engine>>| {
    setter.set(|input: TickInput| async move {
        // Your logic here.
        TickOutput::default()
    });
}));
```

`on_round_start` accepts a `RoundStartHandler`:

```rust
type RoundStartHandler = Arc<dyn Fn(TickSetter, Arc<Mutex<Engine>>) + Send + Sync>;
```

The closure is called once per round. `setter.set(…)` installs the tick function for that round. You can re-install a different tick function mid-round by calling `setter.set(…)` again.

## `TickInput`

Delivered once per frame (60 times/second):

```rust
pub struct TickInput {
    pub gameid: u32,
    pub frame: i32,  // current engine frame number

    // Events that occurred since the last tick (garbage arrivals, framesets).
    pub events: Vec<GameClientEvent>,

    // The local engine (lock it briefly; do not hold across awaits).
    // NOTE: any mutations made inside the tick fn are reverted after it returns.
    pub engine: Arc<Mutex<Engine>>,

    // Targeting
    pub can_target: bool,
    pub server_targets: Vec<u32>,  // currently targeted game IDs from server
    pub enemies: Vec<u32>,         // all opponent game IDs
    /// Write a TargetStrategy here to change targeting behaviour.
    pub target: Arc<Mutex<TargetStrategy>>,

    // Garbage
    pub pause_iges: Arc<Mutex<bool>>,       // pause IGE application
    pub force_pause_iges: Arc<Mutex<bool>>, // stops IGE application until unset

    // Keypresses queued from previous ticks, not yet sent
    pub key_queue: Arc<Mutex<Vec<TickKeypress>>>,
}
```

## `GameClientEvent`

Events delivered in `input.events` each tick:

```rust
pub enum GameClientEvent {
    /// Incoming garbage attack arrived this tick.
    Garbage {
        frame: i32,
        amount: i32,
        size: usize,
        id: i32,
        column: usize,
    },
    /// A batch of replay frames was sent to the server (internal housekeeping).
    Frameset {
        provisioned: i32,
        frames: Vec<GameReplayFrame>,
    },
}
```

Check for incoming garbage:

```rust
let has_garbage = input.events.iter().any(|e| matches!(e, GameClientEvent::Garbage { .. }));
```

## `TickOutput`

Return this from your tick function:

```rust
pub struct TickOutput {
    /// Key events to send this frame.  None = no input.
    pub keys: Option<Vec<TickKeypress>>,
    /// Closures called after the frame is processed (useful for logging).
    pub run_after: Option<Vec<Arc<dyn Fn() + Send + Sync>>>,
}

impl Default for TickOutput { … }  // keys: None, run_after: None
```

## `TickKeypress`

Represents a single keydown or keyup event:

```rust
pub struct TickKeypress {
    pub kind: TickKeypressKind,
    pub frame: f64,
    pub data: TickKeypressData,
}

pub enum TickKeypressKind { Keydown, Keyup }

pub struct TickKeypressData {
    pub key: String,   // e.g. "moveLeft", "rotateCW", "hardDrop"
    pub subframe: f64, // 0.0–1.0 within the frame
}
```

### Key names

| Key string  | Action                   |
| ----------- | ------------------------ |
| `moveLeft`  | Move left                |
| `moveRight` | Move right               |
| `softDrop`  | Soft drop                |
| `hardDrop`  | Hard drop                |
| `rotateCCW` | Rotate counter-clockwise |
| `rotateCW`  | Rotate clockwise         |
| `rotate180` | Rotate 180°              |
| `hold`      | Hold piece               |

## Minimal tick function example

```rust
use triangle_rs::types::events::wrapper::{TickInput, TickOutput, TickKeypress, TickKeypressKind, TickKeypressData};

game.on_round_start(|setter, _engine| {
    setter.set(|input: TickInput| async move {
        if input.frame == 0 {
            // Hard drop on frame 0
            return TickOutput {
                keys: Some(vec![
                    TickKeypress {
                        kind: TickKeypressKind::Keydown,
                        frame: input.frame as f64,
                        data: TickKeypressData { key: "hardDrop".to_string(), subframe: 0.0 },
                    }
                ]),
                run_after: None,
            };
        }
        TickOutput::default()
    });
});
```

## Reading the engine from the tick function

```rust
use triangle_rs::engine::queue::types::Mino;

setter.set(|input: TickInput| async move {
    let eng = input.engine.lock().await;
    let board = &eng.board;         // Board { state: Vec<Vec<Option<Tile>>>, width, height, .. }
    let current_piece: Mino = eng.falling.symbol; // currently falling piece
    let next_pieces: &[Mino] = eng.queue.as_slice(); // upcoming pieces
    let held: Option<Mino> = eng.held;
    drop(eng); // release lock before any await

    TickOutput::default()
});
```

> **Note**: Mutations to the engine inside the tick function are rolled back after it returns. The engine state passed in is a read-only snapshot for decision making only.

> **Lock discipline**: Lock `engine` for the shortest possible window. Do **not** hold the lock across `.await` points — the tick loop locks engine separately.

## Targeting

Your tick function receives:

- `can_target` — whether the client is allowed to change targets.
- `server_targets` — the game IDs the server currently sees as the target.
- `enemies` — all live opponent game IDs.
- `target` — `Arc<Mutex<Option<u32>>>` — write here to change the local targeting intent.

```rust
use triangle_rs::classes::game::TargetStrategy;

// Target a specific game ID
*input.target.lock().await = TargetStrategy::Manual(5);

// Use an automatic strategy
*input.target.lock().await = TargetStrategy::Elims; // target lowest-health opponent
```

The server only accepts a targeting change if `can_target` is true; the library sends the change automatically when it detects a diff.

### `TargetStrategy`

When using `BotWrapper`, the strategy can be set automatically:

```rust
pub enum TargetStrategy {
    Even,       // distribute garbage evenly
    Elims,      // target lowest-health opponent
    Random,     // randomise each send
    Payback,    // target whoever last attacked you
    Manual(u32), // fixed game ID
}
```

## Garbage / IGEs

Incoming garbage is applied automatically by the game loop between frames. You can pause application:

```rust
// Pause (still collects garbage, applies later)
*input.pause_iges.lock().await = true;

// Hard pause (never apply until explicitly unset)
*input.force_pause_iges.lock().await = true;
```

## Game state (`Self_`)

`client.game.self_` is `Option<Self_>` and is set for the client's own engine.

```rust
pub struct Self_ {
    pub gameid: u32,
    pub engine: Arc<Mutex<Engine>>,
    pub options: GameOptions,
    pub can_target: bool,
    pub server_targets: Vec<u32>,
    pub enemies: Vec<u32>,
    pub target: Arc<Mutex<Option<u32>>>,
    pub pause_iges: Arc<Mutex<bool>>,
    pub force_pause_iges: Arc<Mutex<bool>>,
    pub key_queue: Arc<Mutex<Vec<TickKeypress>>>,
    // …
}
```

## Spectating opponents

You can read opponent board states in real time by spectating them.

```rust
// Spectate by game ID
game.spectate(vec![1, 2, 3]);

// Spectate all opponents
game.spectate_all();

// Spectate by user ID strings
game.spectate_userids(vec!["abc123".to_string()]);

// Stop spectating
game.unspectate(vec![1]);
game.unspectate_all();
game.unspectate_userids(vec!["abc123".to_string()]);
```

`game.players` — list of `Player` objects for all opponents.

```rust
pub struct Player {
    pub name: String,
    pub gameid: u32,
    pub userid: String,
    // engine is an Arc<Mutex<Engine>>, readable at any time
}
```

Read an opponent's board:

```rust
if let Some(player) = game.players.iter().find(|p| p.userid == "abc123") {
    let eng = player.engine.lock().await;
    let height = eng.board.highest_occupied_row();
    drop(eng);
}
```

### SpectatingStrategy

How opponent frames are consumed:

| Variant   | Behaviour                                               |
| --------- | ------------------------------------------------------- |
| `Instant` | All buffered frames run immediately each tick (default) |
| `Smooth`  | One frame per tick; if >20 frames behind, fast-forwards |

Change at runtime:

```rust
game.players[0].set_strategy(SpectatingStrategy::Smooth);
```

### SpectatingState

```rust
pub enum SpectatingState {
    Inactive,  // not spectating this player
    Waiting,   // request sent, waiting for server ack
    Active,    // receiving frames
}
```

## Opponents iterator

```rust
// Returns all players except the client's own ID.
let opponents = game.opponents(&client.user.id);
```

## Snapshot utilities (internal)

These are used internally but are public if you need them:

```rust
// Build an EngineSnapshot from a raw GameReplayState payload.
pub fn snapshot_from_state(frame: f64, init: &Options, state: &Value) -> EngineSnapshot

// Construct a fresh Engine from room options.
pub fn create_engine(options: &Options, gameid: u32, all_players: &[RawPlayer]) -> Engine
```

## Constants

| Name  | Value                            |
| ----- | -------------------------------- |
| `FPS` | `60.0`                           |
| `FPM` | `12` (frames per ribbon message) |
