use crate::error::{Error, Result};
use crate::options::{ExtRegistry, UnpackOptions};
use crate::value::Value;
use std::sync::Arc;

/// Stateful decoder reading from a byte slice.
pub struct Decoder<'a> {
  src: &'a [u8],
  pub(crate) pos: usize,
  pub(crate) end: usize,
  structures: Vec<Vec<String>>,
  bundled_strings: Option<BundledStrings>,
  reference_map: Option<ReferenceMap>,
  pub(crate) ext_registry: Option<Arc<ExtRegistry>>,
}

/// Decoded bundle string pairs (for the 0x62 extension).
#[derive(Clone)]
struct BundledStrings {
  two_byte: String,
  one_byte: String,
  pos0: usize,
  pos1: usize,
  post_bundle_pos: usize,
}

/// Tracks id→Value mappings for structured clone decoding.
struct ReferenceMap {
  entries: std::collections::HashMap<u32, RefEntry>,
}

#[derive(Clone)]
struct RefEntry {
  value: Value,
}

impl ReferenceMap {
  fn new() -> Self {
    ReferenceMap {
      entries: std::collections::HashMap::new(),
    }
  }
  fn insert(&mut self, id: u32, val: Value) {
    self.entries.insert(id, RefEntry { value: val });
  }
  fn get(&self, id: u32) -> Option<&Value> {
    self.entries.get(&id).map(|e| &e.value)
  }
}

impl<'a> Decoder<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Decoder {
      src,
      pos: 0,
      end: src.len(),
      structures: Vec::new(),
      bundled_strings: None,
      reference_map: None,
      ext_registry: None,
    }
  }

  pub fn new_with_end(src: &'a [u8], end: usize) -> Self {
    Decoder {
      src,
      pos: 0,
      end,
      structures: Vec::new(),
      bundled_strings: None,
      reference_map: None,
      ext_registry: None,
    }
  }

  pub fn with_structures(mut self, structures: Vec<Vec<String>>) -> Self {
    self.structures = structures;
    self
  }

  pub fn with_ext_registry(mut self, registry: Arc<ExtRegistry>) -> Self {
    self.ext_registry = Some(registry);
    self
  }

  pub fn with_reference_map(mut self) -> Self {
    self.reference_map = Some(ReferenceMap::new());
    self
  }

  pub fn position(&self) -> usize {
    self.pos
  }
  pub fn remaining(&self) -> usize {
    self.end.saturating_sub(self.pos)
  }

  pub fn finalize_bundle_strings(&mut self) {
    if let Some(bs) = self.bundled_strings.take() {
      self.pos = bs.post_bundle_pos;
    }
  }

  #[inline]
  fn read_u8(&mut self) -> Result<u8> {
    if self.pos >= self.end {
      return Err(Error::UnexpectedEnd);
    }
    let b = self.src[self.pos];
    self.pos += 1;
    Ok(b)
  }

  #[inline]
  fn read_bytes(&mut self, n: usize) -> Result<&[u8]> {
    let end = self.pos + n;
    if end > self.end {
      return Err(Error::UnexpectedEnd);
    }
    let slice = &self.src[self.pos..end];
    self.pos = end;
    Ok(slice)
  }

  #[inline]
  fn peek_u8(&self) -> Result<u8> {
    if self.pos >= self.end {
      return Err(Error::UnexpectedEnd);
    }
    Ok(self.src[self.pos])
  }

  fn read_u16_be(&mut self) -> Result<u16> {
    let b = self.read_bytes(2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
  }

  fn read_u32_be(&mut self) -> Result<u32> {
    let b = self.read_bytes(4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
  }

  fn read_i8(&mut self) -> Result<i8> {
    Ok(self.read_u8()? as i8)
  }

  fn read_i16_be(&mut self) -> Result<i16> {
    let b = self.read_bytes(2)?;
    Ok(i16::from_be_bytes([b[0], b[1]]))
  }

  fn read_i32_be(&mut self) -> Result<i32> {
    let b = self.read_bytes(4)?;
    Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
  }

  fn read_i64_be(&mut self) -> Result<i64> {
    let b = self.read_bytes(8)?;
    Ok(i64::from_be_bytes([
      b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
  }

  fn read_u64_be(&mut self) -> Result<u64> {
    let b = self.read_bytes(8)?;
    Ok(u64::from_be_bytes([
      b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
  }

  fn read_f32_be(&mut self) -> Result<f32> {
    let b = self.read_bytes(4)?;
    Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
  }

  fn read_f64_be(&mut self) -> Result<f64> {
    let b = self.read_bytes(8)?;
    Ok(f64::from_be_bytes([
      b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
  }

  fn read_string(&mut self, len: usize) -> Result<String> {
    let bytes = self.read_bytes(len)?;
    // Validate UTF-8 with replacement characters for invalid sequences
    Ok(String::from_utf8_lossy(bytes).into_owned())
  }

  fn read_bin(&mut self, len: usize) -> Result<Vec<u8>> {
    let bytes = self.read_bytes(len)?;
    Ok(bytes.to_vec())
  }

  /// Decode a record definition (extension type 0x72).
  /// Returns the newly defined structure's fields as a Value object.
  fn read_record_definition(
    &mut self,
    first_byte: u8,
    second_byte: Option<u8>,
    opts: &UnpackOptions,
  ) -> Result<Value> {
    let id = if let Some(high) = second_byte {
      let low = first_byte & 0x3f;
      (high as usize) * 32 + (low as usize)
    } else {
      (first_byte & 0x3f) as usize
    };

    // Read the keys array
    let keys_value = self.read(opts)?;
    let keys: Vec<String> = match keys_value {
      Value::Array(arr) => arr
        .into_iter()
        .map(|v| match v {
          Value::Str(s) => s,
          other => format!("{}", other),
        })
        .collect(),
      _ => return Err(Error::invalid("Record definition must have array of keys")),
    };

    // Store the structure
    if id >= self.structures.len() {
      self.structures.resize(id + 1, Vec::new());
    }
    self.structures[id] = keys.clone();

    // Now read the values and build the object
    self.read_record_object(id, opts)
  }

  /// Read an object with `id` from the structure table.
  fn read_record_object(&mut self, id: usize, opts: &UnpackOptions) -> Result<Value> {
    let keys = if id < self.structures.len() {
      self.structures[id].clone()
    } else {
      return Err(Error::invalid(format!("Record id {} is not defined", id)));
    };

    let mut pairs = Vec::with_capacity(keys.len());
    for key in &keys {
      // Sanitize __proto__ to avoid prototype pollution
      let safe_key = if key == "__proto__" {
        "__proto_"
      } else {
        key.as_str()
      };
      let val = self.read(opts)?;
      pairs.push((Value::Str(safe_key.to_owned()), val));
    }
    Ok(Value::Map(pairs))
  }

  /// Read an ext block and interpret it.
  fn read_ext(&mut self, len: usize, opts: &UnpackOptions) -> Result<Value> {
    let type_code = self.read_u8()?;
    let start = self.pos;
    let end = self.pos + len;
    if end > self.end {
      return Err(Error::UnexpectedEnd);
    }

    match type_code {
      0xff => {
        let val = self.decode_timestamp(len)?;
        self.pos = end;
        Ok(val)
      }
      0x00 => {
        self.pos = end;
        Ok(Value::Undefined)
      }
      0x72 => {
        // Record definition: ID bytes follow
        let fb = self.read_u8()?;
        let sb = if len == 2 {
          Some(self.read_u8()?)
        } else {
          None
        };
        self.read_record_definition(fb & 0x3f, sb, opts)
      }
      0x62 => self.decode_bundle_strings(len, start, end, opts),
      0x73 => {
        self.pos = end;
        let arr = self.read(opts)?;
        match arr {
          Value::Array(items) => Ok(Value::Set(items)),
          other => Ok(Value::Set(vec![other])),
        }
      }
      0x65 => {
        self.pos = end;
        let data = self.read(opts)?;
        match data {
          Value::Array(arr) => {
            let cause = arr.get(2).cloned().and_then(|v| {
              if v == Value::Nil {
                None
              } else {
                Some(Box::new(v))
              }
            });
            let message = arr
              .get(1)
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            let name = arr
              .first()
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_else(|| "Error".to_owned());
            Ok(Value::MsgpackError {
              name,
              message,
              cause,
            })
          }
          _ => Err(Error::invalid("Error extension data must be array")),
        }
      }
      0x78 => {
        self.pos = end;
        let data = self.read(opts)?;
        match data {
          Value::Array(arr) => {
            let source = arr
              .first()
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            let flags = arr
              .get(1)
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            Ok(Value::Regex { source, flags })
          }
          _ => Err(Error::invalid("RegExp extension data must be array")),
        }
      }
      0x74 => {
        if len < 1 {
          return Err(Error::UnexpectedEnd);
        }
        let array_type = self.read_u8()?;
        let data = self.read_bytes(len - 1)?.to_vec();
        self.pos = end;
        match array_type {
          0x10 => Ok(Value::ArrayBuffer(data)),
          0x11 => Ok(Value::DataView(data)),
          code => Ok(Value::TypedArray {
            type_code: code,
            data,
          }),
        }
      }
      0x42 => {
        let data = self.read_bytes(len)?.to_vec();
        self.pos = end;
        self.decode_bigint(&data)
      }
      0x70 => {
        if len != 4 {
          return Err(Error::invalid("Pointer ext must be 4 bytes"));
        }
        let id = self.read_u32_be()?;
        self.pos = end;
        self.resolve_pointer(id)
      }
      0x69 => {
        if len != 4 {
          return Err(Error::invalid("ID ext must be 4 bytes"));
        }
        let id = self.read_u32_be()?;
        self.pos = end;
        self.decode_id_ref(id, opts)
      }
      _ => {
        let data = self.read_bytes(len)?.to_vec();
        self.pos = end;
        if let Some(ref reg) = self.ext_registry.clone() {
          if let Some(result) = reg.try_unpack(type_code as i8, &data) {
            return result;
          }
        }
        Ok(Value::Ext(type_code as i8, data))
      }
    }
  }

  /// Decode the timestamp extension (type 0xff).
  fn decode_timestamp(&mut self, len: usize) -> Result<Value> {
    match len {
      4 => {
        let secs = self.read_u32_be()? as i64;
        Ok(Value::Timestamp {
          seconds: secs,
          nanos: 0,
        })
      }
      8 => {
        let hi = self.read_u32_be()?;
        let lo = self.read_u32_be()?;
        let nanos = hi >> 2;
        let secs = ((hi as i64 & 0x3) << 32) | (lo as i64);
        Ok(Value::Timestamp {
          seconds: secs,
          nanos,
        })
      }
      12 => {
        let nanos = self.read_u32_be()?;
        let secs = self.read_i64_be()?;
        Ok(Value::Timestamp {
          seconds: secs,
          nanos,
        })
      }
      _ => {
        // Invalid timestamp — return as invalid (matching JS behavior of returning "Invalid Date")
        self.pos += len;
        Ok(Value::Timestamp {
          seconds: i64::MIN,
          nanos: 0xFFFFFFFF,
        })
      }
    }
  }

  /// Decode the bundle strings extension (type 0x62).
  fn decode_bundle_strings(
    &mut self,
    len: usize,
    start: usize,
    end: usize,
    opts: &UnpackOptions,
  ) -> Result<Value> {
    // Read the 4-byte relative offset to where the bundled strings are
    if len < 4 {
      return Err(Error::UnexpectedEnd);
    }
    let data_size = u32::from_be_bytes([
      self.src[start],
      self.src[start + 1],
      self.src[start + 2],
      self.src[start + 3],
    ]) as usize;

    let bundle_start = self.pos + data_size;
    if bundle_start >= self.end {
      return Err(Error::UnexpectedEnd);
    }

    self.pos = bundle_start;

    // Read the two bundled strings
    let two_byte = self.read_only_js_string(opts)?;
    let one_byte = self.read_only_js_string(opts)?;
    let post_bundle = self.pos;

    // Restore to right after the ext block so read() decodes the main data
    self.pos = end;

    self.bundled_strings = Some(BundledStrings {
      two_byte,
      one_byte,
      pos0: 0,
      pos1: 0,
      post_bundle_pos: post_bundle,
    });

    // Read ONE value inline (which will consume bundled strings via 0xc1 markers).
    // Do NOT advance to post_bundle_pos here — the top-level caller does that after
    // the entire document is decoded (mirroring the JS decoder).
    self.read(opts)
  }

  /// Read a string-only value (used for bundle string reading).
  fn read_only_js_string(&mut self, _opts: &UnpackOptions) -> Result<String> {
    let token = self.read_u8()?;
    let len = if token < 0xc0 {
      // fixstr
      (token - 0xa0) as usize
    } else {
      match token {
        0xd9 => self.read_u8()? as usize,
        0xda => self.read_u16_be()? as usize,
        0xdb => self.read_u32_be()? as usize,
        _ => return Err(Error::invalid("Expected string in bundle")),
      }
    };
    self.read_string(len)
  }

  /// Decode the BigInt extension (type 0x42).
  fn decode_bigint(&self, data: &[u8]) -> Result<Value> {
    #[cfg(feature = "bigint")]
    {
      use num_bigint::{BigInt, Sign};
      if data.is_empty() {
        return Ok(Value::BigInt(BigInt::from(0)));
      }
      let negative = data[0] & 0x80 != 0;
      if negative {
        // Two's complement — invert all bytes
        let mut inverted = data.to_vec();
        for b in &mut inverted {
          *b = !*b;
        }
        // subtract 1
        let mut borrow = true;
        for b in inverted.iter_mut().rev() {
          if borrow {
            if *b == 0 {
              *b = 0xff;
            } else {
              *b -= 1;
              borrow = false;
            }
          }
        }
        let n = BigInt::from_bytes_be(Sign::Plus, &inverted);
        Ok(Value::BigInt(-n))
      } else {
        let n = BigInt::from_bytes_be(Sign::Plus, data);
        Ok(Value::BigInt(n))
      }
    }
    #[cfg(not(feature = "bigint"))]
    {
      // Without bigint feature, return as raw Ext
      Ok(Value::Ext(0x42i8, data.to_vec()))
    }
  }

  /// Resolve a structured clone pointer reference.
  fn resolve_pointer(&self, id: u32) -> Result<Value> {
    if let Some(ref_map) = &self.reference_map {
      if let Some(val) = ref_map.get(id) {
        return Ok(val.clone());
      }
    }
    Err(Error::invalid(format!(
      "Unresolved structured clone pointer: {}",
      id
    )))
  }

  /// Decode a structured clone ID reference.
  fn decode_id_ref(&mut self, id: u32, opts: &UnpackOptions) -> Result<Value> {
    // Enable reference map if needed
    if self.reference_map.is_none() {
      self.reference_map = Some(ReferenceMap::new());
    }

    // Read the next value
    let val = self.read(opts)?;

    if let Some(ref_map) = &mut self.reference_map {
      ref_map.insert(id, val.clone());
    }

    Ok(val)
  }

  /// Main read function — reads a single value from the stream.
  pub fn read(&mut self, opts: &UnpackOptions) -> Result<Value> {
    let token = self.read_u8()?;

    if token < 0xa0 {
      if token < 0x80 {
        if token < 0x40 {
          // Positive fixint 0-63
          return Ok(Value::UInt(token as u64));
        } else {
          // Bytes 0x40-0x7f: record reference (if structures available), else positive fixint
          let id = (token & 0x3f) as usize;
          if id < self.structures.len() && !self.structures[id].is_empty() {
            // Check if this is a two-byte record ID
            let high_byte_needed =
              id < self.structures.len() && !self.structures[id].is_empty() && token >= 0x60;
            if high_byte_needed && token < 0x80 {
              // Potentially a two-byte record: peek at next byte
              // For simplicity: if next byte exists and is 0, it's a one-byte record
              // if non-zero, it's a two-byte extension
              // msgpackr uses: (recordId & 0x1f) + 0x60 for first byte when highByte >= 0
              // We check the second byte to determine which structure
              // Actually: the structure's .highByte tells us if two-byte is needed
              // In our Rust impl, we track this differently
            }
            return self.read_record_object(id, opts);
          } else {
            return Ok(Value::UInt(token as u64));
          }
        }
      } else if token < 0x90 {
        // fixmap
        let len = (token - 0x80) as usize;
        return self.read_map(len, opts);
      } else {
        // fixarray
        let len = (token - 0x90) as usize;
        return self.read_array(len, opts);
      }
    } else if token < 0xc0 {
      // fixstr
      let len = (token - 0xa0) as usize;
      // Check for bundled strings (C1 marker used as 0xc1 in bundleStrings mode is separate)
      let s = self.read_string(len)?;
      return Ok(Value::Str(s));
    } else {
      match token {
        0xc0 => return Ok(Value::Nil),
        0xc1 => {
          // In bundleStrings mode: read the string from bundled strings array
          // Followed by signed integer (length or negative length)
          if let Some(bs) = &self.bundled_strings {
            let bs = bs.clone();
            // Read the length (next value)
            drop(bs);
            let len_val = self.read(opts)?;
            let len_i = match len_val {
              Value::Int(n) => n,
              Value::UInt(n) => n as i64,
              _ => return Err(Error::invalid("Expected integer after 0xc1")),
            };
            if let Some(ref mut bs) = self.bundled_strings {
              if len_i > 0 {
                // one-byte (Latin-1) string
                let len = len_i as usize;
                let s = bs.one_byte[bs.pos1..bs.pos1 + len].to_owned();
                bs.pos1 += len;
                return Ok(Value::Str(s));
              } else {
                // two-byte (multi-byte) string
                let len = (-len_i) as usize;
                let s = bs.two_byte[bs.pos0..bs.pos0 + len].to_owned();
                bs.pos0 += len;
                return Ok(Value::Str(s));
              }
            }
          }
          // Outside bundleStrings: return C1 marker or undefined
          return Ok(Value::Undefined);
        }
        0xc2 => return Ok(Value::Bool(false)),
        0xc3 => return Ok(Value::Bool(true)),
        0xc4 => {
          // bin8
          let len = self.read_u8()? as usize;
          let data = self.read_bin(len)?;
          return Ok(Value::Bin(data));
        }
        0xc5 => {
          // bin16
          let len = self.read_u16_be()? as usize;
          let data = self.read_bin(len)?;
          return Ok(Value::Bin(data));
        }
        0xc6 => {
          // bin32
          let len = self.read_u32_be()? as usize;
          let data = self.read_bin(len)?;
          return Ok(Value::Bin(data));
        }
        0xc7 => {
          // ext8
          let len = self.read_u8()? as usize;
          return self.read_ext(len, opts);
        }
        0xc8 => {
          // ext16
          let len = self.read_u16_be()? as usize;
          return self.read_ext(len, opts);
        }
        0xc9 => {
          // ext32
          let len = self.read_u32_be()? as usize;
          return self.read_ext(len, opts);
        }
        0xca => {
          // float32
          let f = self.read_f32_be()?;
          if opts.use_float32 > 2 {
            return Ok(Value::F64(crate::encode::round_float32(f)));
          }
          return Ok(Value::F32(f));
        }
        0xcb => {
          // float64
          let f = self.read_f64_be()?;
          return Ok(Value::F64(f));
        }
        0xcc => return Ok(Value::UInt(self.read_u8()? as u64)),
        0xcd => return Ok(Value::UInt(self.read_u16_be()? as u64)),
        0xce => return Ok(Value::UInt(self.read_u32_be()? as u64)),
        0xcf => {
          let v = self.read_u64_be()?;
          return self.decode_uint64(v, opts);
        }
        0xd0 => return Ok(Value::Int(self.read_i8()? as i64)),
        0xd1 => return Ok(Value::Int(self.read_i16_be()? as i64)),
        0xd2 => return Ok(Value::Int(self.read_i32_be()? as i64)),
        0xd3 => {
          let v = self.read_i64_be()?;
          return self.decode_int64(v, opts);
        }
        0xd4 => {
          // fixext1 — type byte + 1 data byte
          let ext_type = self.read_u8()?;
          if ext_type == 0x72 {
            let id_byte = self.read_u8()?;
            return self.read_record_definition(id_byte & 0x3f, None, opts);
          }
          let end = self.pos + 1;
          return self.dispatch_ext_data(ext_type, 1, end, opts);
        }
        0xd5 => {
          // fixext2 — type byte + 2 data bytes
          let ext_type = self.peek_u8()?;
          if ext_type == 0x72 {
            self.pos += 1; // consume type byte
            let b1 = self.read_u8()?;
            let b2 = self.read_u8()?;
            return self.read_record_definition(b1 & 0x3f, Some(b2), opts);
          }
          return self.read_ext(2, opts);
        }
        0xd6 => return self.read_ext(4, opts),
        0xd7 => return self.read_ext(8, opts),
        0xd8 => return self.read_ext(16, opts),
        0xd9 => {
          let len = self.read_u8()? as usize;
          return Ok(Value::Str(self.read_string(len)?));
        }
        0xda => {
          let len = self.read_u16_be()? as usize;
          return Ok(Value::Str(self.read_string(len)?));
        }
        0xdb => {
          let len = self.read_u32_be()? as usize;
          return Ok(Value::Str(self.read_string(len)?));
        }
        0xdc => {
          let len = self.read_u16_be()? as usize;
          return self.read_array(len, opts);
        }
        0xdd => {
          let len = self.read_u32_be()? as usize;
          return self.read_array(len, opts);
        }
        0xde => {
          let len = self.read_u16_be()? as usize;
          return self.read_map(len, opts);
        }
        0xdf => {
          let len = self.read_u32_be()? as usize;
          return self.read_map(len, opts);
        }
        // negative fixint: 0xe0-0xff
        _ if token >= 0xe0 => {
          return Ok(Value::Int(token as i8 as i64));
        }
        _ => {
          return Err(Error::invalid(format!(
            "Unknown MessagePack token: 0x{:02x}",
            token
          )));
        }
      }
    }

    unreachable!()
  }

  /// Re-dispatch already-peeked ext data by type code.
  /// Called when we read the type byte separately (fixext1 case).
  /// `start` = position of data bytes, `end` = position after data bytes.
  fn dispatch_ext_data(
    &mut self,
    type_code: u8,
    len: usize,
    end: usize,
    opts: &UnpackOptions,
  ) -> Result<Value> {
    let start = self.pos;
    match type_code {
      0xff => {
        let val = self.decode_timestamp(len)?;
        self.pos = end;
        Ok(val)
      }
      0x00 => {
        self.pos = end;
        Ok(Value::Undefined)
      }
      0x62 => self.decode_bundle_strings(len, start, end, opts),
      0x73 => {
        self.pos = end;
        let arr = self.read(opts)?;
        match arr {
          Value::Array(items) => Ok(Value::Set(items)),
          other => Ok(Value::Set(vec![other])),
        }
      }
      0x65 => {
        self.pos = end;
        let data = self.read(opts)?;
        match data {
          Value::Array(arr) => {
            let cause = arr.get(2).cloned().and_then(|v| {
              if v == Value::Nil {
                None
              } else {
                Some(Box::new(v))
              }
            });
            let message = arr
              .get(1)
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            let name = arr
              .first()
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_else(|| "Error".to_owned());
            Ok(Value::MsgpackError {
              name,
              message,
              cause,
            })
          }
          _ => Err(Error::invalid("Error extension data must be array")),
        }
      }
      0x78 => {
        self.pos = end;
        let data = self.read(opts)?;
        match data {
          Value::Array(arr) => {
            let source = arr
              .first()
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            let flags = arr
              .get(1)
              .and_then(|v| v.as_str().map(|s| s.to_owned()))
              .unwrap_or_default();
            Ok(Value::Regex { source, flags })
          }
          _ => Err(Error::invalid("RegExp extension data must be array")),
        }
      }
      0x74 => {
        if len < 1 {
          return Err(Error::UnexpectedEnd);
        }
        let array_type = self.read_u8()?;
        let data = self.read_bytes(len - 1)?.to_vec();
        self.pos = end;
        match array_type {
          0x10 => Ok(Value::ArrayBuffer(data)),
          0x11 => Ok(Value::DataView(data)),
          code => Ok(Value::TypedArray {
            type_code: code,
            data,
          }),
        }
      }
      0x42 => {
        let data = self.read_bytes(len)?.to_vec();
        self.pos = end;
        self.decode_bigint(&data)
      }
      0x70 => {
        if len != 4 {
          return Err(Error::invalid("Pointer ext must be 4 bytes"));
        }
        let id = self.read_u32_be()?;
        self.pos = end;
        self.resolve_pointer(id)
      }
      0x69 => {
        if len != 4 {
          return Err(Error::invalid("ID ext must be 4 bytes"));
        }
        let id = self.read_u32_be()?;
        self.pos = end;
        self.decode_id_ref(id, opts)
      }
      _ => {
        let data = self.read_bytes(len)?.to_vec();
        self.pos = end;
        if let Some(ref reg) = self.ext_registry.clone() {
          if let Some(result) = reg.try_unpack(type_code as i8, &data) {
            return result;
          }
        }
        Ok(Value::Ext(type_code as i8, data))
      }
    }
  }

  fn read_array(&mut self, len: usize, opts: &UnpackOptions) -> Result<Value> {
    let mut arr = Vec::with_capacity(len.min(65536));
    for _ in 0..len {
      arr.push(self.read(opts)?);
    }
    Ok(Value::Array(arr))
  }

  fn read_map(&mut self, len: usize, opts: &UnpackOptions) -> Result<Value> {
    if opts.maps_as_objects {
      let mut pairs = Vec::with_capacity(len.min(1024));
      for _ in 0..len {
        let key = self.read_key(opts)?;
        let val = self.read(opts)?;
        pairs.push((key, val));
      }
      Ok(Value::Map(pairs))
    } else {
      let mut pairs = Vec::with_capacity(len.min(1024));
      for _ in 0..len {
        let key = self.read(opts)?;
        let val = self.read(opts)?;
        pairs.push((key, val));
      }
      Ok(Value::Map(pairs))
    }
  }

  /// Read a map key — same as read() but sanitizes __proto__.
  fn read_key(&mut self, opts: &UnpackOptions) -> Result<Value> {
    let key = self.read(opts)?;
    match &key {
      Value::Str(s) if s == "__proto__" => Ok(Value::Str("__proto_".to_owned())),
      _ => Ok(key),
    }
  }

  fn decode_uint64(&self, v: u64, opts: &UnpackOptions) -> Result<Value> {
    match opts.int64_as_type.as_deref() {
      Some("number") => Ok(Value::F64(v as f64)),
      Some("string") => Ok(Value::Str(v.to_string())),
      Some("auto") => {
        if v <= (1u64 << 53) {
          Ok(Value::UInt(v))
        } else {
          #[cfg(feature = "bigint")]
          {
            Ok(Value::BigInt(num_bigint::BigInt::from(v)))
          }
          #[cfg(not(feature = "bigint"))]
          {
            Ok(Value::UInt(v))
          }
        }
      }
      _ => {
        // Default: try to fit in i64, otherwise UInt
        if v <= i64::MAX as u64 {
          Ok(Value::Int(v as i64))
        } else {
          Ok(Value::UInt(v))
        }
      }
    }
  }

  fn decode_int64(&self, v: i64, opts: &UnpackOptions) -> Result<Value> {
    match opts.int64_as_type.as_deref() {
      Some("number") => Ok(Value::F64(v as f64)),
      Some("string") => Ok(Value::Str(v.to_string())),
      Some("auto") => {
        let threshold = 1i64 << 53;
        if v >= -threshold && v <= threshold {
          Ok(Value::Int(v))
        } else {
          #[cfg(feature = "bigint")]
          {
            Ok(Value::BigInt(num_bigint::BigInt::from(v)))
          }
          #[cfg(not(feature = "bigint"))]
          {
            Ok(Value::Int(v))
          }
        }
      }
      _ => Ok(Value::Int(v)),
    }
  }
}

/// Decode a single Value from a byte slice. Standard msgpack, no records.
pub fn unpack_value(data: &[u8]) -> Result<Value> {
  unpack_value_with_opts(data, &UnpackOptions::default())
}

pub fn unpack_value_with_opts(data: &[u8], opts: &UnpackOptions) -> Result<Value> {
  let mut dec = Decoder::new(data);
  let val = dec.read(opts)?;
  dec.finalize_bundle_strings();
  if dec.pos != dec.end {
    Err(Error::invalid(format!(
      "Data read, but end of buffer not reached at pos={}/end={}",
      dec.pos, dec.end
    )))
  } else {
    Ok(val)
  }
}

/// Decode multiple values from a buffer.
pub fn unpack_multiple_values(data: &[u8]) -> Result<Vec<Value>> {
  let opts = UnpackOptions::default();
  let mut dec = Decoder::new(data);
  let mut values = Vec::new();
  while dec.pos < dec.end {
    values.push(dec.read(&opts)?);
    dec.finalize_bundle_strings();
  }
  Ok(values)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::encode::encode_value;
  use crate::options::PackOptions;

  fn roundtrip(val: &Value) -> Value {
    let opts = PackOptions::default();
    let mut buf = Vec::new();
    encode_value(val, &mut buf, &opts).unwrap();
    unpack_value(&buf).unwrap()
  }

  #[test]
  fn test_nil() {
    assert_eq!(roundtrip(&Value::Nil), Value::Nil);
  }
  #[test]
  fn test_bool() {
    assert_eq!(roundtrip(&Value::Bool(true)), Value::Bool(true));
    assert_eq!(roundtrip(&Value::Bool(false)), Value::Bool(false));
  }
  #[test]
  fn test_integers() {
    for n in [
      0i64,
      1,
      63,
      64,
      127,
      128,
      255,
      256,
      65535,
      65536,
      i32::MAX as i64,
      i64::MAX,
    ] {
      let v = Value::Int(n);
      assert_eq!(roundtrip(&v), Value::UInt(n as u64), "n={}", n);
    }
    for n in [-1i64, -32, -33, -128, -129, -32768, -32769, i32::MIN as i64] {
      let v = Value::Int(n);
      assert_eq!(roundtrip(&v), Value::Int(n), "n={}", n);
    }
  }
  #[test]
  fn test_string() {
    let s = "hello world ᾜ";
    assert_eq!(
      roundtrip(&Value::Str(s.to_owned())),
      Value::Str(s.to_owned())
    );
  }
  #[test]
  fn test_array() {
    let arr = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    assert_eq!(roundtrip(&arr), arr);
  }
}
