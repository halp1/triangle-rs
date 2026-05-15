use crate::error::Result;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A function called when a custom extension type code is encountered during decode.
/// Takes the raw extension data bytes and returns a decoded `Value`.
pub type UnpackExtFn = Arc<dyn Fn(&[u8]) -> Result<Value> + Send + Sync>;

/// A function called when a custom extension type code is encountered during decode,
/// receiving the already-decoded inner `Value` (equivalent to msgpackr JS `extension.read`).
/// For fixext1, the inner value is the next value in the stream (filler byte is skipped).
pub type UnpackValueExtFn = Arc<dyn Fn(Value) -> Result<Value> + Send + Sync>;

/// A function called when a `Value::Ext` with a matching type code is being encoded.
/// Returns `Some(bytes)` to replace the raw payload, or `None` to use the payload as-is.
pub type PackExtFn = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// Registry of custom extension type handlers, equivalent to msgpackr's `addExtension`.
///
/// Register decode handlers with [`ExtRegistry::add_unpack`] and (optionally)
/// custom encode transforms with [`ExtRegistry::add_pack`].
#[derive(Clone, Default)]
pub struct ExtRegistry {
  unpackers: HashMap<i8, UnpackExtFn>,
  value_unpackers: HashMap<i8, UnpackValueExtFn>,
  packers: HashMap<i8, PackExtFn>,
}

impl ExtRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// Register a decode handler for a custom extension type code.
  /// Called whenever the decoder encounters `ext(type_code, data)`.
  pub fn add_unpack(
    &mut self,
    type_code: i8,
    f: impl Fn(&[u8]) -> Result<Value> + Send + Sync + 'static,
  ) {
    self.unpackers.insert(type_code, Arc::new(f));
  }

  /// Register a value-based decode handler (equivalent to msgpackr JS `extension.read`).
  /// For fixext1, the decoder skips the filler byte and reads the next stream value,
  /// passing the decoded `Value` directly to this handler.
  pub fn add_unpack_value(
    &mut self,
    type_code: i8,
    f: impl Fn(Value) -> Result<Value> + Send + Sync + 'static,
  ) {
    self.value_unpackers.insert(type_code, Arc::new(f));
  }

  /// Register an encode transform for a custom extension type code.
  /// Called when encoding `Value::Ext(type_code, data)` — can rewrite the payload.
  /// Most users don't need this; it exists for advanced compatibility with JS `addExtension`.
  pub fn add_pack(
    &mut self,
    type_code: i8,
    f: impl Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static,
  ) {
    self.packers.insert(type_code, Arc::new(f));
  }

  /// Try to decode custom extension data. Returns `None` if no handler is registered.
  pub fn try_unpack(&self, type_code: i8, data: &[u8]) -> Option<Result<Value>> {
    self.unpackers.get(&type_code).map(|f| f(data))
  }

  /// Try to decode with a value-based handler. Returns `None` if no handler is registered.
  pub fn try_unpack_value(&self, type_code: i8, inner: Value) -> Option<Result<Value>> {
    self.value_unpackers.get(&type_code).map(|f| f(inner))
  }

  /// Returns true if a value-based handler is registered for this type code.
  pub fn has_unpack_value(&self, type_code: i8) -> bool {
    self.value_unpackers.contains_key(&type_code)
  }

  /// Try to transform ext payload on encode. Returns `None` if no handler is registered.
  pub fn try_pack(&self, type_code: i8, data: &[u8]) -> Option<Vec<u8>> {
    self.packers.get(&type_code).and_then(|f| f(data))
  }
}

/// Float32 encoding mode — matches msgpackr's FLOAT32_OPTIONS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float32Mode {
  #[default]
  Never = 0,
  Always = 1,
  DecimalRound = 3,
  DecimalFit = 4,
}

/// Options for the Packer/encoder.
#[derive(Debug, Clone)]
pub struct PackOptions {
  /// If true, use the record extension for JS-object-style maps.
  /// Default false for standalone pack(), true for Packer::new().
  pub use_records: bool,
  /// Use variable map size (fixmap/map16/map32). Needed for >65535 keys.
  pub variable_map_size: bool,
  /// Encode undefined as nil (0xc0) instead of fixext1(0,0).
  pub encode_undefined_as_nil: bool,
  /// Encode non-integer floats as float32 when possible.
  pub use_float32: Float32Mode,
  /// Enable moreTypes (Set, Error, RegExp, TypedArray, ArrayBuffer, DataView).
  pub more_types: bool,
  /// Enable structured cloning (id/pointer extensions).
  pub structured_clone: bool,
  /// Bundle strings for faster decoding on browsers.
  pub bundle_strings: bool,
  /// Use Timestamp32 (drops milliseconds) when possible.
  pub use_timestamp32: bool,
  /// For large bigints, convert to float64 instead of erroring.
  pub large_bigint_to_float: bool,
  /// For large bigints, convert to string instead of erroring.
  pub large_bigint_to_string: bool,
  /// For large bigints, use the BigInt extension (0x42).
  pub use_bigint_extension: bool,
  /// Shared structures array for the record extension.
  pub structures: Option<Vec<Vec<String>>>,
  /// Max shared structures (default 32, max 8160).
  pub max_shared_structures: usize,
  /// Max own (local) structures.
  pub max_own_structures: usize,
  /// Encode Maps as empty objects (back-compat).
  pub map_as_empty_object: bool,
  /// Encode Sets as empty objects (back-compat).
  pub set_as_empty_object: bool,
  /// Encode sequential structures (stream mode).
  pub sequential: bool,
  /// Coerce string keys that look like numbers to numbers.
  pub coercible_key_as_number: bool,
}

impl Default for PackOptions {
  fn default() -> Self {
    PackOptions {
      use_records: false,
      variable_map_size: false,
      encode_undefined_as_nil: false,
      use_float32: Float32Mode::Never,
      more_types: false,
      structured_clone: false,
      bundle_strings: false,
      use_timestamp32: false,
      large_bigint_to_float: false,
      large_bigint_to_string: false,
      use_bigint_extension: false,
      structures: None,
      max_shared_structures: 0,
      max_own_structures: 64,
      map_as_empty_object: false,
      set_as_empty_object: false,
      sequential: false,
      coercible_key_as_number: false,
    }
  }
}

/// Options for the Unpacker/decoder.
#[derive(Debug, Clone, Default)]
pub struct UnpackOptions {
  /// If true (and structures provided), decode record extension.
  pub use_records: bool,
  /// Decode maps as objects (key→value pairs). Default true.
  pub maps_as_objects: bool,
  /// How to decode int64/uint64: None = bigint/UInt, "number", "string", "auto"
  pub int64_as_type: Option<String>,
  /// Decode float32 with decimal rounding.
  pub use_float32: u8,
  /// Freeze decoded arrays and objects (read-only).
  pub freeze_data: bool,
  /// Shared structures for record extension decoding.
  pub structures: Option<Vec<Vec<String>>>,
  /// Enable structured clone decoding.
  pub structured_clone: bool,
  /// Copy binary buffers instead of slicing.
  pub copy_buffers: bool,
  /// Allow arrays in map keys.
  pub allow_arrays_in_map_keys: bool,
}

impl UnpackOptions {
  pub fn standard() -> Self {
    UnpackOptions {
      maps_as_objects: true,
      ..Default::default()
    }
  }
}
