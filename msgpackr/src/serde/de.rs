use crate::error::{Error, Result};
use crate::value::Value;
use serde::de::{self, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

/// Serde deserializer backed by a msgpack `Value`.
pub struct MsgpackDeserializer {
  value: Value,
}

impl MsgpackDeserializer {
  pub fn new(value: Value) -> Self {
    MsgpackDeserializer { value }
  }
}

impl<'de> Deserializer<'de> for MsgpackDeserializer {
  type Error = Error;

  fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Nil => visitor.visit_unit(),
      Value::Bool(b) => visitor.visit_bool(b),
      Value::Int(n) => visitor.visit_i64(n),
      Value::UInt(n) => visitor.visit_u64(n),
      Value::F32(f) => visitor.visit_f32(f),
      Value::F64(f) => visitor.visit_f64(f),
      Value::Str(s) => visitor.visit_string(s),
      Value::Bin(b) => visitor.visit_byte_buf(b),
      Value::Array(arr) => visitor.visit_seq(SeqDeserializer::new(arr)),
      Value::Map(pairs) => visitor.visit_map(MapDeserializer::new(pairs)),
      Value::Undefined => visitor.visit_unit(),
      Value::Timestamp { seconds, nanos } => visitor.visit_map(MapDeserializer::new(vec![
        (Value::Str("seconds".into()), Value::Int(seconds)),
        (Value::Str("nanos".into()), Value::UInt(nanos as u64)),
      ])),
      #[cfg(feature = "bigint")]
      Value::BigInt(n) => visitor.visit_string(n.to_string()),
      Value::Ext(type_code, data) => visitor.visit_map(MapDeserializer::new(vec![
        (Value::Str("type_code".into()), Value::Int(type_code as i64)),
        (Value::Str("data".into()), Value::Bin(data)),
      ])),
      other => visitor.visit_unit(),
    }
  }

  fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Bool(b) => visitor.visit_bool(b),
      Value::Nil => visitor.visit_bool(false),
      Value::Int(0) | Value::UInt(0) => visitor.visit_bool(false),
      Value::Int(_) | Value::UInt(_) => visitor.visit_bool(true),
      v => Err(Error::invalid(format!("expected bool, got {:?}", v))),
    }
  }

  fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_i64(&self.value)?;
    visitor.visit_i8(n as i8)
  }
  fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_i64(&self.value)?;
    visitor.visit_i16(n as i16)
  }
  fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_i64(&self.value)?;
    visitor.visit_i32(n as i32)
  }
  fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_i64(&self.value)?;
    visitor.visit_i64(n)
  }
  fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_u64(&self.value)?;
    visitor.visit_u8(n as u8)
  }
  fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_u64(&self.value)?;
    visitor.visit_u16(n as u16)
  }
  fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_u64(&self.value)?;
    visitor.visit_u32(n as u32)
  }
  fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let n = extract_u64(&self.value)?;
    visitor.visit_u64(n)
  }
  fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let f = match &self.value {
      Value::F32(f) => *f,
      Value::F64(f) => *f as f32,
      Value::Int(n) => *n as f32,
      Value::UInt(n) => *n as f32,
      v => return Err(Error::invalid(format!("expected float, got {:?}", v))),
    };
    visitor.visit_f32(f)
  }
  fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    let f = match &self.value {
      Value::F64(f) => *f,
      Value::F32(f) => *f as f64,
      Value::Int(n) => *n as f64,
      Value::UInt(n) => *n as f64,
      v => return Err(Error::invalid(format!("expected float, got {:?}", v))),
    };
    visitor.visit_f64(f)
  }
  fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Str(s) => {
        let mut chars = s.chars();
        let c = chars
          .next()
          .ok_or_else(|| Error::invalid("empty string for char"))?;
        if chars.next().is_some() {
          return Err(Error::invalid("string has more than one char"));
        }
        visitor.visit_char(c)
      }
      v => Err(Error::invalid(format!("expected char, got {:?}", v))),
    }
  }
  fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Str(s) => visitor.visit_string(s),
      v => Err(Error::invalid(format!("expected string, got {:?}", v))),
    }
  }
  fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    self.deserialize_str(visitor)
  }
  fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Bin(b) => visitor.visit_byte_buf(b),
      Value::Str(s) => visitor.visit_string(s),
      v => Err(Error::invalid(format!("expected bytes, got {:?}", v))),
    }
  }
  fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    self.deserialize_bytes(visitor)
  }
  fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Nil | Value::Undefined => visitor.visit_none(),
      v => visitor.visit_some(MsgpackDeserializer::new(v)),
    }
  }
  fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    visitor.visit_unit()
  }
  fn deserialize_unit_struct<V: Visitor<'de>>(
    self,
    _name: &'static str,
    visitor: V,
  ) -> Result<V::Value> {
    visitor.visit_unit()
  }
  fn deserialize_newtype_struct<V: Visitor<'de>>(
    self,
    _name: &'static str,
    visitor: V,
  ) -> Result<V::Value> {
    visitor.visit_newtype_struct(self)
  }
  fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Array(arr) => visitor.visit_seq(SeqDeserializer::new(arr)),
      v => Err(Error::invalid(format!("expected array, got {:?}", v))),
    }
  }
  fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
    self.deserialize_seq(visitor)
  }
  fn deserialize_tuple_struct<V: Visitor<'de>>(
    self,
    _name: &'static str,
    _len: usize,
    visitor: V,
  ) -> Result<V::Value> {
    self.deserialize_seq(visitor)
  }
  fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Map(pairs) => visitor.visit_map(MapDeserializer::new(pairs)),
      v => Err(Error::invalid(format!("expected map, got {:?}", v))),
    }
  }
  fn deserialize_struct<V: Visitor<'de>>(
    self,
    _name: &'static str,
    _fields: &'static [&'static str],
    visitor: V,
  ) -> Result<V::Value> {
    self.deserialize_map(visitor)
  }
  fn deserialize_enum<V: Visitor<'de>>(
    self,
    _name: &'static str,
    _variants: &'static [&'static str],
    visitor: V,
  ) -> Result<V::Value> {
    match self.value {
      Value::Str(s) => visitor.visit_enum(StringEnumDeserializer(s)),
      Value::Array(mut arr) => {
        if arr.len() != 2 {
          return Err(Error::invalid("enum must be [variant, value]"));
        }
        let variant = match arr.remove(0) {
          Value::Str(s) => s,
          v => {
            return Err(Error::invalid(format!(
              "enum variant must be string, got {:?}",
              v
            )))
          }
        };
        visitor.visit_enum(EnumDeserializer {
          variant,
          value: arr.remove(0),
        })
      }
      v => Err(Error::invalid(format!("expected enum, got {:?}", v))),
    }
  }
  fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    self.deserialize_str(visitor)
  }
  fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
    visitor.visit_unit()
  }
}

// ─── Sequence deserializer ───────────────────────────────────────────────────

struct SeqDeserializer {
  items: std::vec::IntoIter<Value>,
}

impl SeqDeserializer {
  fn new(items: Vec<Value>) -> Self {
    SeqDeserializer {
      items: items.into_iter(),
    }
  }
}

impl<'de> SeqAccess<'de> for SeqDeserializer {
  type Error = Error;

  fn next_element_seed<T: de::DeserializeSeed<'de>>(
    &mut self,
    seed: T,
  ) -> Result<Option<T::Value>> {
    match self.items.next() {
      Some(v) => seed.deserialize(MsgpackDeserializer::new(v)).map(Some),
      None => Ok(None),
    }
  }
}

// ─── Map deserializer ────────────────────────────────────────────────────────

struct MapDeserializer {
  pairs: std::vec::IntoIter<(Value, Value)>,
  next_value: Option<Value>,
}

impl MapDeserializer {
  fn new(pairs: Vec<(Value, Value)>) -> Self {
    MapDeserializer {
      pairs: pairs.into_iter(),
      next_value: None,
    }
  }
}

impl<'de> MapAccess<'de> for MapDeserializer {
  type Error = Error;

  fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
    match self.pairs.next() {
      Some((k, v)) => {
        self.next_value = Some(v);
        seed.deserialize(MsgpackDeserializer::new(k)).map(Some)
      }
      None => Ok(None),
    }
  }
  fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
    let v = self
      .next_value
      .take()
      .ok_or_else(|| Error::invalid("value called before key"))?;
    seed.deserialize(MsgpackDeserializer::new(v))
  }
}

// ─── Enum deserializer ───────────────────────────────────────────────────────

struct EnumDeserializer {
  variant: String,
  value: Value,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
  type Error = Error;
  type Variant = VariantDeserializer;

  fn variant_seed<V: de::DeserializeSeed<'de>>(
    self,
    seed: V,
  ) -> Result<(V::Value, VariantDeserializer)> {
    let v = seed.deserialize(MsgpackDeserializer::new(Value::Str(self.variant)))?;
    Ok((v, VariantDeserializer { value: self.value }))
  }
}

struct VariantDeserializer {
  value: Value,
}

impl<'de> VariantAccess<'de> for VariantDeserializer {
  type Error = Error;

  fn unit_variant(self) -> Result<()> {
    Ok(())
  }
  fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
    seed.deserialize(MsgpackDeserializer::new(self.value))
  }
  fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
    match self.value {
      Value::Array(arr) => visitor.visit_seq(SeqDeserializer::new(arr)),
      v => Err(Error::invalid(format!(
        "expected array for tuple variant, got {:?}",
        v
      ))),
    }
  }
  fn struct_variant<V: Visitor<'de>>(
    self,
    _fields: &'static [&'static str],
    visitor: V,
  ) -> Result<V::Value> {
    match self.value {
      Value::Map(pairs) => visitor.visit_map(MapDeserializer::new(pairs)),
      v => Err(Error::invalid(format!(
        "expected map for struct variant, got {:?}",
        v
      ))),
    }
  }
}

// ─── String-unit-enum helper ─────────────────────────────────────────────────

struct StringEnumDeserializer(String);

impl<'de> EnumAccess<'de> for StringEnumDeserializer {
  type Error = Error;
  type Variant = UnitOnlyVariantDeserializer;

  fn variant_seed<V: de::DeserializeSeed<'de>>(
    self,
    seed: V,
  ) -> Result<(V::Value, UnitOnlyVariantDeserializer)> {
    let v = seed.deserialize(MsgpackDeserializer::new(Value::Str(self.0)))?;
    Ok((v, UnitOnlyVariantDeserializer))
  }
}

struct UnitOnlyVariantDeserializer;

impl<'de> VariantAccess<'de> for UnitOnlyVariantDeserializer {
  type Error = Error;
  fn unit_variant(self) -> Result<()> {
    Ok(())
  }
  fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, _seed: T) -> Result<T::Value> {
    Err(Error::invalid("expected unit variant"))
  }
  fn tuple_variant<V: Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value> {
    Err(Error::invalid("expected unit variant"))
  }
  fn struct_variant<V: Visitor<'de>>(
    self,
    _fields: &'static [&'static str],
    _visitor: V,
  ) -> Result<V::Value> {
    Err(Error::invalid("expected unit variant"))
  }
}

// ─── Integer extraction helpers ─────────────────────────────────────────────

fn extract_i64(v: &Value) -> Result<i64> {
  match v {
    Value::Int(n) => Ok(*n),
    Value::UInt(n) => {
      if *n > i64::MAX as u64 {
        return Err(Error::invalid("uint64 out of range for i64"));
      }
      Ok(*n as i64)
    }
    Value::F32(f) => Ok(*f as i64),
    Value::F64(f) => Ok(*f as i64),
    other => Err(Error::invalid(format!("expected integer, got {:?}", other))),
  }
}

fn extract_u64(v: &Value) -> Result<u64> {
  match v {
    Value::UInt(n) => Ok(*n),
    Value::Int(n) => {
      if *n < 0 {
        return Err(Error::invalid("negative integer for unsigned"));
      }
      Ok(*n as u64)
    }
    Value::F32(f) => Ok(*f as u64),
    Value::F64(f) => Ok(*f as u64),
    other => Err(Error::invalid(format!(
      "expected unsigned integer, got {:?}",
      other
    ))),
  }
}
