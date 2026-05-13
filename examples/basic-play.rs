use std::env;

use triangle::{
  Client, ClientOptions, Credentials,
  classes::{
    client::RibbonOptions,
    ribbon::{self},
  },
  types::{
    events::recv,
    game::{Key, tick},
    room::Bracket,
  },
};

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

  client
    .room()
    .await
    .unwrap()
    .switch(Bracket::Player)
    .await
    .ok();

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

    client
      .register_ticker(|input| {
        Box::pin(async move {
          tick::Out {
            keys: if input.engine.frame % 60 == 0 {
              vec![
                tick::Keypress {
                  r#type: tick::KeypressType::Keydown,
                  frame: input.engine.frame,
                  data: tick::KeypressData {
                    key: Key::HardDrop,
                    subframe: 0.0,
                    hoisted: false,
                  },
                },
                tick::Keypress {
                  r#type: tick::KeypressType::Keyup,
                  frame: input.engine.frame,
                  data: tick::KeypressData {
                    key: Key::HardDrop,
                    subframe: 0.0,
                    hoisted: false,
                  },
                },
              ]
            } else {
              vec![]
            },
            run_after: vec![],
          }
        })
      })
      .await
      .ok();
  }
}
