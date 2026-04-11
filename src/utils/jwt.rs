use crate::error::{Result, TriangleError};
use serde_json::Value;

fn decode_segment(segment: &str) -> Result<Vec<u8>> {
  let b64 = segment.replace('-', "+").replace('_', "/");
  let pad = match b64.len() % 4 {
    0 => "",
    2 => "==",
    3 => "=",
    _ => return Err(TriangleError::InvalidToken),
  };

  Ok(base64::Engine::decode(
    &base64::engine::general_purpose::STANDARD,
    format!("{}{}", b64, pad),
  )?)
}

pub fn parse_token_payload(token: &str) -> Result<Value> {
  let payload = token.split('.').nth(1).ok_or(TriangleError::InvalidToken)?;
  let decoded = decode_segment(payload)?;
  Ok(serde_json::from_slice(&decoded)?)
}

pub fn parse_token(token: &str) -> Result<String> {
  parse_token_payload(token)?["sub"]
    .as_str()
    .map(str::to_string)
    .ok_or(TriangleError::InvalidToken)
}
