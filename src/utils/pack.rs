use std::sync::OnceLock;

use msgpackr::{Error, PackOptions, Packer, UnpackOptions, Unpacker, Value};

static UNPACKER: OnceLock<Unpacker> = OnceLock::new();

fn unpacker() -> &'static Unpacker {
  UNPACKER.get_or_init(|| {
    let mut unpacker = Unpacker::with_options(UnpackOptions {
      int64_as_type: Some("number".to_string()),
      ..Default::default()
    });
    unpacker.add_extension(1, |data| {
			println!("detected extension 1, decoding...");
      let inner = if data.is_empty() {
        Value::Nil
      } else {
        msgpackr::unpack(data)?
      };
      let mut pairs = vec![(Value::Str("success".to_string()), Value::Bool(true))];
      if let Value::Map(extra) = inner {
        pairs.extend(extra);
      }
			println!("unpacked extension 1: {pairs:?}");
      Ok(Value::Map(pairs))
    });
    unpacker.add_extension(2, |data| {
			println!("detected extension 2, decoding...");
      let inner = if data.is_empty() {
        Value::Nil
      } else {
        msgpackr::unpack(data)?
      };
      let mut pairs = vec![(Value::Str("success".to_string()), Value::Bool(false))];
      if !matches!(inner, Value::Nil) {
        pairs.push((Value::Str("error".to_string()), inner));
      }
			println!("unpacked extension 2: {pairs:?}");
      Ok(Value::Map(pairs))
    });

    unpacker
  })
}

pub fn unpack(data: &[u8]) -> Result<Value, String> {
  unpacker()
    .unpack(data)
    .map_err(|e| format!("Failed to unpack data: {e}"))
}

pub fn unpack_typed<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, Error> {
  let value = unpacker().unpack(data)?;
  msgpackr::serde::from_value(value)
}

pub fn pack(value: &Value) -> Result<Vec<u8>, Error> {
  let mut packer = Packer::with_options(PackOptions {
    bundle_strings: true,
    sequential: true,
    ..Default::default()
  });
  packer.pack_value(value)
}

pub fn pack_typed(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
  let mut packer = Packer::with_options(PackOptions {
    bundle_strings: true,
    sequential: true,
    ..Default::default()
  });
  msgpackr::serde::to_vec_with_packer(value, &mut packer)
    .map_err(|e| format!("Failed to pack data: {e}"))
}
