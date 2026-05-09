use msgpackr::options::{Float32Mode, PackOptions, UnpackOptions};
use msgpackr::value::Value;
use msgpackr::Packer;
use msgpackr::{pack, pack_with_opts, unpack, unpack_multiple, unpack_opts};

/// Load a generated fixture's .msgpack bytes.
fn fixture(name: &str) -> Vec<u8> {
  let manifest_path = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/generated/manifest.json"
  );
  let manifest_bytes =
    std::fs::read(manifest_path).expect("manifest.json not found — run gen_fixtures.js first");
  let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
  let entries = manifest.as_array().unwrap();
  let entry = entries
    .iter()
    .find(|e| {
      let n = e["name"].as_str().unwrap();
      n.contains(name)
    })
    .unwrap_or_else(|| panic!("Fixture '{}' not found in manifest", name));
  let filename = entry["name"].as_str().unwrap();
  let path = format!(
    "{}/tests/fixtures/generated/{}.msgpack",
    env!("CARGO_MANIFEST_DIR"),
    filename
  );
  std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", path, e))
}

fn roundtrip(val: &Value) -> Value {
  let encoded = pack(val).expect("pack failed");
  unpack(&encoded).expect("unpack failed")
}

fn roundtrip_records(val: &Value) -> Value {
  let mut packer = Packer::new();
  let encoded = packer.pack_value(val).expect("pack failed");
  packer.unpack_value(&encoded).expect("unpack failed")
}

// ─── Basic types ────────────────────────────────────────────────────────────

#[test]
fn test_nil() {
  let bytes = fixture("nil");
  assert_eq!(unpack(&bytes).unwrap(), Value::Nil);
  assert_eq!(roundtrip(&Value::Nil), Value::Nil);
}

#[test]
fn test_bool_true() {
  let bytes = fixture("bool_true");
  assert_eq!(unpack(&bytes).unwrap(), Value::Bool(true));
}

#[test]
fn test_bool_false() {
  let bytes = fixture("bool_false");
  assert_eq!(unpack(&bytes).unwrap(), Value::Bool(false));
}

#[test]
fn test_bool_roundtrip() {
  assert_eq!(roundtrip(&Value::Bool(true)), Value::Bool(true));
  assert_eq!(roundtrip(&Value::Bool(false)), Value::Bool(false));
}

// ─── Integers ────────────────────────────────────────────────────────────────

#[test]
fn test_integers_from_fixtures() {
  let cases: &[(i64, &str)] = &[
    (0, "int_0"),
    (1, "int_1"),
    (31, "int_31"),
    (32, "int_32"),
    (63, "int_63"),
    (64, "int_64"),
    (127, "int_127"),
    (128, "int_128"),
    (255, "int_255"),
    (256, "int_256"),
    (-1, "int_neg1"),
    (-32, "int_neg32"),
    (-33, "int_neg33"),
    (-128, "int_neg128"),
    (-129, "int_neg129"),
    (-32768, "int_neg32768"),
    (-32769, "int_neg32769"),
    (-2147483648, "int_neg2147483648"),
  ];
  for (expected, name) in cases {
    let bytes = fixture(name);
    let val = unpack(&bytes).unwrap();
    assert!(
      val.as_i64() == Some(*expected) || val.as_u64() == Some(*expected as u64),
      "Fixture {}: expected {}, got {:?}",
      name,
      expected,
      val
    );
  }
}

#[test]
fn test_integer_roundtrip() {
  let cases: &[i64] = &[
    0,
    1,
    31,
    32,
    63,
    64,
    127,
    128,
    255,
    256,
    i16::MAX as i64,
    i16::MIN as i64,
    i32::MAX as i64,
    i32::MIN as i64,
    i64::MAX,
    i64::MIN,
    -1,
    -32,
    -128,
    -129,
    -32768,
  ];
  for &n in cases {
    let val = Value::Int(n);
    let rt = roundtrip(&val);
    assert!(
      rt.as_i64() == Some(n) || rt.as_u64().map(|u| u as i64) == Some(n),
      "roundtrip failed for {}: got {:?}",
      n,
      rt
    );
  }
}

#[test]
fn test_integer_encoding_compatibility() {
  // Without records: integers 0-127 use positive fixint
  let bytes_64_no_records = pack(&Value::UInt(64)).unwrap();
  assert_eq!(
    bytes_64_no_records,
    vec![0x40],
    "64 without records must be fixint 0x40"
  );

  let bytes_127_no_records = pack(&Value::UInt(127)).unwrap();
  assert_eq!(
    bytes_127_no_records,
    vec![0x7f],
    "127 without records must be fixint 0x7f"
  );

  // With records: integers 64-127 must use uint8 (0xcc)
  let mut packer = Packer::new();
  let bytes_64_records = packer.pack_value(&Value::UInt(64)).unwrap();
  assert_eq!(
    bytes_64_records,
    vec![0xcc, 0x40],
    "64 with records must be 0xcc 0x40"
  );

  let bytes_127_records = packer.pack_value(&Value::UInt(127)).unwrap();
  assert_eq!(
    bytes_127_records,
    vec![0xcc, 0x7f],
    "127 with records must be 0xcc 0x7f"
  );

  // 0-63 always fixint regardless of records
  let bytes_63_records = packer.pack_value(&Value::UInt(63)).unwrap();
  assert_eq!(
    bytes_63_records,
    vec![0x3f],
    "63 with records must be fixint 0x3f"
  );

  // Verify fixture compatibility
  let f64 = fixture("norecords_int_64");
  assert_eq!(f64, vec![0x40]);
  let f64r = fixture("records_int_64");
  assert_eq!(f64r, vec![0xcc, 0x40]);
}

// ─── Strings ─────────────────────────────────────────────────────────────────

#[test]
fn test_string_empty() {
  let bytes = fixture("str_empty");
  assert_eq!(unpack(&bytes).unwrap(), Value::Str("".to_owned()));
}

#[test]
fn test_string_hello() {
  let bytes = fixture("str_hello");
  assert_eq!(unpack(&bytes).unwrap(), Value::Str("hello".to_owned()));
}

#[test]
fn test_string_31() {
  let bytes = fixture("str_31");
  assert_eq!(unpack(&bytes).unwrap(), Value::Str("a".repeat(31)));
  assert_eq!(bytes[0], 0xbf, "31-char string must use fixstr 0xbf");
}

#[test]
fn test_string_32() {
  let bytes = fixture("str_32");
  assert_eq!(unpack(&bytes).unwrap(), Value::Str("a".repeat(32)));
  assert_eq!(bytes[0], 0xd9, "32-char string must use str8 0xd9");
}

#[test]
fn test_string_256() {
  let bytes = fixture("str_256");
  assert_eq!(unpack(&bytes).unwrap(), Value::Str("a".repeat(256)));
  assert_eq!(bytes[0], 0xda, "256-char string must use str16 0xda");
}

#[test]
fn test_string_unicode() {
  let bytes = fixture("str_unicode");
  let val = unpack(&bytes).unwrap();
  assert_eq!(val, Value::Str("héllo wörld 日本語 🎉".to_owned()));
}

#[test]
fn test_string_roundtrip() {
  let s31 = "a".repeat(31);
  let s256 = "a".repeat(256);
  let cases = ["", "hello", s31.as_str(), s256.as_str(), "héllo 日本語"];
  for s in &cases {
    let val = Value::Str(s.to_string());
    let rt = roundtrip(&val);
    assert_eq!(rt, val, "roundtrip failed for string of len {}", s.len());
  }
}

// ─── Binary ──────────────────────────────────────────────────────────────────

#[test]
fn test_bin_empty() {
  let bytes = fixture("bin_empty");
  assert_eq!(unpack(&bytes).unwrap(), Value::Bin(vec![]));
}

#[test]
fn test_bin_small() {
  let bytes = fixture("bin_small");
  assert_eq!(
    unpack(&bytes).unwrap(),
    Value::Bin(vec![0xde, 0xad, 0xbe, 0xef])
  );
}

#[test]
fn test_bin_roundtrip() {
  let data = vec![0u8, 1, 2, 3, 0xff, 0xfe];
  let val = Value::Bin(data.clone());
  assert_eq!(roundtrip(&val), Value::Bin(data));
}

// ─── Arrays ──────────────────────────────────────────────────────────────────

#[test]
fn test_array_empty() {
  let bytes = fixture("array_empty");
  assert_eq!(unpack(&bytes).unwrap(), Value::Array(vec![]));
}

#[test]
fn test_array_small() {
  let bytes = fixture("array_small");
  assert_eq!(
    unpack(&bytes).unwrap(),
    Value::Array(vec![Value::UInt(1), Value::UInt(2), Value::UInt(3)])
  );
}

#[test]
fn test_array_15() {
  let bytes = fixture("array_15");
  assert_eq!(bytes[0], 0x9f, "15-element array must use fixarray 0x9f");
  let val = unpack(&bytes).unwrap();
  assert!(matches!(val, Value::Array(arr) if arr.len() == 15));
}

#[test]
fn test_array_16() {
  let bytes = fixture("array_16");
  assert_eq!(bytes[0], 0xdc, "16-element array must use array16 0xdc");
  let val = unpack(&bytes).unwrap();
  assert!(matches!(val, Value::Array(arr) if arr.len() == 16));
}

#[test]
fn test_array_nested() {
  let bytes = fixture("array_nested");
  let val = unpack(&bytes).unwrap();
  assert!(matches!(val, Value::Array(arr) if arr.len() == 2));
}

// ─── Maps ────────────────────────────────────────────────────────────────────

#[test]
fn test_map_empty() {
  let bytes = fixture("map_empty");
  assert_eq!(unpack(&bytes).unwrap(), Value::Map(vec![]));
}

#[test]
fn test_map_small() {
  let bytes = fixture("map_small");
  let val = unpack(&bytes).unwrap();
  assert!(matches!(val, Value::Map(pairs) if pairs.len() == 2));
}

#[test]
fn test_map_15_keys() {
  // msgpackr always uses map16 (0xde) for plain objects (to avoid two-pass counting)
  let bytes = fixture("map_15_keys");
  assert_eq!(bytes[0], 0xde, "msgpackr uses map16 for all plain objects");
  let val = unpack(&bytes).unwrap();
  if let Value::Map(pairs) = val {
    assert_eq!(pairs.len(), 15);
  } else {
    panic!("Expected map with 15 entries");
  }
}

#[test]
fn test_map_16_keys() {
  let bytes = fixture("map_16_keys");
  assert_eq!(bytes[0], 0xde, "16-key map must use map16 0xde");
}

#[test]
fn test_map_roundtrip() {
  let map = Value::Map(vec![
    (Value::Str("a".to_owned()), Value::Int(1)),
    (Value::Str("b".to_owned()), Value::Bool(true)),
    (Value::Str("c".to_owned()), Value::Nil),
  ]);
  assert_eq!(roundtrip(&map), map);
}

// ─── Record extension ────────────────────────────────────────────────────────

#[test]
fn test_records_first_definition() {
  let bytes = fixture("records_first_def");
  let mut packer = Packer::new();
  let val = packer.unpack_value(&bytes).expect("unpack failed");
  assert!(
    matches!(val, Value::Map(_)),
    "decoded record should be a Map, got {:?}",
    val
  );
  if let Value::Map(pairs) = val {
    assert_eq!(pairs.len(), 3, "should have 3 fields");
    let keys: Vec<_> = pairs
      .iter()
      .map(|(k, _)| k.as_str().unwrap_or(""))
      .collect();
    assert!(keys.contains(&"x") && keys.contains(&"y") && keys.contains(&"z"));
  }
}

#[test]
fn test_records_reuse_structure() {
  let bytes1 = fixture("records_first_def");
  let bytes2 = fixture("records_reuse");

  let mut packer = Packer::new();
  let v1 = packer.unpack_value(&bytes1).unwrap();
  let v2 = packer.unpack_value(&bytes2).unwrap();

  if let Value::Map(pairs2) = v2 {
    let keys: Vec<_> = pairs2
      .iter()
      .map(|(k, _)| k.as_str().unwrap_or(""))
      .collect();
    assert!(keys.contains(&"x") && keys.contains(&"y") && keys.contains(&"z"));
  } else {
    panic!("Expected Map, got {:?}", v2);
  }
}

#[test]
fn test_records_array_of_structs() {
  let bytes = fixture("records_array_of_structs");
  let mut packer = Packer::new();
  let val = packer.unpack_value(&bytes).unwrap();
  if let Value::Array(arr) = &val {
    assert_eq!(arr.len(), 3);
    for item in arr {
      assert!(matches!(item, Value::Map(_)));
    }
  } else {
    panic!("Expected array of records, got {:?}", val);
  }
}

#[test]
fn test_records_encode_decode() {
  let mut packer = Packer::new();
  let obj = Value::Map(vec![
    (Value::Str("x".to_owned()), Value::Int(1)),
    (Value::Str("y".to_owned()), Value::Int(2)),
    (Value::Str("z".to_owned()), Value::Int(3)),
  ]);
  let encoded = packer.pack_value(&obj).unwrap();

  let mut packer2 = Packer::new();
  let decoded = packer2.unpack_value(&encoded).unwrap();
  assert_eq!(decoded, obj);
}

#[test]
fn test_records_structure_reuse_is_compact() {
  let mut packer = Packer::new();
  let obj = Value::Map(vec![
    (
      Value::Str("name".to_owned()),
      Value::Str("Alice".to_owned()),
    ),
    (Value::Str("age".to_owned()), Value::UInt(30)),
  ]);

  // First encoding: includes the record definition
  let first = packer.pack_value(&obj).unwrap();

  // Second encoding: should be more compact (just record ID + values)
  let second = packer.pack_value(&obj).unwrap();

  assert!(
    second.len() < first.len(),
    "Second encoding ({} bytes) should be more compact than first ({})",
    second.len(),
    first.len()
  );
}

// ─── Timestamps ──────────────────────────────────────────────────────────────

#[test]
fn test_timestamp_epoch() {
  let bytes = fixture("timestamp_epoch");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(
      val,
      Value::Timestamp {
        seconds: 0,
        nanos: 0
      }
    ),
    "got {:?}",
    val
  );
}

#[test]
fn test_timestamp_now() {
  let bytes = fixture("timestamp_now");
  let val = unpack(&bytes).unwrap();
  if let Value::Timestamp { seconds, nanos: _ } = val {
    // 2023-06-15T12:30:00Z = 1686832200
    assert_eq!(seconds, 1686832200, "timestamp seconds mismatch");
  } else {
    panic!("Expected Timestamp, got {:?}", val);
  }
}

#[test]
fn test_timestamp_millis() {
  let bytes = fixture("timestamp_millis");
  let val = unpack(&bytes).unwrap();
  if let Value::Timestamp { seconds, nanos } = val {
    assert_eq!(seconds, 1686832200, "seconds mismatch");
    assert_eq!(nanos, 123_000_000, "nanos (millis) mismatch");
  } else {
    panic!("Expected Timestamp, got {:?}", val);
  }
}

#[test]
fn test_timestamp_before_epoch() {
  let bytes = fixture("timestamp_before_epoch");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(val, Value::Timestamp { seconds, .. } if seconds < 0),
    "Expected negative seconds for pre-epoch date, got {:?}",
    val
  );
}

#[test]
fn test_timestamp_roundtrip() {
  let ts = Value::Timestamp {
    seconds: 1686829800,
    nanos: 123_000_000,
  };
  let opts = PackOptions::default();
  let encoded = pack_with_opts(&ts, &opts).unwrap();
  let decoded = unpack(&encoded).unwrap();
  assert_eq!(decoded, ts);
}

// ─── Floats ──────────────────────────────────────────────────────────────────

#[test]
fn test_float_special_values() {
  let manifest_path = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/generated/manifest.json"
  );
  let manifest_bytes = std::fs::read(manifest_path).unwrap();
  let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
  let entries = manifest.as_array().unwrap();

  let nan_entry = entries
    .iter()
    .find(|e| e["description"].as_str().unwrap().contains("float NaN"))
    .unwrap();
  let nan_bytes = std::fs::read(format!(
    "{}/tests/fixtures/generated/{}.msgpack",
    env!("CARGO_MANIFEST_DIR"),
    nan_entry["name"].as_str().unwrap()
  ))
  .unwrap();
  let nan_val = unpack(&nan_bytes).unwrap();
  match nan_val {
    Value::F64(f) => assert!(f.is_nan(), "Expected NaN"),
    Value::F32(f) => assert!(f.is_nan(), "Expected NaN"),
    other => panic!("Expected float NaN, got {:?}", other),
  }

  let inf_entry = entries
    .iter()
    .find(|e| e["description"].as_str().unwrap() == "float Infinity")
    .unwrap();
  let inf_bytes = std::fs::read(format!(
    "{}/tests/fixtures/generated/{}.msgpack",
    env!("CARGO_MANIFEST_DIR"),
    inf_entry["name"].as_str().unwrap()
  ))
  .unwrap();
  let inf_val = unpack(&inf_bytes).unwrap();
  match inf_val {
    Value::F64(f) => assert!(f.is_infinite() && f > 0.0, "Expected +Inf"),
    Value::F32(f) => assert!(f.is_infinite() && f > 0.0, "Expected +Inf"),
    other => panic!("Expected +Inf, got {:?}", other),
  }

  let ninf_entry = entries
    .iter()
    .find(|e| e["description"].as_str().unwrap() == "float -Infinity")
    .unwrap();
  let ninf_bytes = std::fs::read(format!(
    "{}/tests/fixtures/generated/{}.msgpack",
    env!("CARGO_MANIFEST_DIR"),
    ninf_entry["name"].as_str().unwrap()
  ))
  .unwrap();
  let ninf_val = unpack(&ninf_bytes).unwrap();
  match ninf_val {
    Value::F64(f) => assert!(f.is_infinite() && f < 0.0, "Expected -Inf"),
    Value::F32(f) => assert!(f.is_infinite() && f < 0.0, "Expected -Inf"),
    other => panic!("Expected -Inf, got {:?}", other),
  }
}

#[test]
fn test_float_f32_modes() {
  // Verify that the float32 mode fixtures decode correctly
  let f64_bytes = fixture("float_1_5_f64");
  let f32_bytes = fixture("float_1_5_f32always");
  assert_eq!(f64_bytes[0], 0xcb, "f64 should start with 0xcb");
  assert_eq!(f32_bytes[0], 0xca, "f32 always should start with 0xca");

  let f64_val = unpack(&f64_bytes).unwrap();
  let f32_val = unpack(&f32_bytes).unwrap();
  // Both should decode to ~1.5
  match f64_val {
    Value::F64(f) => assert!((f - 1.5).abs() < 1e-10),
    _ => panic!(),
  }
  match f32_val {
    Value::F32(f) => assert!((f - 1.5f32).abs() < 1e-6),
    _ => panic!(),
  }
}

// ─── More types ──────────────────────────────────────────────────────────────

#[test]
fn test_more_types_set() {
  let bytes = fixture("more_types_set");
  let opts = UnpackOptions {
    maps_as_objects: true,
    ..Default::default()
  };
  let val = unpack_opts(&bytes, &opts).unwrap();
  assert!(matches!(val, Value::Set(_)), "expected Set, got {:?}", val);
  if let Value::Set(items) = val {
    assert_eq!(items.len(), 4);
  }
}

#[test]
fn test_more_types_regexp() {
  let bytes = fixture("more_types_regexp");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(val, Value::Regex { .. }),
    "expected Regex, got {:?}",
    val
  );
  if let Value::Regex { source, flags } = val {
    assert_eq!(source, "hello");
    assert!(flags.contains('g') && flags.contains('i'));
  }
}

#[test]
fn test_more_types_error() {
  let bytes = fixture("more_types_error");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(val, Value::MsgpackError { .. }),
    "expected Error, got {:?}",
    val
  );
  if let Value::MsgpackError { name, message, .. } = val {
    assert_eq!(name, "TypeError");
    assert_eq!(message, "test error");
  }
}

#[test]
fn test_more_types_uint8array() {
  let bytes = fixture("more_types_uint8array");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(val, Value::TypedArray { type_code: 1, .. }),
    "expected Uint8Array, got {:?}",
    val
  );
  if let Value::TypedArray { data, .. } = val {
    assert_eq!(data, vec![1u8, 2, 3, 4, 5]);
  }
}

#[test]
fn test_more_types_arraybuffer() {
  let bytes = fixture("more_types_arraybuffer");
  let val = unpack(&bytes).unwrap();
  assert!(
    matches!(
      val,
      Value::ArrayBuffer(_)
        | Value::TypedArray {
          type_code: 0x10,
          ..
        }
    ),
    "expected ArrayBuffer, got {:?}",
    val
  );
}

// ─── Encode/decode symmetry with Node.js ────────────────────────────────────

#[test]
fn test_encode_matches_node_nil() {
  let rust_bytes = pack(&Value::Nil).unwrap();
  let node_bytes = fixture("nil");
  assert_eq!(rust_bytes, node_bytes, "nil encoding mismatch");
}

#[test]
fn test_encode_matches_node_int_63() {
  let rust_bytes = pack(&Value::UInt(63)).unwrap();
  let node_bytes = fixture("norecords_int_63");
  assert_eq!(rust_bytes, node_bytes, "int 63 encoding mismatch");
}

#[test]
fn test_encode_matches_node_int_64() {
  let rust_bytes = pack(&Value::UInt(64)).unwrap();
  let node_bytes = fixture("norecords_int_64");
  assert_eq!(rust_bytes, node_bytes, "int 64 encoding mismatch");
}

#[test]
fn test_encode_matches_node_int_neg1() {
  let rust_bytes = pack(&Value::Int(-1)).unwrap();
  let node_bytes = fixture("int_neg1");
  assert_eq!(rust_bytes, node_bytes, "int -1 encoding mismatch");
}

#[test]
fn test_encode_matches_node_bool_true() {
  let rust_bytes = pack(&Value::Bool(true)).unwrap();
  let node_bytes = fixture("bool_true");
  assert_eq!(rust_bytes, node_bytes, "bool true encoding mismatch");
}

#[test]
fn test_encode_matches_node_str_hello() {
  let rust_bytes = pack(&Value::Str("hello".to_owned())).unwrap();
  let node_bytes = fixture("str_hello");
  assert_eq!(rust_bytes, node_bytes, "str 'hello' encoding mismatch");
}

#[test]
fn test_encode_matches_node_array_small() {
  let arr = Value::Array(vec![Value::UInt(1), Value::UInt(2), Value::UInt(3)]);
  let rust_bytes = pack(&arr).unwrap();
  let node_bytes = fixture("array_small");
  assert_eq!(rust_bytes, node_bytes, "small array encoding mismatch");
}

// ─── Multi-value decode ──────────────────────────────────────────────────────

#[test]
fn test_multi_value_decode() {
  let bytes = fixture("multi_values");
  let vals = unpack_multiple(&bytes).unwrap();
  assert_eq!(vals.len(), 3, "expected 3 values, got {}", vals.len());
  assert_eq!(vals[0], Value::UInt(1));
  assert_eq!(vals[1], Value::Str("hello".to_owned()));
  assert!(matches!(&vals[2], Value::Array(arr) if arr.len() == 3));
}

// ─── Deep nesting ────────────────────────────────────────────────────────────

#[test]
fn test_deep_nesting() {
  let bytes = fixture("deep_nesting");
  let val = unpack(&bytes).unwrap();
  assert!(matches!(val, Value::Map(_)));
}

// ─── Large array ─────────────────────────────────────────────────────────────

#[test]
fn test_large_array() {
  let bytes = fixture("large_array_1000");
  let val = unpack(&bytes).unwrap();
  if let Value::Array(arr) = val {
    assert_eq!(arr.len(), 1000);
  } else {
    panic!("Expected array");
  }
}

// ─── Undefined ───────────────────────────────────────────────────────────────

#[test]
fn test_undefined_fixture() {
  let bytes = fixture("undefined_value");
  let val = unpack(&bytes).unwrap();
  assert_eq!(val, Value::Undefined, "expected Undefined, got {:?}", val);
}

#[test]
fn test_undefined_roundtrip() {
  let encoded = pack(&Value::Undefined).unwrap();
  let decoded = unpack(&encoded).unwrap();
  assert_eq!(decoded, Value::Undefined);
}

// ─── Serde integration ───────────────────────────────────────────────────────

#[cfg(test)]
mod serde_tests {
  use super::*;
  use msgpackr::{from_slice, to_vec};
  use serde::{Deserialize, Serialize};

  #[derive(Debug, Serialize, Deserialize, PartialEq)]
  struct Point {
    x: i32,
    y: i32,
  }

  #[derive(Debug, Serialize, Deserialize, PartialEq)]
  struct Person {
    name: String,
    age: u8,
    active: bool,
  }

  #[test]
  fn test_serde_struct_roundtrip() {
    let p = Point { x: 10, y: -5 };
    let bytes = to_vec(&p).unwrap();
    let decoded: Point = from_slice(&bytes).unwrap();
    assert_eq!(decoded, p);
  }

  #[test]
  fn test_serde_person_roundtrip() {
    let p = Person {
      name: "Alice".to_owned(),
      age: 30,
      active: true,
    };
    let bytes = to_vec(&p).unwrap();
    let decoded: Person = from_slice(&bytes).unwrap();
    assert_eq!(decoded, p);
  }

  #[test]
  fn test_serde_option_some() {
    let v: Option<i32> = Some(42);
    let bytes = to_vec(&v).unwrap();
    let decoded: Option<i32> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, v);
  }

  #[test]
  fn test_serde_option_none() {
    let v: Option<i32> = None;
    let bytes = to_vec(&v).unwrap();
    let decoded: Option<i32> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, v);
  }

  #[test]
  fn test_serde_vec() {
    let v = vec![1i32, 2, 3, 4, 5];
    let bytes = to_vec(&v).unwrap();
    let decoded: Vec<i32> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, v);
  }

  #[test]
  fn test_serde_nested() {
    let v: Vec<Point> = vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
    let bytes = to_vec(&v).unwrap();
    let decoded: Vec<Point> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, v);
  }
}

// ─── Iter ────────────────────────────────────────────────────────────────────

#[test]
fn test_iter() {
  let bytes = fixture("multi_values");
  let vals: Vec<_> = msgpackr::Iter::new(&bytes)
    .collect::<Result<_, _>>()
    .unwrap();
  assert_eq!(vals.len(), 3);
}
