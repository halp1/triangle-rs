use std::fmt;

/// All possible MessagePack values, including msgpackr extensions.
#[derive(Debug, Clone)]
pub enum Value {
  Nil,
  Bool(bool),
  /// Signed integer (fits in i64)
  Int(i64),
  /// Unsigned integer (does not fit in i64 as positive)
  UInt(u64),
  F32(f32),
  F64(f64),
  Str(String),
  Bin(Vec<u8>),
  Array(Vec<Value>),
  /// Ordered key-value pairs (msgpack map)
  Map(Vec<(Value, Value)>),
  /// Raw extension: (type_code, data)
  Ext(i8, Vec<u8>),
  /// Timestamp extension (type 0xff)
  Timestamp {
    seconds: i64,
    nanos: u32,
  },
  /// Undefined — encoded as fixext1(type=0, data=0) in msgpackr
  Undefined,
  /// msgpackr moreTypes: JS Set — encoded as fixext1(type=0x73) + array
  Set(Vec<Value>),
  /// msgpackr moreTypes: Error — encoded as fixext1(type=0x65) + [name, message, cause]
  MsgpackError {
    name: String,
    message: String,
    cause: Option<Box<Value>>,
  },
  /// msgpackr moreTypes: RegExp — encoded as fixext1(type=0x78) + [source, flags]
  Regex {
    source: String,
    flags: String,
  },
  /// msgpackr moreTypes: TypedArray — encoded as ext(type=0x74, data=[type_code, ...bytes])
  TypedArray {
    type_code: u8,
    data: Vec<u8>,
  },
  /// msgpackr moreTypes: ArrayBuffer — encoded as ext(type=0x74, data=[0x10, ...bytes])
  ArrayBuffer(Vec<u8>),
  /// msgpackr moreTypes: DataView — encoded as ext(type=0x74, data=[0x11, ...bytes])
  DataView(Vec<u8>),
  /// msgpackr BigInt extension (type=0x42)
  #[cfg(feature = "bigint")]
  BigInt(num_bigint::BigInt),
}

/// Named TypedArray type codes matching msgpackr
pub mod typed_array_codes {
  pub const INT8: u8 = 0;
  pub const UINT8: u8 = 1;
  pub const UINT8_CLAMPED: u8 = 2;
  pub const INT16: u8 = 3;
  pub const UINT16: u8 = 4;
  pub const INT32: u8 = 5;
  pub const UINT32: u8 = 6;
  pub const FLOAT32: u8 = 7;
  pub const FLOAT64: u8 = 8;
  pub const BIG_INT64: u8 = 9;
  pub const BIG_UINT64: u8 = 10;
  pub const ARRAY_BUFFER: u8 = 16;
  pub const DATA_VIEW: u8 = 17;
}

impl PartialEq for Value {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Value::Nil, Value::Nil) => true,
      (Value::Bool(a), Value::Bool(b)) => a == b,
      (Value::Int(a), Value::Int(b)) => a == b,
      (Value::UInt(a), Value::UInt(b)) => a == b,
      (Value::Int(a), Value::UInt(b)) => *a >= 0 && (*a as u64) == *b,
      (Value::UInt(a), Value::Int(b)) => *b >= 0 && *a == (*b as u64),
      (Value::F32(a), Value::F32(b)) => a == b,
      (Value::F64(a), Value::F64(b)) => a == b,
      (Value::Str(a), Value::Str(b)) => a == b,
      (Value::Bin(a), Value::Bin(b)) => a == b,
      (Value::Array(a), Value::Array(b)) => a == b,
      (Value::Map(a), Value::Map(b)) => a == b,
      (Value::Ext(ta, da), Value::Ext(tb, db)) => ta == tb && da == db,
      (
        Value::Timestamp {
          seconds: s1,
          nanos: n1,
        },
        Value::Timestamp {
          seconds: s2,
          nanos: n2,
        },
      ) => s1 == s2 && n1 == n2,
      (Value::Undefined, Value::Undefined) => true,
      (Value::Set(a), Value::Set(b)) => a == b,
      _ => false,
    }
  }
}

impl fmt::Display for Value {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Value::Nil => write!(f, "null"),
      Value::Bool(b) => write!(f, "{}", b),
      Value::Int(n) => write!(f, "{}", n),
      Value::UInt(n) => write!(f, "{}", n),
      Value::F32(v) => write!(f, "{}", v),
      Value::F64(v) => write!(f, "{}", v),
      Value::Str(s) => write!(f, "{:?}", s),
      Value::Bin(b) => write!(f, "<binary {} bytes>", b.len()),
      Value::Array(arr) => {
        write!(f, "[")?;
        for (i, v) in arr.iter().enumerate() {
          if i > 0 {
            write!(f, ", ")?;
          }
          write!(f, "{}", v)?;
        }
        write!(f, "]")
      }
      Value::Map(m) => {
        write!(f, "{{")?;
        for (i, (k, v)) in m.iter().enumerate() {
          if i > 0 {
            write!(f, ", ")?;
          }
          write!(f, "{}: {}", k, v)?;
        }
        write!(f, "}}")
      }
      Value::Ext(t, d) => write!(f, "ext({}, {} bytes)", t, d.len()),
      Value::Timestamp { seconds, nanos } => write!(f, "timestamp({}.{})", seconds, nanos),
      Value::Undefined => write!(f, "undefined"),
      Value::Set(_) => write!(f, "Set(...)"),
      Value::MsgpackError { name, message, .. } => write!(f, "{}({})", name, message),
      Value::Regex { source, flags } => write!(f, "/{}/{}", source, flags),
      Value::TypedArray { type_code, data } => {
        write!(f, "TypedArray({}, {} bytes)", type_code, data.len())
      }
      Value::ArrayBuffer(d) => write!(f, "ArrayBuffer({} bytes)", d.len()),
      Value::DataView(d) => write!(f, "DataView({} bytes)", d.len()),
      #[cfg(feature = "bigint")]
      Value::BigInt(n) => write!(f, "{}n", n),
    }
  }
}

// Convenience From impls
impl From<()> for Value {
  fn from(_: ()) -> Self {
    Value::Nil
  }
}
impl From<bool> for Value {
  fn from(v: bool) -> Self {
    Value::Bool(v)
  }
}
impl From<i8> for Value {
  fn from(v: i8) -> Self {
    Value::Int(v as i64)
  }
}
impl From<i16> for Value {
  fn from(v: i16) -> Self {
    Value::Int(v as i64)
  }
}
impl From<i32> for Value {
  fn from(v: i32) -> Self {
    Value::Int(v as i64)
  }
}
impl From<i64> for Value {
  fn from(v: i64) -> Self {
    Value::Int(v)
  }
}
impl From<u8> for Value {
  fn from(v: u8) -> Self {
    Value::UInt(v as u64)
  }
}
impl From<u16> for Value {
  fn from(v: u16) -> Self {
    Value::UInt(v as u64)
  }
}
impl From<u32> for Value {
  fn from(v: u32) -> Self {
    Value::UInt(v as u64)
  }
}
impl From<u64> for Value {
  fn from(v: u64) -> Self {
    Value::UInt(v)
  }
}
impl From<f32> for Value {
  fn from(v: f32) -> Self {
    Value::F32(v)
  }
}
impl From<f64> for Value {
  fn from(v: f64) -> Self {
    Value::F64(v)
  }
}
impl From<String> for Value {
  fn from(v: String) -> Self {
    Value::Str(v)
  }
}
impl From<&str> for Value {
  fn from(v: &str) -> Self {
    Value::Str(v.to_owned())
  }
}
impl From<Vec<u8>> for Value {
  fn from(v: Vec<u8>) -> Self {
    Value::Bin(v)
  }
}
impl From<Vec<Value>> for Value {
  fn from(v: Vec<Value>) -> Self {
    Value::Array(v)
  }
}
impl<V: Into<Value>> From<Option<V>> for Value {
  fn from(v: Option<V>) -> Self {
    match v {
      Some(x) => x.into(),
      None => Value::Nil,
    }
  }
}

impl Value {
  pub fn is_nil(&self) -> bool {
    matches!(self, Value::Nil)
  }
  pub fn as_str(&self) -> Option<&str> {
    if let Value::Str(s) = self {
      Some(s)
    } else {
      None
    }
  }
  pub fn as_i64(&self) -> Option<i64> {
    match self {
      Value::Int(n) => Some(*n),
      Value::UInt(n) if *n <= i64::MAX as u64 => Some(*n as i64),
      _ => None,
    }
  }
  pub fn as_u64(&self) -> Option<u64> {
    match self {
      Value::UInt(n) => Some(*n),
      Value::Int(n) if *n >= 0 => Some(*n as u64),
      _ => None,
    }
  }
  pub fn as_f64(&self) -> Option<f64> {
    match self {
      Value::F32(f) => Some(*f as f64),
      Value::F64(f) => Some(*f),
      _ => None,
    }
  }
  pub fn as_array(&self) -> Option<&[Value]> {
    if let Value::Array(a) = self {
      Some(a)
    } else {
      None
    }
  }
  pub fn as_map(&self) -> Option<&[(Value, Value)]> {
    if let Value::Map(m) = self {
      Some(m)
    } else {
      None
    }
  }
  pub fn as_bytes(&self) -> Option<&[u8]> {
    if let Value::Bin(b) = self {
      Some(b)
    } else {
      None
    }
  }

  /// Convert integer Value to the most compact integer type for display
  pub fn normalize_int(self) -> Self {
    match self {
      Value::UInt(n) if n <= i64::MAX as u64 => Value::Int(n as i64),
      other => other,
    }
  }
}
