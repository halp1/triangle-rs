use crate::decode::Decoder;
use crate::error::Result;
use crate::options::UnpackOptions;
use crate::packer::Packer;
use crate::value::Value;

/// Iterator over multiple msgpack values in a single buffer.
pub struct Iter<'a> {
  decoder: Decoder<'a>,
  opts: UnpackOptions,
}

impl<'a> Iter<'a> {
  pub fn new(data: &'a [u8]) -> Self {
    Iter {
      decoder: Decoder::new(data),
      opts: UnpackOptions::standard(),
    }
  }

  pub fn with_opts(data: &'a [u8], opts: UnpackOptions) -> Self {
    Iter {
      decoder: Decoder::new(data),
      opts,
    }
  }
}

impl<'a> Iterator for Iter<'a> {
  type Item = Result<Value>;

  fn next(&mut self) -> Option<Result<Value>> {
    if self.decoder.remaining() == 0 {
      return None;
    }
    Some(self.decoder.read(&self.opts))
  }
}

/// Iterate over encoded values using a Packer's settings.
pub struct PackedIter<'a> {
  decoder: Decoder<'a>,
  opts: UnpackOptions,
}

impl<'a> PackedIter<'a> {
  pub fn new(data: &'a [u8], packer: &Packer) -> Self {
    PackedIter {
      decoder: Decoder::new(data).with_structures(packer.structures.clone()),
      opts: UnpackOptions {
        use_records: packer.options.use_records,
        maps_as_objects: true,
        ..Default::default()
      },
    }
  }
}

impl<'a> Iterator for PackedIter<'a> {
  type Item = Result<Value>;

  fn next(&mut self) -> Option<Result<Value>> {
    if self.decoder.remaining() == 0 {
      return None;
    }
    Some(self.decoder.read(&self.opts))
  }
}
