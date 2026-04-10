pub mod api;
pub mod events;

pub use api::Api;
pub use events::EventEmitter;

pub const VERSION: &str = "4.2.0";
pub const USER_AGENT: &str = concat!("Triangle.rs/4.2.0 (+https://triangle.haelp.dev)");

/// Decode the `sub` claim from a JWT token without verifying the signature.
pub fn parse_token(token: &str) -> crate::error::Result<String> {
  let parts: Vec<&str> = token.split('.').collect();
  if parts.len() < 2 {
    return Err(crate::error::TriangleError::Api(
      "invalid JWT: expected at least 2 segments".to_string(),
    ));
  }
  let b64 = parts[1].replace('-', "+").replace('_', "/");
  let pad = match b64.len() % 4 {
    0 => "".to_string(),
    2 => "==".to_string(),
    3 => "=".to_string(),
    _ => {
      return Err(crate::error::TriangleError::Api(
        "invalid base64 padding".to_string(),
      ));
    }
  };
  let decoded = base64::Engine::decode(
    &base64::engine::general_purpose::STANDARD,
    format!("{}{}", b64, pad),
  )
  .map_err(|e| crate::error::TriangleError::Api(format!("base64 decode error: {}", e)))?;

  let payload: serde_json::Value = serde_json::from_slice(&decoded)
    .map_err(|e| crate::error::TriangleError::Api(format!("JWT payload JSON error: {}", e)))?;

  payload["sub"]
    .as_str()
    .map(|s| s.to_string())
    .ok_or_else(|| crate::error::TriangleError::Api("JWT missing 'sub' claim".to_string()))
}
