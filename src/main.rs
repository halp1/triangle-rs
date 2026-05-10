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
  },
};

#[tokio::main]
async fn main() {
  dotenvy::dotenv().ok();

  let mut client = Client::new(ClientOptions {
    token: Credentials::Token(env::var("TOKEN").expect("TOKEN env var not set")),
    game: None,
    user_agent: None,
    social: None,
    ribbon: Some(RibbonOptions {
      options: Some(ribbon::Options {
        debug: true,
        logging: ribbon::LoggingLevel::All,
        spooling: true,
      }),
      handling: None,
      transport: None,
      user_agent: None,
    }),
  })
  .await
  .expect("Failed to create client");

  println!("Client created: {:?}", client.user);

  let invite = client
    .wait::<recv::social::Invite>()
    .await
    .expect("Failed to receive invite");

  println!("Received invite: {:?}", invite);

  client
    .join_room(&invite.roomid)
    .await
    .expect("Failed to join room");

  println!(
    "Joined room {}, waiting for game to start...",
    invite.roomid
  );

  loop {
    let start = client
      .ribbon
      .wait::<recv::client::game::round::Start>()
      .await
      .expect("Failed to receive game start event");

    start
      .ticker
      .inject(|input| {
        Box::pin(async move {
          println!("Ticker input: {:?}", input);
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
      .await;
  }
}
