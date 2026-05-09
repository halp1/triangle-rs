use crate::error::{Result};
use crate::options::{Float32Mode, PackOptions};
use crate::value::{typed_array_codes, Value};
use std::sync::Arc;

fn mult10_table() -> &'static [f64; 256] {
  use std::sync::OnceLock;
  static TABLE: OnceLock<[f64; 256]> = OnceLock::new();
  TABLE.get_or_init(|| {
    let mut t = [0f64; 256];
    for i in 0..256usize {
      let exp = 45.15 - (i as f64) * 0.30103;
      t[i] = 10f64.powf(exp.floor());
    }
    t
  })
}

/// Round a f64 to the nearest significant decimal for float32 decoding.
/// Matches JS `roundFloat32` in msgpackr.
pub fn round_float32(value: f32) -> f64 {
  let bytes = value.to_be_bytes();
  let idx = (((bytes[0] & 0x7f) as usize) << 1) | ((bytes[1] >> 7) as usize);
  let mult = mult10_table()[idx];
  let v = value as f64;
  if mult == 0.0 {
    return v;
  }
  let shifted = mult * v + if v > 0.0 { 0.5 } else { -0.5 };
  (shifted as i64 as f64) / mult
}

/// Core encode of a Value into a byte buffer, with optional extension registry.
pub fn encode_value_with_registry(
  val: &Value,
  buf: &mut Vec<u8>,
  opts: &PackOptions,
  registry: Option<&Arc<crate::options::ExtRegistry>>,
) -> Result<()> {
  if let (Value::Ext(t, data), Some(reg)) = (val, registry) {
    if let Some(new_data) = reg.try_pack(*t, data) {
      encode_ext_raw(*t as u8, &new_data, buf);
      return Ok(());
    }
  }
  encode_value(val, buf, opts)
}

/// Core encode of a Value into a byte buffer.
pub fn encode_value(val: &Value, buf: &mut Vec<u8>, opts: &PackOptions) -> Result<()> {
  match val {
    Value::Nil => buf.push(0xc0),
    Value::Undefined => {
      if opts.encode_undefined_as_nil {
        buf.push(0xc0);
      } else {
        // fixext1, type=0, data=0 (as used by notepack and msgpackr)
        buf.push(0xd4);
        buf.push(0x00);
        buf.push(0x00);
      }
    }
    Value::Bool(true) => buf.push(0xc3),
    Value::Bool(false) => buf.push(0xc2),
    Value::Int(n) => encode_signed(*n, buf, opts),
    Value::UInt(n) => encode_unsigned(*n, buf, opts),
    Value::F32(f) => {
      buf.push(0xca);
      buf.extend_from_slice(&f.to_be_bytes());
    }
    Value::F64(f) => encode_f64(*f, buf, opts),
    Value::Str(s) => encode_str(s.as_str(), buf),
    Value::Bin(b) => encode_bin(b.as_slice(), buf),
    Value::Array(arr) => encode_array(arr, buf, opts)?,
    Value::Map(map) => encode_map(map, buf, opts)?,
    Value::Ext(t, data) => encode_ext_raw(*t as u8, data, buf),
    Value::Timestamp { seconds, nanos } => encode_timestamp(*seconds, *nanos, buf, opts),
    Value::Set(items) => {
      if opts.more_types {
        buf.push(0xd4); // fixext1
        buf.push(0x73); // 's' for Set
        buf.push(0x00); // filler
      }
      encode_array(items, buf, opts)?;
    }
    Value::MsgpackError {
      name,
      message,
      cause,
    } => {
      if opts.more_types {
        buf.push(0xd4); // fixext1
        buf.push(0x65); // 'e' for Error
        buf.push(0x00); // filler
      }
      let cause_val = cause.as_deref().cloned().unwrap_or(Value::Nil);
      encode_array(
        &[
          Value::Str(name.clone()),
          Value::Str(message.clone()),
          cause_val,
        ],
        buf,
        opts,
      )?;
    }
    Value::Regex { source, flags } => {
      if opts.more_types {
        buf.push(0xd4); // fixext1
        buf.push(0x78); // 'x' for regeXp
        buf.push(0x00); // filler
      }
      encode_array(
        &[Value::Str(source.clone()), Value::Str(flags.clone())],
        buf,
        opts,
      )?;
    }
    Value::TypedArray { type_code, data } => {
      write_ext_buffer(data, *type_code, buf);
    }
    Value::ArrayBuffer(data) => {
      if opts.more_types {
        write_ext_buffer(data, typed_array_codes::ARRAY_BUFFER, buf);
      } else {
        encode_bin(data, buf);
      }
    }
    Value::DataView(data) => {
      if opts.more_types {
        write_ext_buffer(data, typed_array_codes::DATA_VIEW, buf);
      } else {
        encode_bin(data, buf);
      }
    }
    #[cfg(feature = "bigint")]
    Value::BigInt(n) => encode_bigint(n, buf, opts)?,
  }
  Ok(())
}

/// Encode a signed integer using the most compact representation.
/// With use_records=true: fixint only for 0..=63 (0x40-0x7f reserved for record IDs)
/// With use_records=false: fixint for 0..=127 (standard msgpack)
pub(crate) fn encode_signed(n: i64, buf: &mut Vec<u8>, opts: &PackOptions) {
  if n >= 0 {
    encode_unsigned(n as u64, buf, opts);
  } else {
    // negative fixint: -32 to -1 → bytes 0xe0-0xff
    if n >= -32 {
      buf.push((0x100i32 + n as i32) as u8);
    } else if n >= -128 {
      buf.push(0xd0);
      buf.push(n as u8);
    } else if n >= -32768 {
      buf.push(0xd1);
      let bytes = (n as i16).to_be_bytes();
      buf.extend_from_slice(&bytes);
    } else if n >= -2_147_483_648 {
      buf.push(0xd2);
      let bytes = (n as i32).to_be_bytes();
      buf.extend_from_slice(&bytes);
    } else {
      buf.push(0xd3);
      buf.extend_from_slice(&n.to_be_bytes());
    }
  }
}

/// Encode an unsigned integer. With records, limits fixint to < 64.
pub(crate) fn encode_unsigned(n: u64, buf: &mut Vec<u8>, opts: &PackOptions) {
  // JS: value < 0x20 || (value < 0x80 && this.useRecords === false) || (value < 0x40 && !this._writeStruct)
  // In Rust (no native addon, so !_writeStruct is always true):
  //   n < 0x20  →  always fixint
  //   n < 0x80 && !use_records  →  fixint when records disabled
  //   n < 0x40 (always, since !_writeStruct)  →  fixint
  // Combined: n < 0x40 || (!use_records && n < 0x80)
  let use_fixint = n < 0x40 || (!opts.use_records && n < 0x80);
  if use_fixint {
    buf.push(n as u8);
  } else if n < 0x100 {
    buf.push(0xcc);
    buf.push(n as u8);
  } else if n < 0x10000 {
    buf.push(0xcd);
    buf.extend_from_slice(&(n as u16).to_be_bytes());
  } else if n < 0x1_0000_0000 {
    buf.push(0xce);
    buf.extend_from_slice(&(n as u32).to_be_bytes());
  } else {
    buf.push(0xcf);
    buf.extend_from_slice(&n.to_be_bytes());
  }
}

/// Encode a f64. With float32 modes, may use 0xca instead of 0xcb.
pub(crate) fn encode_f64(value: f64, buf: &mut Vec<u8>, opts: &PackOptions) {
  match opts.use_float32 {
    Float32Mode::Never => {
      buf.push(0xcb);
      buf.extend_from_slice(&value.to_be_bytes());
    }
    Float32Mode::Always => {
      // Encode as float32 if value fits in the range
      if value < 0x1_0000_0000_u64 as f64 && value >= -2_147_483_648.0 {
        buf.push(0xca);
        buf.extend_from_slice(&(value as f32).to_be_bytes());
      } else {
        buf.push(0xcb);
        buf.extend_from_slice(&value.to_be_bytes());
      }
    }
    Float32Mode::DecimalRound => {
      if value < 0x1_0000_0000_u64 as f64 && value >= -2_147_483_648.0 {
        buf.push(0xca);
        buf.extend_from_slice(&(value as f32).to_be_bytes());
      } else {
        buf.push(0xcb);
        buf.extend_from_slice(&value.to_be_bytes());
      }
    }
    Float32Mode::DecimalFit => {
      if value < 0x1_0000_0000_u64 as f64 && value >= -2_147_483_648.0 {
        let f32_val = value as f32;
        let bytes = f32_val.to_be_bytes();
        let idx = (((bytes[0] & 0x7f) as usize) << 1) | ((bytes[1] >> 7) as usize);
        let mult = mult10_table()[idx];
        let x_shifted = value * mult;
        // If the value fits losslessly as float32 (x_shifted rounds to an integer)
        if mult == 0.0 || (x_shifted + 0.5) as i64 as f64 == x_shifted.round() {
          buf.push(0xca);
          buf.extend_from_slice(&bytes);
        } else {
          buf.push(0xcb);
          buf.extend_from_slice(&value.to_be_bytes());
        }
      } else {
        buf.push(0xcb);
        buf.extend_from_slice(&value.to_be_bytes());
      }
    }
  }
}

/// Encode a UTF-8 string.
pub(crate) fn encode_str(s: &str, buf: &mut Vec<u8>) {
  let bytes = s.as_bytes();
  let len = bytes.len();
  if len < 0x20 {
    buf.push(0xa0 | len as u8);
  } else if len < 0x100 {
    buf.push(0xd9);
    buf.push(len as u8);
  } else if len < 0x10000 {
    buf.push(0xda);
    buf.extend_from_slice(&(len as u16).to_be_bytes());
  } else {
    buf.push(0xdb);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
  }
  buf.extend_from_slice(bytes);
}

/// Encode binary data.
pub(crate) fn encode_bin(data: &[u8], buf: &mut Vec<u8>) {
  let len = data.len();
  if len < 0x100 {
    buf.push(0xc4);
    buf.push(len as u8);
  } else if len < 0x10000 {
    buf.push(0xc5);
    buf.extend_from_slice(&(len as u16).to_be_bytes());
  } else {
    buf.push(0xc6);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
  }
  buf.extend_from_slice(data);
}

/// Encode an array of Values.
pub(crate) fn encode_array(arr: &[Value], buf: &mut Vec<u8>, opts: &PackOptions) -> Result<()> {
  let len = arr.len();
  if len < 0x10 {
    buf.push(0x90 | len as u8);
  } else if len < 0x10000 {
    buf.push(0xdc);
    buf.extend_from_slice(&(len as u16).to_be_bytes());
  } else {
    buf.push(0xdd);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
  }
  for v in arr {
    encode_value(v, buf, opts)?;
  }
  Ok(())
}

/// Encode a map of (Value, Value) pairs.
pub(crate) fn encode_map(
  map: &[(Value, Value)],
  buf: &mut Vec<u8>,
  opts: &PackOptions,
) -> Result<()> {
  let len = map.len();
  if len < 0x10 {
    buf.push(0x80 | len as u8);
  } else if len < 0x10000 {
    buf.push(0xde);
    buf.extend_from_slice(&(len as u16).to_be_bytes());
  } else {
    buf.push(0xdf);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
  }
  for (k, v) in map {
    encode_value(k, buf, opts)?;
    encode_value(v, buf, opts)?;
  }
  Ok(())
}

/// Write a fixext/ext header for a given length and type code, then write data.
pub(crate) fn encode_ext_raw(type_code: u8, data: &[u8], buf: &mut Vec<u8>) {
  let len = data.len();
  match len {
    1 => {
      buf.push(0xd4);
      buf.push(type_code);
    }
    2 => {
      buf.push(0xd5);
      buf.push(type_code);
    }
    4 => {
      buf.push(0xd6);
      buf.push(type_code);
    }
    8 => {
      buf.push(0xd7);
      buf.push(type_code);
    }
    16 => {
      buf.push(0xd8);
      buf.push(type_code);
    }
    _ if len < 0x100 => {
      buf.push(0xc7);
      buf.push(len as u8);
      buf.push(type_code);
    }
    _ if len < 0x10000 => {
      buf.push(0xc8);
      buf.extend_from_slice(&(len as u16).to_be_bytes());
      buf.push(type_code);
    }
    _ => {
      buf.push(0xc9);
      buf.extend_from_slice(&(len as u32).to_be_bytes());
      buf.push(type_code);
    }
  }
  buf.extend_from_slice(data);
}

/// Write a typed array (or ArrayBuffer/DataView) as msgpackr ext(0x74) format.
/// Format: ext(0x74, [type_code, ...raw_bytes])
pub(crate) fn write_ext_buffer(data: &[u8], type_code: u8, buf: &mut Vec<u8>) {
  let payload_len = data.len() + 1; // +1 for type_code byte
  if payload_len < 0x100 {
    buf.push(0xc7);
    buf.push(payload_len as u8);
  } else if payload_len < 0x10000 {
    buf.push(0xc8);
    buf.extend_from_slice(&(payload_len as u16).to_be_bytes());
  } else {
    buf.push(0xc9);
    buf.extend_from_slice(&(payload_len as u32).to_be_bytes());
  }
  buf.push(0x74); // 't' for typed array
  buf.push(type_code);
  buf.extend_from_slice(data);
}

/// Encode a timestamp value using the msgpackr/msgpack timestamp extension (type 0xff).
/// Uses Timestamp32 (4 bytes) when possible, Timestamp64 (8 bytes) for nanosecond precision,
/// or Timestamp96 (12 bytes) for out-of-range.
pub(crate) fn encode_timestamp(seconds: i64, nanos: u32, buf: &mut Vec<u8>, opts: &PackOptions) {
  let use_32 = opts.use_timestamp32 || nanos == 0;
  if use_32 && seconds >= 0 && seconds < 0x1_0000_0000 {
    // Timestamp 32: 4 bytes, seconds only
    buf.push(0xd6); // fixext4
    buf.push(0xff); // timestamp type
    buf.extend_from_slice(&(seconds as u32).to_be_bytes());
  } else if seconds > 0 && seconds < 0x1_0000_0000 {
    // Timestamp 64: 8 bytes, nanoseconds in upper 30 bits + 2 high bits of seconds, then 32-bit seconds
    // nanoseconds adjustment: JS uses date.getMilliseconds() * 4000000 + ((seconds / 1000 / 0x100000000) >> 0)
    // For our purposes: pack nanos (0..=999_999_999) into upper 30 bits of first 32-bit word
    let upper = (nanos * 4) | ((seconds >> 32) as u32 & 0x3);
    buf.push(0xd7); // fixext8
    buf.push(0xff);
    buf.extend_from_slice(&upper.to_be_bytes());
    buf.extend_from_slice(&(seconds as u32).to_be_bytes());
  } else if seconds == 0 && nanos == 0 {
    // Epoch
    buf.push(0xd6);
    buf.push(0xff);
    buf.extend_from_slice(&[0u8; 4]);
  } else {
    // Timestamp 96: 12 bytes
    buf.push(0xc7); // ext8
    buf.push(12);
    buf.push(0xff);
    buf.extend_from_slice(&nanos.to_be_bytes());
    buf.extend_from_slice(&seconds.to_be_bytes());
  }
}

#[cfg(feature = "bigint")]
pub(crate) fn encode_bigint(
  n: &num_bigint::BigInt,
  buf: &mut Vec<u8>,
  opts: &PackOptions,
) -> Result<()> {
  use num_bigint::{BigInt, Sign};

  // First check if it fits in 64-bit
  if let Some(v) = n.to_i64() {
    buf.push(0xd3);
    buf.extend_from_slice(&v.to_be_bytes());
    return Ok(());
  }
  if let Some(v) = n.to_u64() {
    buf.push(0xcf);
    buf.extend_from_slice(&v.to_be_bytes());
    return Ok(());
  }

  // Large bigint — use extension 0x42
  // Format matches msgpackr: big-endian two's complement bytes
  let (sign, mut bytes) = n.to_bytes_be();
  if sign == Sign::Minus {
    // two's complement inversion
    for b in &mut bytes {
      *b = !*b;
    }
    // add 1 (two's complement)
    let mut carry = true;
    for b in bytes.iter_mut().rev() {
      if carry {
        let (nb, c) = b.overflowing_add(1);
        *b = nb;
        carry = c;
      } else {
        break;
      }
    }
  }
  // Ensure the leading byte correctly indicates sign
  // Positive: leading byte < 0x80; Negative: leading byte >= 0x80
  if sign == Sign::Plus && bytes[0] >= 0x80 {
    bytes.insert(0, 0x00);
  }
  encode_ext_raw(0x42, &bytes, buf);
  Ok(())
}

/// Main entry point: encode a Value with default options.
pub fn pack_value(val: &Value) -> Result<Vec<u8>> {
  let opts = PackOptions::default();
  let mut buf = Vec::new();
  encode_value(val, &mut buf, &opts)?;
  Ok(buf)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::options::PackOptions;

  #[test]
  fn test_nil() {
    let opts = PackOptions::default();
    let mut buf = Vec::new();
    encode_value(&Value::Nil, &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xc0]);
  }

  #[test]
  fn test_bool() {
    let opts = PackOptions::default();
    let mut buf = Vec::new();
    encode_value(&Value::Bool(true), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xc3]);
    buf.clear();
    encode_value(&Value::Bool(false), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xc2]);
  }

  #[test]
  fn test_fixint_standard() {
    // useRecords=false: 0-127 are fixint
    let opts = PackOptions {
      use_records: false,
      ..Default::default()
    };
    let mut buf = Vec::new();
    encode_value(&Value::UInt(127), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0x7f]);
  }

  #[test]
  fn test_fixint_records() {
    // useRecords=true: 64-127 are NOT fixint (reserved for record IDs)
    let opts = PackOptions {
      use_records: true,
      ..Default::default()
    };
    let mut buf = Vec::new();
    encode_value(&Value::UInt(64), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xcc, 0x40]); // must use uint8

    buf.clear();
    encode_value(&Value::UInt(63), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0x3f]); // 63 is still fixint
  }

  #[test]
  fn test_negative_fixint() {
    let opts = PackOptions::default();
    let mut buf = Vec::new();
    encode_value(&Value::Int(-1), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xff]);
    buf.clear();
    encode_value(&Value::Int(-32), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xe0]);
  }

  #[test]
  fn test_str() {
    let opts = PackOptions::default();
    let mut buf = Vec::new();
    encode_value(&Value::Str("hello".to_string()), &mut buf, &opts).unwrap();
    assert_eq!(buf, [0xa5, b'h', b'e', b'l', b'l', b'o']);
  }

  #[test]
  fn test_mult10() {
    // index 127 => floor(45.15 - 127*0.30103) = floor(45.15 - 38.23) = floor(6.92) = 6 → 1e6
    let t = mult10_table();
    assert_eq!(t[127], 1e6);
    assert_eq!(t[0], 10f64.powf(45.0)); // floor(45.15) = 45
  }

  #[test]
  fn test_round_float32() {
    let rounded = round_float32(7.99f32);
    assert!((rounded - 7.99).abs() < 1e-10, "rounded={}", rounded);
  }
}
