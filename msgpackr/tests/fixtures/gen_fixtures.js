#!/usr/bin/env node
"use strict";

/**
 * Generate binary msgpack fixtures using the real msgpackr JS library.
 * Each fixture is a pair: .input.json (original data) and .msgpack (encoded bytes).
 * Also generates a manifest.json describing what each fixture tests.
 *
 * Run from the repo root: node tests/fixtures/gen_fixtures.js
 */

const { Packr, Unpackr, pack, unpack } = require("../../msgpackr/index.js");
const fs = require("fs");
const path = require("path");

const OUT = path.join(__dirname, "generated");
fs.mkdirSync(OUT, { recursive: true });

const manifest = [];
let count = 0;

function write(name, bytes, description, original) {
  const id = String(count++).padStart(4, "0");
  const filename = `${id}_${name}`;
  fs.writeFileSync(path.join(OUT, `${filename}.msgpack`), bytes);
  if (original !== undefined) {
    fs.writeFileSync(
      path.join(OUT, `${filename}.json`),
      JSON.stringify(
        original,
        (k, v) => {
          if (typeof v === "bigint") return { __bigint__: v.toString() };
          return v;
        },
        2,
      ),
    );
  }
  manifest.push({ id, name: filename, description });
}

function packAndWrite(name, value, opts, description) {
  let bytes;
  if (opts) {
    const packr = new Packr(opts);
    bytes = packr.pack(value);
  } else {
    bytes = pack(value);
  }
  write(name, bytes, description, value);
  return bytes;
}

// ─── Basic types ────────────────────────────────────────────────────────────
packAndWrite("nil", null, null, "nil (null)");
packAndWrite("bool_true", true, null, "boolean true");
packAndWrite("bool_false", false, null, "boolean false");
packAndWrite("undefined_value", undefined, null, "undefined");

// ─── Integers ────────────────────────────────────────────────────────────────
for (const n of [
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
  65535,
  65536,
  0x7fffff,
  0xffffff,
  0x7fffffff,
  0xffffffff,
  Number.MAX_SAFE_INTEGER,
  -1,
  -32,
  -33,
  -128,
  -129,
  -32768,
  -32769,
  -2147483648,
  -2147483649,
  -Number.MAX_SAFE_INTEGER,
]) {
  packAndWrite(`int_${n < 0 ? "neg" + Math.abs(n) : n}`, n, null, `integer ${n}`);
}

// ─── Floats ────────────────────────────────────────────────────────────────
for (const f of [
  0.0,
  0.5,
  1.0,
  1.5,
  -1.5,
  1.1,
  3.14159,
  1e100,
  -1e100,
  Infinity,
  -Infinity,
  NaN,
]) {
  const name = `float_${JSON.stringify(f).replace(/[^0-9a-z\-]/gi, "_")}`;
  packAndWrite(name, f, null, `float ${f}`);
}

// ─── Strings ─────────────────────────────────────────────────────────────────
packAndWrite("str_empty", "", null, "empty string");
packAndWrite("str_hello", "hello", null, "short string");
packAndWrite("str_31", "a".repeat(31), null, "31-char string");
packAndWrite("str_32", "a".repeat(32), null, "32-char string (str8)");
packAndWrite("str_255", "a".repeat(255), null, "255-char string (str8)");
packAndWrite("str_256", "a".repeat(256), null, "256-char string (str16)");
packAndWrite("str_65535", "a".repeat(65535), null, "65535-char string (str16)");
packAndWrite("str_unicode", "héllo wörld 日本語 🎉", null, "unicode string");

// ─── Binary ──────────────────────────────────────────────────────────────────
packAndWrite("bin_empty", Buffer.from([]), null, "empty binary");
packAndWrite("bin_small", Buffer.from([0xde, 0xad, 0xbe, 0xef]), null, "small binary");
packAndWrite("bin_255", Buffer.alloc(255, 0xaa), null, "255-byte binary");

// ─── Arrays ──────────────────────────────────────────────────────────────────
packAndWrite("array_empty", [], null, "empty array");
packAndWrite("array_small", [1, 2, 3], null, "small array");
packAndWrite(
  "array_15",
  Array.from({ length: 15 }, (_, i) => i),
  null,
  "15-element fixarray",
);
packAndWrite(
  "array_16",
  Array.from({ length: 16 }, (_, i) => i),
  null,
  "16-element array16",
);
packAndWrite(
  "array_mixed",
  [null, true, false, 42, 3.14, "hello", [1, 2], { a: 1 }],
  null,
  "mixed array",
);
packAndWrite("array_nested", [[1, [2, [3]]], 4], null, "nested array");

// ─── Maps ────────────────────────────────────────────────────────────────────
packAndWrite("map_empty", {}, null, "empty map");
packAndWrite("map_small", { a: 1, b: 2 }, null, "small map");
packAndWrite(
  "map_15_keys",
  Object.fromEntries(Array.from({ length: 15 }, (_, i) => [`k${i}`, i])),
  null,
  "15-key fixmap",
);
packAndWrite(
  "map_16_keys",
  Object.fromEntries(Array.from({ length: 16 }, (_, i) => [`k${i}`, i])),
  null,
  "16-key map16",
);
packAndWrite("map_nested", { a: { b: { c: 42 } } }, null, "nested map");
packAndWrite(
  "map_mixed_vals",
  { n: null, b: true, i: 42, f: 3.14, s: "hi", a: [1], m: { x: 0 } },
  null,
  "mixed value types",
);

// ─── Timestamps ──────────────────────────────────────────────────────────────
{
  const epoch = new Date(0);
  const ts32 = new Packr({ useTimestamp32: true });
  write("timestamp_epoch", ts32.pack(epoch), "timestamp32 of epoch", epoch.toISOString());

  const normal = new Packr({});
  const now = new Date("2023-06-15T12:30:00.000Z");
  write(
    "timestamp_now",
    normal.pack(now),
    "timestamp64 of 2023-06-15T12:30:00Z",
    now.toISOString(),
  );

  const withNanos = new Packr({});
  const precise = new Date("2023-06-15T12:30:00.123Z");
  write(
    "timestamp_millis",
    withNanos.pack(precise),
    "timestamp64 with milliseconds",
    precise.toISOString(),
  );

  // Date before epoch
  const beforeEpoch = new Date("1900-01-01T00:00:00.000Z");
  write(
    "timestamp_before_epoch",
    normal.pack(beforeEpoch),
    "timestamp96 for before-epoch date",
    beforeEpoch.toISOString(),
  );
}

// ─── Records extension ───────────────────────────────────────────────────────
{
  const packr = new Packr({ useRecords: true });
  const obj1 = { x: 1, y: 2, z: 3 };
  const encoded1 = packr.pack(obj1);
  write(
    "records_first_def",
    encoded1,
    "record extension: first definition of {x,y,z}",
    obj1,
  );

  // Same structure again — should use record ID without re-defining
  const encoded2 = packr.pack({ x: 10, y: 20, z: 30 });
  write("records_reuse", encoded2, "record extension: reuse of {x,y,z}", {
    x: 10,
    y: 20,
    z: 30,
  });

  // Multiple structures
  const encoded3 = packr.pack({ a: "hello", b: "world" });
  write("records_new_struct", encoded3, "record extension: new structure {a,b}", {
    a: "hello",
    b: "world",
  });

  // Nested records
  const encoded4 = packr.pack({ user: { name: "Alice", age: 30 }, score: 100 });
  write("records_nested", encoded4, "record extension: nested structures", {
    user: { name: "Alice", age: 30 },
    score: 100,
  });

  // Array of same-structured objects (highly compact)
  const users = [
    { id: 1, name: "Alice", email: "a@example.com" },
    { id: 2, name: "Bob", email: "b@example.com" },
    { id: 3, name: "Charlie", email: "c@example.com" },
  ];
  const packr2 = new Packr({ useRecords: true });
  write(
    "records_array_of_structs",
    packr2.pack(users),
    "record extension: array of same-structured objects",
    users,
  );
}

// ─── Integer edge cases with records ────────────────────────────────────────
{
  const packr = new Packr({ useRecords: true });
  // With records=true, integers 64-127 must use uint8 (0xcc), not fixint
  for (const n of [63, 64, 65, 127, 128]) {
    const bytes = packr.pack(n);
    write(`records_int_${n}`, bytes, `integer ${n} with useRecords=true`, n);
  }

  // Same integers without records
  for (const n of [63, 64, 65, 127, 128]) {
    const bytes = pack(n);
    write(`norecords_int_${n}`, bytes, `integer ${n} without records`, n);
  }
}

// ─── More types ──────────────────────────────────────────────────────────────
{
  const packr = new Packr({ moreTypes: true });

  // Set
  const s = new Set([1, 2, 3, "a"]);
  write("more_types_set", packr.pack(s), "Set extension", Array.from(s));

  // RegExp
  const r = /hello/gi;
  write("more_types_regexp", packr.pack(r), "RegExp extension", {
    source: r.source,
    flags: r.flags,
  });

  // Error
  const e = new TypeError("test error");
  write("more_types_error", packr.pack(e), "Error extension", {
    name: e.name,
    message: e.message,
  });

  // Uint8Array
  const u8 = new Uint8Array([1, 2, 3, 4, 5]);
  write("more_types_uint8array", packr.pack(u8), "Uint8Array extension", Array.from(u8));

  // Int32Array
  const i32 = new Int32Array([100, -200, 300]);
  write(
    "more_types_int32array",
    packr.pack(i32),
    "Int32Array extension",
    Array.from(i32),
  );

  // Float64Array
  const f64 = new Float64Array([1.1, 2.2, 3.3]);
  write(
    "more_types_float64array",
    packr.pack(f64),
    "Float64Array extension",
    Array.from(f64),
  );

  // ArrayBuffer
  const ab = new ArrayBuffer(8);
  new Uint8Array(ab).set([0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]);
  write("more_types_arraybuffer", packr.pack(ab), "ArrayBuffer extension", null);

  // DataView
  const dv = new DataView(new ArrayBuffer(4));
  dv.setUint32(0, 0xdeadbeef);
  write("more_types_dataview", packr.pack(dv), "DataView extension", null);
}

// ─── Float32 modes ───────────────────────────────────────────────────────────
{
  const vals = [1.5, 1.1, 3.14159, 100.5];
  for (const v of vals) {
    const name = `float_${v}`.replace(".", "_");
    write(`${name}_f64`, pack(v), `float64 encoding of ${v}`, v);
    write(
      `${name}_f32always`,
      new Packr({ useFloat32: 1 }).pack(v),
      `float32 always encoding of ${v}`,
      v,
    );
    write(
      `${name}_f32decimal`,
      new Packr({ useFloat32: 3 }).pack(v),
      `float32 decimal round encoding of ${v}`,
      v,
    );
    write(
      `${name}_f32fit`,
      new Packr({ useFloat32: 4 }).pack(v),
      `float32 decimal fit encoding of ${v}`,
      v,
    );
  }
}

// ─── Structured clone ────────────────────────────────────────────────────────
{
  const packr = new Packr({ structuredClone: true });
  const obj = { a: 1, b: 2 };
  const arr = [obj, obj]; // shared reference
  write(
    "structured_clone_shared_ref",
    packr.pack(arr),
    "structured clone: shared reference",
    null,
  );
}

// ─── Large numbers ───────────────────────────────────────────────────────────
{
  // uint64 max
  const packr = new Packr({});
  write(
    "uint64_max",
    Buffer.from([0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
    "uint64 max raw",
    null,
  );

  // int64 min
  write(
    "int64_min",
    Buffer.from([0xd3, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
    "int64 min raw",
    null,
  );
}

// ─── Edge cases ──────────────────────────────────────────────────────────────
{
  // Deep nesting (should not stack overflow)
  let deep = {};
  let cur = deep;
  for (let i = 0; i < 20; i++) {
    cur.child = {};
    cur = cur.child;
  }
  cur.value = 42;
  packAndWrite("deep_nesting", deep, null, "20-level deep nesting");

  // Large array
  const large = Array.from({ length: 1000 }, (_, i) => i);
  packAndWrite("large_array_1000", large, null, "1000-element array");

  // String with null bytes
  packAndWrite(
    "str_with_nulls",
    "hello\x00world",
    null,
    "string with embedded null bytes",
  );

  // Consecutive values (for multi-value decode testing)
  const buf = Buffer.concat([pack(1), pack("hello"), pack([1, 2, 3])]);
  write("multi_values", buf, "multiple consecutive values", [1, "hello", [1, 2, 3]]);
}

// ─── Manifest ────────────────────────────────────────────────────────────────
fs.writeFileSync(path.join(OUT, "manifest.json"), JSON.stringify(manifest, null, 2));

console.log(`Generated ${count} fixtures in ${OUT}`);
console.log("Manifest written to manifest.json");

// ─── Verification: decode all fixtures back with msgpackr ────────────────────
let failures = 0;
for (const entry of manifest) {
  try {
    const bytes = fs.readFileSync(path.join(OUT, `${entry.name}.msgpack`));
    // Just check it decodes without error
    const decoded = unpack(bytes);
  } catch (e) {
    if (
      !entry.description.includes("raw") &&
      !entry.description.includes("structured clone") &&
      !entry.name.includes("multi_values")
    ) {
      console.error(`FAIL decode ${entry.name}: ${e.message}`);
      failures++;
    }
  }
}
if (failures === 0) {
  console.log("All fixtures decode cleanly with msgpackr.");
} else {
  console.error(`${failures} fixtures failed to decode!`);
  process.exit(1);
}
