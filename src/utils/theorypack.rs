use crate::error::Result;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum TheorypackResult<T, E = serde_json::Value> {
  Success {
    success: bool,
    #[serde(flatten)]
    value: T,
  },
  Error {
    success: bool,
    error: E,
  },
}

pub fn pack<T: Serialize>(value: &T) -> Result<Vec<u8>> {
  Ok(rmp_serde::to_vec_named(value)?)
}

pub fn unpack<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
  Ok(rmp_serde::from_slice(bytes)?)
}

pub fn unpack_multiple<T: DeserializeOwned>(bytes: &[u8]) -> Result<Vec<T>> {
  let mut cursor = std::io::Cursor::new(bytes);
  let mut values = Vec::new();
  let total = bytes.len() as u64;

  while cursor.position() < total {
    values.push(rmp_serde::from_read(&mut cursor)?);
  }

  Ok(values)
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
  pack(value)
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
  unpack(bytes)
}
