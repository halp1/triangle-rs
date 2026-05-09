# msgpackr-rs

### **DISCLAIMER: 100% vibecoded**

A Rust port of [msgpackr](https://github.com/kriszyp/msgpackr) — 100% wire-compatible with the JavaScript library, including all extensions.

## Install

```toml
[dependencies]
msgpackr = { path = "." }
```

Optional features:

```toml
msgpackr = { path = ".", features = ["bigint"] }   # BigInt extension via num-bigint
msgpackr = { path = ".", features = ["chrono"] }   # Timestamp ↔ chrono::DateTime
```

---

## Quick start

### Encode and decode with serde

```rust
use msgpackr::{to_vec, from_slice};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Point { x: f64, y: f64 }

let p = Point { x: 1.0, y: 2.5 };
let bytes = to_vec(&p)?;
let decoded: Point = from_slice(&bytes)?;
assert_eq!(p, decoded);
```

### Encode and decode `Value` directly

```rust
use msgpackr::{pack, unpack, Value};

let val = Value::Array(vec![
    Value::Int(42),
    Value::Str("hello".into()),
    Value::Bool(true),
]);

let bytes = pack(&val)?;
let back = unpack(&bytes)?;
```

---

## The `Value` type

Every MessagePack value, including msgpackr extensions, maps to a variant:

| Variant                                        | Description                             |
| ---------------------------------------------- | --------------------------------------- |
| `Value::Nil`                                   | `null`                                  |
| `Value::Bool(bool)`                            | boolean                                 |
| `Value::Int(i64)`                              | signed integer                          |
| `Value::UInt(u64)`                             | unsigned integer (> `i64::MAX`)         |
| `Value::F32(f32)` / `Value::F64(f64)`          | float                                   |
| `Value::Str(String)`                           | UTF-8 string                            |
| `Value::Bin(Vec<u8>)`                          | raw bytes                               |
| `Value::Array(Vec<Value>)`                     | array                                   |
| `Value::Map(Vec<(Value, Value)>)`              | ordered key-value pairs                 |
| `Value::Ext(i8, Vec<u8>)`                      | raw extension                           |
| `Value::Timestamp { seconds, nanos }`          | timestamp (ext type `0xff`)             |
| `Value::Undefined`                             | JS `undefined`                          |
| `Value::Set(Vec<Value>)`                       | JS `Set` (moreTypes)                    |
| `Value::MsgpackError { name, message, cause }` | JS `Error` (moreTypes)                  |
| `Value::Regex { source, flags }`               | JS `RegExp` (moreTypes)                 |
| `Value::TypedArray { type_code, data }`        | JS TypedArray (moreTypes)               |
| `Value::ArrayBuffer(Vec<u8>)`                  | JS `ArrayBuffer` (moreTypes)            |
| `Value::DataView(Vec<u8>)`                     | JS `DataView` (moreTypes)               |
| `Value::BigInt(num_bigint::BigInt)`            | BigInt extension (`feature = "bigint"`) |

---

## `Packer` — stateful encoder

Use `Packer` when you want the **record extension**: compact encoding for repeated objects with the same field structure (equivalent to `new Packr()` in JS). After the first time a set of field names is seen, subsequent objects with the same fields omit the field names entirely.

```rust
use msgpackr::{Packer, Value};

let mut packer = Packer::new(); // useRecords = true
```

Pass options using `Packer::with_options` (equivalent to `new Packr({ ... })` in JS):

```rust
use msgpackr::{Packer, PackOptions, Float32Mode, Value};

let mut packer = Packer::with_options(PackOptions {
    use_records: true,
    more_types: true,
    use_float32: Float32Mode::DecimalRound,
    ..Default::default()
});
```

```rust
let user = Value::Map(vec![
    (Value::Str("id".into()),   Value::UInt(1)),
    (Value::Str("name".into()), Value::Str("Alice".into())),
    (Value::Str("age".into()),  Value::UInt(30)),
]);

// First call: emits record definition + values
let first = packer.pack_value(&user)?;

// Second call with same structure: ~40% smaller — only record ID + values
let second = packer.pack_value(&Value::Map(vec![
    (Value::Str("id".into()),   Value::UInt(2)),
    (Value::Str("name".into()), Value::Str("Bob".into())),
    (Value::Str("age".into()),  Value::UInt(25)),
]))?;
```

Decode record-encoded bytes with the **same** `Packer` instance (it tracks the structure table):

```rust
let decoded = packer.unpack_value(&first)?;
```

Or use `Unpacker` for a long-lived stateful decoder (e.g. streaming):

```rust
use msgpackr::{Unpacker, UnpackOptions};

// Default options
let unpacker = Unpacker::new();

// With options (equivalent to new Unpackr({ ... }) in JS)
let unpacker = Unpacker::with_options(UnpackOptions {
    use_records: true,
    maps_as_objects: true,
    ..Default::default()
});

let val = unpacker.unpack(&bytes)?;
```

### Serde with records

```rust
use msgpackr::{to_vec_records, from_slice};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User { id: u64, name: String }

let bytes = to_vec_records(&User { id: 1, name: "Alice".into() })?;
let user: User = from_slice(&bytes)?;
```

---

## Pack / unpack options

Options can be passed to the one-shot functions **or** to stateful `Packer`/`Unpacker` via `with_options`.

### `PackOptions`

```rust
use msgpackr::{pack_with_opts, Packer, PackOptions, Float32Mode, Value};

let opts = PackOptions {
    use_records: false,
    use_float32: Float32Mode::DecimalRound, // encode floats as f32 where safe
    more_types: true,                       // enable Set/Error/RegExp/TypedArray
    use_timestamp32: true,                  // compact 4-byte timestamps (loses sub-second)
    ..Default::default()
};

// One-shot
let bytes = pack_with_opts(&Value::F64(1.5), &opts)?;

// Stateful (equivalent to new Packr({ ... }) in JS)
let mut packer = Packer::with_options(opts);
```

**`Float32Mode` values:**

| Variant           | Description                                              |
| ----------------- | -------------------------------------------------------- |
| `Never` (default) | Always use float64                                       |
| `Always`          | Always use float32 (may lose precision)                  |
| `DecimalRound`    | Use float32 when the value survives a decimal round-trip |
| `DecimalFit`      | Use float32 when the value fits exactly in float32       |

### `UnpackOptions`

```rust
use msgpackr::{unpack_opts, Unpacker, UnpackOptions};

let opts = UnpackOptions {
    maps_as_objects: true, // default — decode maps as ordered (key, value) pairs
    ..Default::default()
};

// One-shot
let val = unpack_opts(&bytes, &opts)?;

// Stateful (equivalent to new Unpackr({ ... }) in JS)
let unpacker = Unpacker::with_options(opts);
```

---

## Multiple values in one buffer

```rust
use msgpackr::{pack, unpack_multiple, Value};

let buf: Vec<u8> = [1i64, 2, 3]
    .iter()
    .flat_map(|&n| pack(&Value::Int(n)).unwrap())
    .collect();

let vals = unpack_multiple(&buf)?;
assert_eq!(vals.len(), 3);
```

---

## Iterators

```rust
use msgpackr::{Iter, Value};

let buf = /* bytes containing multiple msgpack values */;

for result in Iter::new(&buf) {
    let val: Value = result?;
    println!("{:?}", val);
}
```

`PackedIter` does the same but uses a `Packer` for record-aware decoding:

```rust
use msgpackr::{PackedIter, Packer};

let packer = Packer::new();
for result in PackedIter::new(&packer, &buf) {
    println!("{:?}", result?);
}
```

---

## Compatibility with Node.js msgpackr

All data produced by this crate can be decoded by the Node.js library and vice-versa. Key wire-format notes:

- `pack()` / `Packer::standard()` → standard MessagePack (no extensions beyond timestamps)
- `Packer::new()` → record extension enabled; bytes 0x40–0x7f in the stream are record IDs
- With records enabled, integers 64–127 are encoded as `uint8` (`0xcc`) so those bytes are free for record IDs
- msgpackr always uses `map16` (`0xde`) for plain JS objects regardless of key count
- Timestamps use ext type `0xff` in 32/64/96-bit formats per the MessagePack spec

---

## Extensions

Extensions allow custom types to be encoded as MessagePack ext format and decoded back into
`Value` variants. This is the Rust equivalent of JavaScript msgpackr's `addExtension`.

### Wire-level: `Value::Ext`

Any ext data is represented as `Value::Ext(type_code, bytes)`. This alone gives 100% wire
compatibility — you can always exchange raw ext bytes with Node.js:

```rust
use msgpackr::{pack, unpack, Value};

// Pack a custom type
let bytes = pack(&Value::Ext(42, vec![0xde, 0xad, 0xbe, 0xef]))?;

// Unpack produces Value::Ext when no handler is registered
let val = unpack(&bytes)?;
assert!(matches!(val, Value::Ext(42, _)));
```

### `ExtRegistry`: auto-transform custom types

For ergonomics matching JS `addExtension`, register handlers so decode automatically
converts ext bytes into the value you want:

```rust
use msgpackr::{pack, Value};
use msgpackr::unpacker::Unpacker;

let mut unpacker = Unpacker::new();

// Register type code 10: bytes are UTF-8 text → Value::Str
unpacker.add_extension(10, |data| {
    let s = std::str::from_utf8(data)
        .map_err(|_| msgpackr::Error::invalid("bad utf8"))?;
    Ok(Value::Str(s.to_string()))
});

// Encode from Node.js (or pack by hand) and decode transparently
let encoded = pack(&Value::Ext(10, b"hello".to_vec()))?;
let val = unpacker.unpack(&encoded)?;
assert_eq!(val, Value::Str("hello".into()));
```

For stateful encode+decode with the same `Packer`:

```rust
use msgpackr::{Value};
use msgpackr::packer::Packer;

let mut packer = Packer::new();

packer.add_extension(
    10,
    // unpack handler: bytes → Value
    |data| {
        let n = u64::from_be_bytes(data.try_into().unwrap_or([0; 8]));
        Ok(Value::UInt(n))
    },
    // pack handler: transform Value::Ext payload (optional)
    None::<fn(&[u8]) -> Option<Vec<u8>>>,
);

let encoded = packer.pack_value(&Value::Ext(10, 42u64.to_be_bytes().to_vec()))?;
let decoded = packer.unpack_value(&encoded)?;
assert_eq!(decoded, Value::UInt(42));
```

### Interoperability with Node.js `addExtension`

JavaScript side:

```js
import { addExtension, pack, unpack } from "msgpackr";

const MY_TYPE = 10;

addExtension({
  type: MY_TYPE,
  pack(value) {
    // value is a plain number; encode as 8-byte big-endian
    const buf = Buffer.alloc(8);
    buf.writeBigUInt64BE(BigInt(value));
    return buf;
  },
  unpack(buf) {
    return Number(buf.readBigUInt64BE());
  },
});
```

Rust side (decode Node.js output):

```rust
use msgpackr::{Value};
use msgpackr::unpacker::Unpacker;

let mut unpacker = Unpacker::new();
unpacker.add_extension(10, |data| {
    let arr: [u8; 8] = data.try_into()
        .map_err(|_| msgpackr::Error::invalid("expected 8 bytes"))?;
    Ok(Value::UInt(u64::from_be_bytes(arr)))
});

// `bytes` received from Node.js
let val = unpacker.unpack(bytes)?;
```

### Reserved type codes

These codes are used by msgpackr internally — do not register your own handlers for them:

| Code              | Built-in use                              |
| ----------------- | ----------------------------------------- |
| `0x00` (0)        | `Undefined`                               |
| `0x42` (66)       | `BigInt`                                  |
| `0x62` (98)       | Bundle strings                            |
| `0x65` (101)      | `MsgpackError`                            |
| `0x69` (105)      | ID reference                              |
| `0x70` (112)      | Pointer                                   |
| `0x72` (114)      | Record definition                         |
| `0x73` (115)      | `Set`                                     |
| `0x74` (116)      | `TypedArray` / `ArrayBuffer` / `DataView` |
| `0x78` (120)      | `Regex`                                   |
| `0xff` (255 / -1) | Timestamp                                 |

---

## Error handling

All fallible functions return `msgpackr::Result<T>`, which is `std::result::Result<T, msgpackr::Error>`.

```rust
use msgpackr::Error;

match unpack(&bad_bytes) {
    Err(Error::UnexpectedEnd) => eprintln!("truncated data"),
    Err(Error::InvalidData(msg)) => eprintln!("bad data: {}", msg),
    Err(e) => eprintln!("other error: {}", e),
    Ok(val) => { /* ... */ }
}
```

**Error variants:**

| Variant                | When                                    |
| ---------------------- | --------------------------------------- |
| `UnexpectedEnd`        | Buffer truncated mid-value              |
| `InvalidData(String)`  | Malformed data or type mismatch         |
| `UnknownExtension(u8)` | Unrecognised ext type code              |
| `RangeError(String)`   | Value out of range (e.g. too-large map) |
| `TooLarge(String)`     | Structure would exceed size limits      |
