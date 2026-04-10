use triangle::classes::{Client, ClientOptions};

/// Equivalent of `triangle.js/test/client.test.ts`:
///
/// ```typescript
/// test("Client connect", async () => {
///   const client = await Client.create({ token: process.env.TOKEN! });
///   expect(client.user).toBeDefined();
/// });
/// ```
///
/// Run with:
///   TOKEN=<your_token> cargo test --test client_test
#[tokio::test]
async fn client_connect() {
  dotenvy::dotenv().ok();
  let token = std::env::var("TOKEN").expect("TOKEN environment variable must be set");
  let client = Client::create(ClientOptions::with_token(token))
    .await
    .expect("Client::create should succeed");

  assert!(
    !client.user.id.is_empty(),
    "client.user.id must be non-empty"
  );
  assert!(
    !client.user.username.is_empty(),
    "client.user.username must be non-empty"
  );

  client.ribbon.destroy();
}
