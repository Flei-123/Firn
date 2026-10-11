# mc_nbt -- NBT as a tree

`lib/mc/mc_nbt.fi`: the DOM layer over the pull reader of `std.nbt`. A `Doc` is an arena of nodes plus a blob (names,
strings, array payloads); node numbers (u32) are the handles, node 0 means "none".

```
doc_new / doc_free
doc_parse(&d, &bytes, anon, strict)      anon = the nameless root of 1.20.2+; strict = no octets may follow
doc_root(&d) / doc_root_name(&d, &len)
nbt_kind / nbt_elem_kind / nbt_count
nbt_get(&d, compound, name, len) -> node | 0        nbt_at(&d, list, i) -> node | 0
nbt_first / nbt_next                                 iterate the children of a list / compound in order
nbt_i64 (byte, short, int, long)  nbt_f32 / nbt_f64 (the bit patterns)  nbt_str(&d, n, &len)
nbt_arr_ptr / nbt_arr_i32 / nbt_arr_i64              raw big-endian payload of the three array kinds
doc_write(&d, &bytes)                                the document as it was read (byte identical)
doc_snbt(&d, &buf)                                   readable text for logs (floats as bit patterns)
```

* Reading and writing back is byte identical for every input that parses: compound entry order, list element type
  and order, strings and arrays are kept as raw octets.
* Hostile input: nesting <= 512 (vanilla limit), list / array counts are checked against the octets really left BEFORE
  anything is reserved, <= 4 Mi nodes and 256 MiB blob per Doc, every failure is a `TreeError`.
* Version handling: the root form is a parameter (`anon`), not a constant; the caller picks it from
  `mc_version.FLAG_NBT_ANON`.

Tests: `bash tools/mc/run.sh nbt` -- 60 random trees (all 12 tag types, nested compounds, lists of lists, empty
lists, unicode names and strings, extreme numbers) WRITTEN by prismarine-nbt; the dump equals the lines computed from
the generated tree and the write-back equals the input, for the named and the nameless root; 3 x 1500 damaged copies:
0 crashes, whatever parses writes back to the octets it consumed; counter-check. `tests/2741_mc_nbt_basic.fi`: getters,
byte identical write-back, strict / lenient trailing octets, depth bomb (3000 levels), list count bomb, negative array
length, every truncation of a document.
