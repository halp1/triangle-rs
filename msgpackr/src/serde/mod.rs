pub mod de;
pub mod ser;

use self::de::MsgpackDeserializer;
use self::ser::MsgpackSerializer;
use crate::error::Result;
use crate::packer::Packer;
use crate::value::Value;
use serde::{de::DeserializeOwned, Serialize};

/// Serialize a value using a given Packer (respects use_records, etc.)
pub fn to_vec_with_packer<T: Serialize>(value: &T, packer: &mut Packer) -> Result<Vec<u8>> {
  let mut ser = MsgpackSerializer::new(packer);
  value.serialize(&mut ser)?;
  Ok(ser.buf)
}

/// Deserialize a Value into any DeserializeOwned type.
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T> {
  T::deserialize(MsgpackDeserializer::new(value))
}
