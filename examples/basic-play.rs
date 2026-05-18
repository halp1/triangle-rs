use core::panic;
use std::env;
use triangle::{
  Client, ClientOptions, Credentials,
  classes::{
    client::RibbonOptions,
    ribbon::{self},
  },
  types::{events::recv, game::tick, room::Bracket},
};

// could be useful
// struct FrameCounter(f64);
// impl FrameCounter {
//   pub fn new(v: u64) -> Self {
//     Self(v as f64)
//   }

//   pub fn add(&mut self, delta: f64) {
//     self.0 = ((self.0 + delta) * 10.0).round() / 10.0;
//   }

//   pub fn frame(&self) -> u64 {
//     self.0.floor() as u64
//   }

//   pub fn subframe(&self) -> f64 {
//     ((self.0 - self.0.floor()) * 10.0).round() / 10.0
//   }
// }

#[tokio::main]
async fn main() {
  dotenvy::dotenv().ok();

  tracing_subscriber::fmt::init();

  tracing::info!("Starting client...");

  let client = Client::new(ClientOptions {
    token: Credentials::Token(env::var("TOKEN").expect("TOKEN env var not set")),
    game: None,
    user_agent: None,
    social: None,
    ribbon: Some(RibbonOptions {
      options: Some(ribbon::Options {
        debug: true,
        logging: ribbon::LoggingLevel::Error,
        spooling: true,
      }),
      handling: None,
      transport: None,
      user_agent: None,
    }),
  })
  .await
  .expect("Failed to create client");

  tracing::info!("Client created: {:?}", client.user);

  let c = client.clone();
  tokio::select! {
    _ = async move {
    let client = c.clone();

    client
      .ribbon
      .on::<recv::client::DM>(async move |dm| {
        tracing::info!("Received DM from {}: {}", dm.username, dm.content);
      })
      .await;

    let invite = client
      .wait::<recv::social::Invite>()
      .await
      .expect("Failed to receive invite");

    tracing::info!("Received invite: {:?}", invite);

    client
      .join_room(&invite.roomid)
      .await
      .expect("Failed to join room");

    client.room().unwrap().switch(Bracket::Player).await.ok();

    tracing::info!(
      "Joined room {}, waiting for game to start...",
      invite.roomid
    );

    client
      .on::<recv::client::Dead>(|_| async {
        panic!("Connection closed permanently");
      })
      .await;

    loop {
      client
        .ribbon
        .wait::<recv::client::game::round::Start>()
        .await
        .expect("Failed to receive game start event");

      // start engine

      client
        .register_ticker(move |_input| {
          Box::pin(async move {
            tick::Out {
              keys: vec![],
              run_after: vec![]
            }
          })
        })
        .await
        .ok();
    }
  } => {}
    _ = tokio::signal::ctrl_c() => {}
  }

  tracing::warn!("Shutting down...");
  client.destroy().await;
}
