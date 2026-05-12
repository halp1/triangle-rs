use crate::decode::Decoder;
use crate::encode::*;
use crate::error::{Error, Result};
use crate::options::{ExtRegistry, PackOptions, UnpackOptions};
use crate::value::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Structure transition trie node — used to efficiently look up record IDs by key sequence.
type Transitions = HashMap<String, TransitionNode>;

#[derive(Default)]
struct TransitionNode {
  children: Transitions,
  record_id: Option<usize>,
}

/// Packer: stateful encoder that supports the record extension, shared structures,
/// bundle strings, and structured cloning. Equivalent to msgpackr's `Packr`.
pub struct Packer {
  pub options: PackOptions,
  /// The structures table: index = record ID (0-based), value = ordered field names
  pub structures: Vec<Vec<String>>,
  /// Number of structures that are "shared" (persisted across calls)
  pub shared_length: usize,
  /// Transition trie root for fast structure lookup
  transitions: Option<TransitionNode>,
  /// IDs available for new own (non-shared) structures
  next_own_id: usize,
  /// IDs available for new shared structures
  next_shared_id: usize,
  /// Custom extension handlers
  pub ext_registry: Option<Arc<ExtRegistry>>,
}

impl Packer {
  /// Create a new Packer with record extension enabled by default (matches `new Packr()` in JS).
  pub fn new() -> Self {
    Packer::with_options(PackOptions {
      use_records: true,
      max_shared_structures: 0,
      max_own_structures: 64,
      ..Default::default()
    })
  }

  /// Create a Packer with given options.
  pub fn with_options(options: PackOptions) -> Self {
    let has_shared = options.structures.is_some();
    let max_shared = if options.max_shared_structures > 0 {
      options.max_shared_structures
    } else if has_shared {
      32
    } else {
      0
    };
    let max_own = if options.max_own_structures > 0 {
      options.max_own_structures
    } else if has_shared {
      32
    } else {
      64
    };

    let structures = options.structures.clone().unwrap_or_default();
    let shared_length = structures.len();
    let next_shared_id = 0x40 + shared_length;
    let next_own_id = 0x40 + max_shared;

    let mut p = Packer {
      options,
      structures,
      shared_length,
      transitions: None,
      next_own_id,
      next_shared_id,
      ext_registry: None,
    };
    p.options.max_shared_structures = max_shared;
    p.options.max_own_structures = max_own;
    p
  }

  /// Create a standard-mode Packer (use_records=false) — produces plain msgpack.
  pub fn standard() -> Self {
    Packer::with_options(PackOptions::default())
  }

  /// Register a custom extension handler.
  ///
  /// - `type_code`: the MessagePack ext type code (must not conflict with built-in codes)
  /// - `unpack`: called when this type code is encountered during decode
  /// - `pack`: optional transform on `Value::Ext` payload during encode
  ///
  /// Reserved codes used by msgpackr: `0x00` (Undefined), `0x42` (BigInt), `0x62` (bundle),
  /// `0x65` (Error), `0x69` (ID ref), `0x70` (pointer), `0x72` (record def),
  /// `0x73` (Set), `0x74` (TypedArray), `0x78` (RegExp), `0xff` (Timestamp).
  pub fn add_extension(
    &mut self,
    type_code: i8,
    unpack: impl Fn(&[u8]) -> crate::error::Result<Value> + Send + Sync + 'static,
    pack: Option<impl Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static>,
  ) {
    let reg = Arc::make_mut(
      self
        .ext_registry
        .get_or_insert_with(|| Arc::new(ExtRegistry::new())),
    );
    reg.add_unpack(type_code, unpack);
    if let Some(p) = pack {
      reg.add_pack(type_code, p);
    }
  }

  /// Encode any `Value` to bytes with the Packer's options.
  pub fn pack_value(&mut self, val: &Value) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);
    self.encode_value_into(val, &mut buf)?;
    Ok(buf)
  }

  /// Encode a `Value` into an existing buffer.
  pub fn encode_value_into(&mut self, val: &Value, buf: &mut Vec<u8>) -> Result<()> {
    match val {
      Value::Map(pairs) if self.options.use_records => {
        let keys: Vec<&str> = pairs
          .iter()
          .map(|(k, _)| match k {
            Value::Str(s) => s.as_str(),
            _ => "",
          })
          .collect();
        self.encode_record_object(&keys, pairs, buf)
      }
      _ => encode_value_with_registry(val, buf, &self.options, self.ext_registry.as_ref()),
    }
  }

  /// Build transition trie from current structures (called lazily).
  fn ensure_transitions(&mut self) {
    if self.transitions.is_none() {
      let mut root = TransitionNode::default();
      for (i, keys) in self.structures.iter().enumerate() {
        if keys.is_empty() {
          continue;
        }
        let mut node = &mut root;
        for key in keys {
          node = node.children.entry(key.clone()).or_default();
        }
        node.record_id = Some(i + 0x40);
      }
      self.transitions = Some(root);
    }
  }

  /// Look up a record ID for the given keys. Returns None if not found.
  fn find_record_id(&mut self, keys: &[&str]) -> Option<usize> {
    self.ensure_transitions();
    let root = self.transitions.as_ref()?;
    let mut node = root;
    for key in keys {
      node = node.children.get(*key)?;
    }
    node.record_id
  }

  /// Register a new structure and return its ID.
  /// Returns (id, is_shared).
  fn register_structure(&mut self, keys: &[&str]) -> (usize, bool) {
    let max_shared = self.options.max_shared_structures;
    let max_own = self.options.max_own_structures;
    let shared_limit_id = max_shared + 0x40;
    let max_struct_id = max_shared + max_own + 0x40;

    let is_shared = self.next_shared_id < shared_limit_id;
    let id = if is_shared {
      let id = self.next_shared_id;
      self.next_shared_id += 1;
      id
    } else {
      // Use own structure slot (cycles)
      let id = self.next_own_id;
      self.next_own_id += 1;
      if self.next_own_id >= max_struct_id {
        self.next_own_id = shared_limit_id;
      }
      id
    };

    let idx = id - 0x40;
    let owned_keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
    if idx >= self.structures.len() {
      self.structures.resize(idx + 1, Vec::new());
    }
    self.structures[idx] = owned_keys.clone();
    if is_shared {
      self.shared_length = (self.shared_length).max(idx + 1);
    }

    // Update transitions trie
    self.transitions = None; // invalidate, will rebuild on next need

    (id, is_shared)
  }

  /// Use 2-byte record IDs?
  fn use_two_byte_records(&self) -> bool {
    self.options.max_shared_structures > 32
      || (self.options.max_own_structures + self.options.max_shared_structures > 64)
  }

  /// Encode a map/object using the record extension.
  /// `keys` must be string keys in field order.
  /// `pairs` must match keys in same order.
  pub fn encode_record_object(
    &mut self,
    keys: &[&str],
    pairs: &[(Value, Value)],
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    if let Some(record_id) = self.find_record_id(keys) {
      self.write_record_id(record_id, buf);
    } else {
      let (record_id, is_shared) = self.register_structure(keys);
      if is_shared {
        // Shared structure: just write the ID
        self.write_record_id(record_id, buf);
      } else {
        // Own structure: write record definition inline
        self.write_record_definition(record_id, keys, buf)?;
      }
    }
    // Write the values
    for (_, v) in pairs {
      self.encode_value_into(v, buf)?;
    }
    Ok(())
  }

  fn write_record_id(&self, id: usize, buf: &mut Vec<u8>) {
    if self.use_two_byte_records() && id >= 0x60 {
      let adjusted = id - 0x60;
      buf.push(((adjusted & 0x1f) + 0x60) as u8);
      buf.push((adjusted >> 5) as u8);
    } else {
      buf.push(id as u8);
    }
  }

  fn write_record_definition(&self, id: usize, keys: &[&str], buf: &mut Vec<u8>) -> Result<()> {
    if self.use_two_byte_records() && id >= 0x60 {
      let adjusted = id - 0x60;
      buf.push(0xd5); // fixext2
      buf.push(0x72); // 'r'
      buf.push(((adjusted & 0x1f) + 0x60) as u8);
      buf.push((adjusted >> 5) as u8);
    } else {
      buf.push(0xd4); // fixext1
      buf.push(0x72); // 'r'
      buf.push(id as u8);
    }
    // Write the keys array
    let key_vals: Vec<Value> = keys.iter().map(|k| Value::Str(k.to_string())).collect();
    encode_array(&key_vals, buf, &self.options)?;
    Ok(())
  }

  /// Decode a byte slice using this packer's structures and options.
  pub fn unpack_value(&self, data: &[u8]) -> Result<Value> {
    let opts = self.unpack_opts();
    let mut dec = Decoder::new(data).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    let val = dec.read(&opts)?;
    if dec.position() != data.len() {
      return Err(Error::invalid("Data read, but end of buffer not reached"));
    }
    Ok(val)
  }

  /// Decode multiple values.
  pub fn unpack_multiple(&self, data: &[u8]) -> Result<Vec<Value>> {
    let opts = self.unpack_opts();
    let mut dec = Decoder::new(data).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    let mut values = Vec::new();
    while dec.position() < data.len() {
      values.push(dec.read(&opts)?);
    }
    Ok(values)
  }

  fn unpack_opts(&self) -> UnpackOptions {
    UnpackOptions {
      use_records: self.options.use_records,
      maps_as_objects: true,
      structured_clone: self.options.structured_clone,
      ..Default::default()
    }
  }

  /// Clear all shared data (structures).
  pub fn clear_shared_data(&mut self) {
    self.structures.clear();
    self.shared_length = 0;
    self.transitions = None;
    self.next_shared_id = 0x40;
    self.next_own_id = 0x40 + self.options.max_shared_structures;
  }
}

impl Default for Packer {
  fn default() -> Self {
    Packer::new()
  }
}

/// Encode a field map using the record extension with a given packer state.
/// Used by the serde serializer.
pub struct RecordEncoder {
  pub keys: Vec<String>,
  pub values_buf: Vec<u8>,
}

impl RecordEncoder {
  pub fn new() -> Self {
    RecordEncoder {
      keys: Vec::new(),
      values_buf: Vec::new(),
    }
  }

  pub fn add_field(&mut self, key: &str, value_bytes: &[u8]) {
    self.keys.push(key.to_owned());
    self.values_buf.extend_from_slice(value_bytes);
  }

  /// Flush the record into `buf`, using `packer` to look up/register the structure.
  pub fn flush(self, packer: &mut Packer, buf: &mut Vec<u8>) -> Result<()> {
    let key_refs: Vec<&str> = self.keys.iter().map(|s| s.as_str()).collect();

    if let Some(record_id) = packer.find_record_id(&key_refs) {
      packer.write_record_id(record_id, buf);
    } else {
      let (record_id, is_shared) = packer.register_structure(&key_refs);
      if is_shared {
        packer.write_record_id(record_id, buf);
      } else {
        packer.write_record_definition(record_id, &key_refs, buf)?;
      }
    }
    buf.extend_from_slice(&self.values_buf);
    Ok(())
  }
}

impl Default for RecordEncoder {
  fn default() -> Self {
    Self::new()
  }
}
