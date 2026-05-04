use std::env;

use triangle::{
  Client, ClientOptions, Credentials,
  classes::{
    client::RibbonOptions,
    ribbon::{self},
  },
  types::events::recv,
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

  let invite = client.wait::<recv::social::Invite>().await;

  println!("Received invite: {:?}", invite);
}
