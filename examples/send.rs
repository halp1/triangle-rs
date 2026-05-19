use triangle::{Client, ClientOptions};

#[tokio::main]
async fn main() {
  dotenvy::dotenv().ok();
  let client = tokio::spawn(async {
    Client::new(ClientOptions::with_token(
      std::env::var("TOKEN").expect("TOKEN env var not set"),
    ))
    .await
    .expect("Failed to create client")
  })
  .await
  .unwrap();

  client.destroy().await;
}
