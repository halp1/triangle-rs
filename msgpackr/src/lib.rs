pub mod decode;
pub mod encode;
pub mod error;
pub mod iter;
pub mod options;
pub mod packer;
pub mod serde;
pub mod unpacker;
pub mod value;

pub use error::{Error, Result};
pub use iter::{Iter, PackedIter};
pub use options::{ExtRegistry, Float32Mode, PackOptions, UnpackOptions};
pub use packer::Packer;
pub use unpacker::Unpacker;
pub use value::Value;

use decode::{unpack_multiple_values, unpack_value, unpack_value_with_opts};
use encode::encode_value;

/// Encode a `Value` to bytes using standard msgpack (no record extension).
pub fn pack(val: &Value) -> Result<Vec<u8>> {
  let opts = PackOptions::default();
  let mut buf = Vec::new();
  encode_value(val, &mut buf, &opts)?;
  Ok(buf)
}

/// Encode a `Value` to bytes with specific options.
pub fn pack_with_opts(val: &Value, opts: &PackOptions) -> Result<Vec<u8>> {
  let mut buf = Vec::new();
  encode_value(val, &mut buf, opts)?;
  Ok(buf)
}

/// Decode bytes to a `Value` using standard msgpack.
pub fn unpack(data: &[u8]) -> Result<Value> {
  unpack_value(data)
}

/// Decode bytes to a `Value` with specific options.
pub fn unpack_opts(data: &[u8], opts: &UnpackOptions) -> Result<Value> {
  unpack_value_with_opts(data, opts)
}

/// Decode multiple values from a single buffer.
pub fn unpack_multiple(data: &[u8]) -> Result<Vec<Value>> {
  unpack_multiple_values(data)
}

/// Serialize any `serde::Serialize` type to msgpack bytes using default options.
pub fn to_vec<T: ::serde::Serialize>(value: &T) -> Result<Vec<u8>> {
  let mut packer = Packer::standard();
  serde::to_vec_with_packer(value, &mut packer)
}

/// Deserialize msgpack bytes into any `serde::de::DeserializeOwned` type.
pub fn from_slice<T: ::serde::de::DeserializeOwned>(data: &[u8]) -> Result<T> {
  let val = unpack_value(data)?;
  serde::from_value(val)
}

/// Serialize using the record extension for compact object encoding.
pub fn to_vec_records<T: ::serde::Serialize>(value: &T) -> Result<Vec<u8>> {
  let mut packer = Packer::new();
  serde::to_vec_with_packer(value, &mut packer)
}
