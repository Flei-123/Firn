# mc_packet -- bounds checked Reader / Writer for protocol 763 (1.20.1)

`lib/mc/mc_packet.fi`, `import mc.mc_packet`. Built on `std.bytes` (big endian scalars,
VarInt layout). Tests: `tests/2660..2662_mc_packet_*.fi`; cross-check
`bash tools/mc/packet/run.sh`.

## Model

* **Sticky error flag.** `Reader` / `Writer` hold `err` (0 = ok). The first failure
  is stored and never overwritten; afterwards every `rd_*` returns a zero value and
  does not move the cursor, every `wr_*` writes nothing. A packet driver reads all
  fields and checks `rd_ok(r)` once.
* **Nothing from the peer is trusted.** Every length / count is checked against the
  octets that are really left before any loop or allocation (`rd_array_count`,
  `rd_bitset`, `rd_string`). A forged count of 2^31-1 gives `PE_UNDERFLOW` at once.
* **Spans, not copies.** `rd_string`, `rd_rest`, `rd_bytes`, `rd_nbt` return a pointer
  INTO the packet buffer (`u64`) plus a length; copy if the packet buffer is reused.

## Setup

```
var r: mc_packet.Reader = mc_packet.Reader { b: bytes.bytes_new(), err: 0 }
mc_packet.reader_init(&r, ptr, len)        // or reader_from_bytes(&r, &bytes)
...
var w: mc_packet.Writer = mc_packet.writer_new();  defer mc_packet.writer_free(&w)
mc_packet.wr_varint(&w, 5);  if !mc_packet.wr_ok(&w) { ... }
// bytes: wr_ptr(&w), wr_len(&w);  writer_clear(&w) reuses the memory
```

## Reader (`rd_*`)

| Function | Notes |
|---|---|
| `rd_u8 i8 u16 i16 u32 i32 u64 i64 f32 f64` | big endian |
| `rd_bool` | strict: only 0 / 1, else `PE_BAD_VALUE` |
| `rd_varint` / `rd_varlong` | max 5 / 10 octets (`PE_VARINT`); bits beyond 32 / 64 are dropped and non minimal forms accepted, like the vanilla reader |
| `rd_string(r, max_chars, &n) -> ptr` | VarInt octet length, strict UTF-8; octets <= 3*max_chars (checked first), UTF-16 units <= max_chars. Errors `PE_NEGATIVE`, `PE_TOO_LONG`, `PE_UNDERFLOW`, `PE_BAD_UTF8` |
| `rd_identifier(r, &n) -> ptr` | string (max 32767) + `'namespace:path'` check (`PE_BAD_IDENT`): namespace `[a-z0-9_.-]`, path `[a-z0-9_./-]`, at most one `:` |
| `rd_uuid(r, &hi, &lo)` | two u64 |
| `rd_position(r, &x, &y, &z)` | 26 / 26 / 12 bit signed (x, z, y) |
| `rd_angle -> u8`, `angle_to_degrees`, `degrees_to_angle` | 1/256 turn |
| `rd_bitset(r, dst, cap_longs) -> n` | VarInt count + longs into `dst` (machine order); `bitset_get/set(words, n, bit)` |
| `rd_array_count(r, min_elem, max) -> n` | VarInt count; `n <= max` and `n * min_elem <= remaining`, else `PE_TOO_LONG` / `PE_UNDERFLOW` / `PE_NEGATIVE`; the caller loops `n` times |
| `rd_optional` | bool prefix; read `T` when true |
| `rd_rest(r, &n) -> ptr`, `rd_bytes(r, n)`, `rd_byte_array(r, max, &n)` | raw octets |
| `rd_nbt(r, anon, &n) -> ptr` | one NBT value, structurally validated with `std.nbt` (depth 256, lengths vs. data). `anon=false`: named root, must be a compound (1.20.1 / files); `anon=true`: nameless root, any tag (1.20.2+). A lone TAG_End is "no NBT" (1 octet). Errors `PE_BAD_NBT`, `PE_UNDERFLOW` |
| `rd_slot(r, ver, &Slot)` | `ver` 763: present, itemId VarInt, count i8, named NBT; 764 / 765: nameless NBT; other versions (data components, >= 766) -> `PE_BAD_VALUE` |
| `rd_skip`, `rd_seek`, `rd_pos`, `rd_remaining`, `rd_fail`, `rd_ok`, `rd_err` | cursor / state |

## Writer (`wr_*`)

Same names and types. Validating ones: `wr_string(p, n, max_chars)` (strict UTF-8 and
the same limits, nothing written on error), `wr_identifier`, `wr_position` (range
check -> `PE_BAD_VALUE`), `wr_nbt(anon, p, n)` (re-validated, refuses malformed tags),
`wr_slot(ver, &Slot)`. A null pointer with a length (result of a failed read) gives
`PE_BAD_VALUE` instead of a crash. Allocator failure -> `PE_OOM` (folded in by
`wr_ok` / `wr_err`).

## Error codes

`PE_OK 0`, `PE_UNDERFLOW 1`, `PE_TOO_LONG 2`, `PE_BAD_UTF8 3`, `PE_BAD_VALUE 4`,
`PE_VARINT 5`, `PE_NEGATIVE 6`, `PE_OOM 7`, `PE_BAD_IDENT 8`, `PE_BAD_NBT 9`.

## What is tested against what

* `2660`: octets from the public protocol tables written out by hand (VarInt table,
  position example, bitset, slot with NBT in 763 and 764).
* `2661`: every failure case (underflow per width, sticky behaviour, 6 octet VarInt,
  forged lengths and counts, 8 invalid UTF-8 shapes, identifiers, NBT depth bomb).
* `2662`: 3000 random packets of 15 field kinds, write -> read identical; then
  damaged copies (bit flip, truncate, 0xff run, garbage) read with the same and a
  random script: no crash, cursor never beyond the end.
* `tools/mc/packet/run.sh`: 1200 random packets of 12 real 1.20.1 packet types
  (`system_chat`, `position`, `block_change`, `named_entity_spawn`, `update_time`,
  `custom_payload`, `settings`, `window_items` with NBT slots, `tab_complete`,
  `chat_message`, `multi_block_change`, `chat_suggestions`) serialised by the protodef
  serializer of node-minecraft-protocol; the Firn reader prints the values, the Firn
  writer re-encodes every packet byte for byte. Damaged inputs: no crash.
* Gap: UTF-16 "units" for the string limit are counted from strict UTF-8 (supplementary
  characters = 2 units), as the vanilla check does. Modified UTF-8 (NBT strings) is not
  involved.

## Speed (release-fast, this server, `tools/mc/packet/bench.fi`)

See the `BENCH` lines of `run.sh`; typical: writing 36 octet movement packets
~2.8 M pkt/s (~100 MB/s), reading ~1.0 M pkt/s (scalar reads go through
`std.bytes.get_*`, one octet at a time -- the obvious next optimisation), strings with
validation 4.5 M / 6 M pkt/s (~210 / ~280 MB/s).
