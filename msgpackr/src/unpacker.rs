use crate::decode::Decoder;
use crate::error::Result;
use crate::options::{ExtRegistry, UnpackOptions};
use crate::value::Value;
use std::sync::Arc;

/// Unpacker: stateful decoder with shared structures and options.
/// Equivalent to msgpackr's `Unpackr`.
pub struct Unpacker {
  pub options: UnpackOptions,
  pub structures: Vec<Vec<String>>,
  pub ext_registry: Option<Arc<ExtRegistry>>,
}

impl Unpacker {
  /// Standard-mode unpacker (no records, maps as objects).
  pub fn new() -> Self {
    Unpacker {
      options: UnpackOptions {
        maps_as_objects: true,
        ..Default::default()
      },
      structures: Vec::new(),
      ext_registry: None,
    }
  }

  /// Create an Unpacker with given options.
  pub fn with_options(options: UnpackOptions) -> Self {
    let structures = options.structures.clone().unwrap_or_default();
    Unpacker {
      options,
      structures,
      ext_registry: None,
    }
  }

  /// Register a custom extension handler.
  /// See [`Packer::add_extension`] for details on reserved type codes.
  pub fn add_extension(
    &mut self,
    type_code: i8,
    unpack: impl Fn(&[u8]) -> crate::error::Result<Value> + Send + Sync + 'static,
  ) {
    let reg = Arc::make_mut(
      self
        .ext_registry
        .get_or_insert_with(|| Arc::new(ExtRegistry::new())),
    );
    reg.add_unpack(type_code, unpack);
  }

  /// Decode a single value.
  pub fn unpack(&self, data: &[u8]) -> Result<Value> {
    let mut dec = Decoder::new(data).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    let val = dec.read(&self.options)?;
    dec.finalize_bundle_strings();
    if dec.position() != data.len() {
      return Err(crate::error::Error::invalid(format!(
        "Data read, but end of buffer not reached at {}/{}",
        dec.position(),
        data.len()
      )));
    }
    Ok(val)
  }

  /// Decode with explicit end position.
  pub fn unpack_with_end(&self, data: &[u8], end: usize) -> Result<Value> {
    let mut dec = Decoder::new_with_end(data, end).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    dec.read(&self.options)
  }

  /// Decode multiple sequential values from a single buffer.
  pub fn unpack_multiple(&self, data: &[u8]) -> Result<Vec<Value>> {
    let mut dec = Decoder::new(data).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    let mut values = Vec::new();
    while dec.position() < data.len() {
      values.push(dec.read(&self.options)?);
      dec.finalize_bundle_strings();
    }
    Ok(values)
  }

  /// Decode with callback for each value (can return false to stop).
  pub fn unpack_multiple_cb<F>(&self, data: &[u8], mut cb: F) -> Result<()>
  where
    F: FnMut(Value, usize, usize) -> bool,
  {
    let mut dec = Decoder::new(data).with_structures(self.structures.clone());
    if let Some(ref reg) = self.ext_registry {
      dec = dec.with_ext_registry(reg.clone());
    }
    while dec.position() < data.len() {
      let start = dec.position();
      let val = dec.read(&self.options)?;
      dec.finalize_bundle_strings();
      let end = dec.position();
      if !cb(val, start, end) {
        break;
      }
    }
    Ok(())
  }
}

impl Default for Unpacker {
  fn default() -> Self {
    Unpacker::new()
  }
}
