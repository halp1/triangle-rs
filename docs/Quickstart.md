# Quickstart

triangle-rs is an async Rust library for interfacing with [TETR.IO](https://tetr.io) as a bot client. It wraps the ribbon WebSocket protocol, the HTTP API, engine simulation, and social features.

## Requirements

- Rust 1.80+
- A TETR.IO account (or token)
- A tokio async runtime

## Installation

Add to `Cargo.toml`:

```toml
[dependencies]
triangle-rs = { path = "../triangle-rs" }   # or crates.io once published
tokio = { version = "1", features = ["full"] }
```

## Minimal example — join a room, play a game

```rust
use triangle_rs::{
    classes::client::{Client, ClientOptions, GameOptions},
    classes::game::SpectatingStrategy,
    types::events::wrapper::{ClientGameRoundStart, TickInput, TickOutput, TickKeypress, TickKeypressKind, TickKeypressData},
};

#[tokio::main]
async fn main() -> triangle_rs::Result<()> {
    let client = Client::create(ClientOptions::with_token("YOUR_TOKEN")).await?;

    println!("Logged in as {}", client.user.username);

    // Join a custom room by code
    let room = client.join_room("MYROOM").await?;
    println!("Joined room: {}", room.name);

    // Wait for a round to start, then register a tick function
    client.on("client.game.round.start", |data| {
        // Use the typed event system instead (see Events.md)
    });

    // Start the game (if host)
    client.start_room_game().await?;

    // Keep the process alive
    let _ = client.wait("client.dead").await;
    Ok(())
}
```

## Connecting with credentials

```rust
use triangle_rs::classes::client::{ClientOptions, TokenOrCredentials};

let options = ClientOptions {
    token: TokenOrCredentials::Credentials {
        username: "mybot".to_string(),
        password: "hunter2".to_string(),
    },
    game: None,
    user_agent: None,
    turnstile: None,
    social: None,
};

let client = Client::create(options).await?;
```

## Event-driven flow

Everything in triangle-rs is event-driven. The `Client` exposes `on` / `wait` wrappers over the internal `EventEmitter`. Use the strongly-typed event system for clean, safe code:

```rust
use triangle_rs::utils::events::TypedEvent;
use triangle_rs::types::events::wrapper::ClientReady;

// One-shot, typed
let ready: ClientReady = {
    let raw = client.wait(ClientReady::NAME).await.unwrap();
    serde_json::from_value(raw).unwrap()
};
println!("endpoint: {}", ready.endpoint);

// Persistent listener, typed
client.ribbon.emitter.on_typed::<ClientReady, _>(|ev| {
    println!("(re)connected to {}", ev.endpoint);
});
```

See [Events.md](Events.md) for the full event catalogue.

## Next steps

| Topic                                       | Document                   |
| ------------------------------------------- | -------------------------- |
| Client API                                  | [Client.md](Client.md)     |
| Playing a game (tick loop, keys, targeting) | [Game.md](Game.md)         |
| Engine internals                            | [Engine.md](Engine.md)     |
| Bot framework (BotWrapper / Adapter)        | [Adapters.md](Adapters.md) |
| Room management                             | [Room.md](Room.md)         |
| Social (friends, DMs, notifications)        | [Social.md](Social.md)     |
| All typed events                            | [Events.md](Events.md)     |
