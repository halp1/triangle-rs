use crate::encode::*;
use crate::error::{Error, Result};
use crate::options::PackOptions;
use crate::packer::{Packer, RecordEncoder};
use crate::value::Value;
use serde::ser::{
  self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
  SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};

/// Serde serializer for msgpack.
pub struct MsgpackSerializer<'p> {
  packer: &'p mut Packer,
  pub buf: Vec<u8>,
}

impl<'p> MsgpackSerializer<'p> {
  pub fn new(packer: &'p mut Packer) -> Self {
    MsgpackSerializer {
      packer,
      buf: Vec::with_capacity(256),
    }
  }
}

impl<'p, 'a> ser::Serializer for &'a mut MsgpackSerializer<'p> {
  type Ok = ();
  type Error = Error;
  type SerializeSeq = SeqEncoder<'a, 'p>;
  type SerializeTuple = SeqEncoder<'a, 'p>;
  type SerializeTupleStruct = SeqEncoder<'a, 'p>;
  type SerializeTupleVariant = SeqEncoder<'a, 'p>;
  type SerializeMap = MapEncoder<'a, 'p>;
  type SerializeStruct = StructEncdr<'a, 'p>;
  type SerializeStructVariant = StructEncdr<'a, 'p>;

  fn serialize_bool(self, v: bool) -> Result<()> {
    self.buf.push(if v { 0xc3 } else { 0xc2 });
    Ok(())
  }
  fn serialize_i8(self, v: i8) -> Result<()> {
    encode_signed(v as i64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_i16(self, v: i16) -> Result<()> {
    encode_signed(v as i64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_i32(self, v: i32) -> Result<()> {
    encode_signed(v as i64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_i64(self, v: i64) -> Result<()> {
    encode_signed(v, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_u8(self, v: u8) -> Result<()> {
    encode_unsigned(v as u64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_u16(self, v: u16) -> Result<()> {
    encode_unsigned(v as u64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_u32(self, v: u32) -> Result<()> {
    encode_unsigned(v as u64, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_u64(self, v: u64) -> Result<()> {
    encode_unsigned(v, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_f32(self, v: f32) -> Result<()> {
    self.buf.push(0xca);
    self.buf.extend_from_slice(&v.to_be_bytes());
    Ok(())
  }
  fn serialize_f64(self, v: f64) -> Result<()> {
    encode_f64(v, &mut self.buf, &self.packer.options);
    Ok(())
  }
  fn serialize_char(self, v: char) -> Result<()> {
    let mut s = String::new();
    s.push(v);
    encode_str(&s, &mut self.buf);
    Ok(())
  }
  fn serialize_str(self, v: &str) -> Result<()> {
    encode_str(v, &mut self.buf);
    Ok(())
  }
  fn serialize_bytes(self, v: &[u8]) -> Result<()> {
    encode_bin(v, &mut self.buf);
    Ok(())
  }
  fn serialize_none(self) -> Result<()> {
    self.buf.push(0xc0);
    Ok(())
  }
  fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
    value.serialize(self)
  }
  fn serialize_unit(self) -> Result<()> {
    self.buf.push(0xc0);
    Ok(())
  }
  fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
    self.buf.push(0xc0);
    Ok(())
  }
  fn serialize_unit_variant(
    self,
    _name: &'static str,
    _variant_index: u32,
    variant: &'static str,
  ) -> Result<()> {
    encode_str(variant, &mut self.buf);
    Ok(())
  }
  fn serialize_newtype_struct<T: ?Sized + Serialize>(
    self,
    _name: &'static str,
    value: &T,
  ) -> Result<()> {
    value.serialize(self)
  }
  fn serialize_newtype_variant<T: ?Sized + Serialize>(
    self,
    _name: &'static str,
    _variant_index: u32,
    variant: &'static str,
    value: &T,
  ) -> Result<()> {
    // Encode as fixarray [variant_name, value]
    self.buf.push(0x92); // fixarray len=2
    encode_str(variant, &mut self.buf);
    value.serialize(self)
  }
  fn serialize_seq(self, len: Option<usize>) -> Result<SeqEncoder<'a, 'p>> {
    Ok(SeqEncoder::new(self, len))
  }
  fn serialize_tuple(self, len: usize) -> Result<SeqEncoder<'a, 'p>> {
    Ok(SeqEncoder::new(self, Some(len)))
  }
  fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SeqEncoder<'a, 'p>> {
    Ok(SeqEncoder::new(self, Some(len)))
  }
  fn serialize_tuple_variant(
    self,
    _name: &'static str,
    _variant_index: u32,
    variant: &'static str,
    len: usize,
  ) -> Result<SeqEncoder<'a, 'p>> {
    // Encode as [variant_name, [values...]]
    self.buf.push(0x92); // fixarray len=2
    encode_str(variant, &mut self.buf);
    Ok(SeqEncoder::new(self, Some(len)))
  }
  fn serialize_map(self, _len: Option<usize>) -> Result<MapEncoder<'a, 'p>> {
    Ok(MapEncoder::new(self))
  }
  fn serialize_struct(self, name: &'static str, _len: usize) -> Result<StructEncdr<'a, 'p>> {
    Ok(StructEncdr::new(self, name))
  }
  fn serialize_struct_variant(
    self,
    _name: &'static str,
    _variant_index: u32,
    variant: &'static str,
    len: usize,
  ) -> Result<StructEncdr<'a, 'p>> {
    // Encode as [variant_name, {fields...}]
    self.buf.push(0x92); // fixarray len=2
    encode_str(variant, &mut self.buf);
    Ok(StructEncdr::new(self, variant))
  }
}

// ─── Sequence encoder ───────────────────────────────────────────────────────

pub struct SeqEncoder<'a, 'p> {
  ser: &'a mut MsgpackSerializer<'p>,
  len: Option<usize>,
  // For unknown-length sequences, buffer items and prepend header later
  items_buf: Vec<u8>,
  count: usize,
}

fn write_array_header(n: usize, buf: &mut Vec<u8>) {
  if n < 16 {
    buf.push(0x90 | n as u8);
  } else if n < 0x10000 {
    buf.push(0xdc);
    buf.extend_from_slice(&(n as u16).to_be_bytes());
  } else {
    buf.push(0xdd);
    buf.extend_from_slice(&(n as u32).to_be_bytes());
  }
}

impl<'a, 'p> SeqEncoder<'a, 'p> {
  fn new(ser: &'a mut MsgpackSerializer<'p>, len: Option<usize>) -> Self {
    if let Some(n) = len {
      write_array_header(n, &mut ser.buf);
    }
    SeqEncoder {
      ser,
      len,
      items_buf: Vec::new(),
      count: 0,
    }
  }

  fn serialize_item<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    self.count += 1;
    if self.len.is_some() {
      value.serialize(&mut *self.ser)?;
    } else {
      // Buffer into items_buf
      let old_len = self.ser.buf.len();
      value.serialize(&mut *self.ser)?;
      let written = self.ser.buf[old_len..].to_vec();
      self.ser.buf.truncate(old_len);
      self.items_buf.extend_from_slice(&written);
    }
    Ok(())
  }

  fn finalize(self) {
    if self.len.is_none() {
      let mut header = Vec::new();
      write_array_header(self.count, &mut header);
      self.ser.buf.extend_from_slice(&header);
      self.ser.buf.extend_from_slice(&self.items_buf);
    }
  }
}

impl<'a, 'p> SerializeSeq for SeqEncoder<'a, 'p> {
  type Ok = ();
  type Error = Error;
  fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    self.serialize_item(value)
  }
  fn end(self) -> Result<()> {
    self.finalize();
    Ok(())
  }
}

impl<'a, 'p> SerializeTuple for SeqEncoder<'a, 'p> {
  type Ok = ();
  type Error = Error;
  fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    self.serialize_item(value)
  }
  fn end(self) -> Result<()> {
    self.finalize();
    Ok(())
  }
}

impl<'a, 'p> SerializeTupleStruct for SeqEncoder<'a, 'p> {
  type Ok = ();
  type Error = Error;
  fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    self.serialize_item(value)
  }
  fn end(self) -> Result<()> {
    self.finalize();
    Ok(())
  }
}

impl<'a, 'p> SerializeTupleVariant for SeqEncoder<'a, 'p> {
  type Ok = ();
  type Error = Error;
  fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    self.serialize_item(value)
  }
  fn end(self) -> Result<()> {
    self.finalize();
    Ok(())
  }
}

// ─── Map encoder ────────────────────────────────────────────────────────────

pub struct MapEncoder<'a, 'p> {
  ser: &'a mut MsgpackSerializer<'p>,
  items: Vec<u8>,
  count: usize,
  // Header placeholder position in ser.buf
  header_pos: usize,
}

impl<'a, 'p> MapEncoder<'a, 'p> {
  fn new(ser: &'a mut MsgpackSerializer<'p>) -> Self {
    // Reserve 3 bytes for map16 header (will be updated on end)
    let header_pos = ser.buf.len();
    ser.buf.push(0xde);
    ser.buf.push(0x00);
    ser.buf.push(0x00);
    MapEncoder {
      ser,
      items: Vec::new(),
      count: 0,
      header_pos,
    }
  }
}

impl<'a, 'p> SerializeMap for MapEncoder<'a, 'p> {
  type Ok = ();
  type Error = Error;

  fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
    let old = self.ser.buf.len();
    key.serialize(&mut *self.ser)?;
    let written = self.ser.buf[old..].to_vec();
    self.ser.buf.truncate(old);
    self.items.extend_from_slice(&written);
    Ok(())
  }
  fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
    let old = self.ser.buf.len();
    value.serialize(&mut *self.ser)?;
    let written = self.ser.buf[old..].to_vec();
    self.ser.buf.truncate(old);
    self.items.extend_from_slice(&written);
    self.count += 1;
    Ok(())
  }
  fn end(self) -> Result<()> {
    if self.count > 0xffff {
      return Err(Error::TooLarge("Map has more than 65535 entries".into()));
    }
    // Update the map16 header
    let n = self.count as u16;
    self.ser.buf[self.header_pos + 1] = (n >> 8) as u8;
    self.ser.buf[self.header_pos + 2] = (n & 0xff) as u8;
    self.ser.buf.extend_from_slice(&self.items);
    Ok(())
  }
}

// ─── Struct encoder ──────────────────────────────────────────────────────────

pub struct StructEncdr<'a, 'p> {
  ser: &'a mut MsgpackSerializer<'p>,
  record: RecordEncoder,
  use_records: bool,
  header_pos: usize,
  field_count: usize,
}

impl<'a, 'p> StructEncdr<'a, 'p> {
  fn new(ser: &'a mut MsgpackSerializer<'p>, _name: &'static str) -> Self {
    let use_records = ser.packer.options.use_records;
    let header_pos = if !use_records {
      let hp = ser.buf.len();
      // Pre-write map16 header (will fill in count on end)
      ser.buf.push(0xde);
      ser.buf.push(0x00);
      ser.buf.push(0x00);
      hp
    } else {
      0
    };
    StructEncdr {
      ser,
      record: RecordEncoder::new(),
      use_records,
      header_pos,
      field_count: 0,
    }
  }
}

impl<'a, 'p> SerializeStruct for StructEncdr<'a, 'p> {
  type Ok = ();
  type Error = Error;

  fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<()> {
    if self.use_records {
      // Buffer the encoded value into a temporary Vec
      let saved_len = self.ser.buf.len();
      value.serialize(&mut *self.ser)?;
      let value_bytes = self.ser.buf[saved_len..].to_vec();
      self.ser.buf.truncate(saved_len);
      self.record.add_field(key, &value_bytes);
    } else {
      // Write key as string, then serialize value directly into ser.buf
      encode_str(key, &mut self.ser.buf);
      value.serialize(&mut *self.ser)?;
      self.field_count += 1;
    }
    Ok(())
  }

  fn end(self) -> Result<()> {
    if self.use_records {
      self.record.flush(self.ser.packer, &mut self.ser.buf)
    } else {
      let count = self.field_count;
      if count > 0xffff {
        return Err(Error::TooLarge("Struct has more than 65535 fields".into()));
      }
      let n = count as u16;
      self.ser.buf[self.header_pos + 1] = (n >> 8) as u8;
      self.ser.buf[self.header_pos + 2] = (n & 0xff) as u8;
      Ok(())
    }
  }
}

impl<'a, 'p> SerializeStructVariant for StructEncdr<'a, 'p> {
  type Ok = ();
  type Error = Error;
  fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<()> {
    SerializeStruct::serialize_field(self, key, value)
  }
  fn end(self) -> Result<()> {
    SerializeStruct::end(self)
  }
}
