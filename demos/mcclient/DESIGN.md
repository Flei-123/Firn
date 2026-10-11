# demos/mcclient -- M0 design (protocol prototype, Minecraft Java 1.20.1, protocol 763)

Goal: a Firn program that joins a REAL 1.20.1 server (offline mode), decodes the
Play packets, reads chunk data into palette sections and writes a world dump
with a checksum. No GPU, no rendering. Clean room: only the public protocol
documentation / `minecraft-data` (PrismarineJS, MIT) are used. The server JAR is
a test subject only (downloaded to a temp dir, never read, never in the repo).

1.20.1 has NO configuration phase: Handshake -> Login -> Play.

## Layout

```
demos/mcclient/
  main.fi                  CLI + state machine                  (lead)
  wire.fi                  framing, zlib threshold, CFB8 hook   (lead)
  chunk.fi                 map_chunk -> palette sections        (lead)
  dump.fi                  canonical dump + checksum            (lead)
  schema.fi                generic packet-schema walker         (worker G)
  versions/v763/protocol.fi  GENERATED packet tables + schemas  (worker G)
tools/mcclient/
  gen_protocol.py          generator: minecraft-data -> Firn    (worker G)
  run.sh                   end-to-end test (real server)        (worker T)
  fetch_server.sh          official download into a temp dir    (worker T)
  ref_dump.cjs             reference client (node-minecraft-protocol) (worker T)
  compare.sh / metrics     (worker T)
tests/26xx_mcclient_*.fi   unit tests                           (lead / G)
```

Version = data (GDD 8.6): nothing about packet numbers or field layouts is typed
by hand. `gen_protocol.py <minecraft-data-dir> <version> <out.fi>` produces the
tables; a new version = rerun the generator with another version string.

## CLI contract of the Firn client

```
mcclient <host> <port> <name> <dump_file> [view_distance=4] [idle_ms=2500]
```
* Offline login, same `name` => same offline UUID.
* Sends Client Information (view distance as given, rest default), answers Keep
  Alive, confirms teleports (Teleport Confirm), handles Set Compression.
* Collects every `map_chunk` until `idle_ms` pass without a new one (after the
  first), then disconnects and writes the dump.
* stdout: one metric per line, `metric <key>=<value>`; keys: `connect_ms`
  (TCP connect done), `join_ms` (start -> Join Game decoded), `first_chunk_ms`,
  `total_ms`, `packets`, `bytes_in`, `bytes_decompressed`, `packets_per_s`,
  `chunks`, `walk_ok`, `walk_fail`, `checksum`.
* exit 0 = dump written.

## Canonical dump (text, UTF-8, LF, no trailing spaces) -- BOTH clients write exactly this

```
mcdump 1
protocol 763
chunks <N>
C <x> <z> sections=<S> blockentities=<B>
S <x> <z> <i> count=<blockCount> distinct=<D> blocks=<h64> biomes=<h64>
H <x> <z> <name> <h64>
L <x> <z> sky=<n> block=<n> h=<h64>
...
checksum sha256 <hex of SHA-256 over every byte above this line, including the last LF>
```
Order: chunks ascending by x, then z. Within a chunk: the `C` line, then `S` lines
by section index ascending (0 = lowest section; 1.20.1 overworld: 24 sections,
minY=-64), then `H` lines sorted by heightmap name (byte order), then the `L`
line. All numbers decimal; `<h64>` = 16 lowercase hex digits.

* `x`,`z` signed decimal chunk coordinates.
* `sections` = number of sections in the chunk data; `blockentities` = number of
  entries in the block-entity array of the packet.
* `count` = the `blockCount` short as sent. `distinct` = number of different
  block-state ids that really occur in the 4096 entries of the section.
* `blocks` = FNV-1a 64 (offset 0xcbf29ce484222325, prime 0x100000001b3) over the
  4096 global block-state ids, each as 2 bytes little endian, in index order
  `(y*16 + z)*16 + x` (y = 0..15 inside the section). `biomes` = same hash over
  the 64 global biome ids (2 bytes LE each), index `(y*4 + z)*4 + x`.
* `H`: heightmaps are NBT long arrays packed 9 bits per entry (no value spans two
  longs, 7 per long, 256 entries): unpack to 256 values, FNV-1a 64 over them as 2
  bytes LE each. One line per heightmap present (name as in the NBT).
* `L`: light data of the packet: `sky` = number of sky-light arrays, `block` =
  number of block-light arrays, `h` = FNV-1a 64 over all those arrays' bytes in
  packet order (sky arrays first, then block arrays), each array as sent (2048
  bytes).

The L lines are the least likely to be stable between two server sessions; the
test reports stability of S/H separately from L so that a flaky L is visible and
not hidden (see ROUND-M0.md).

## Test shape (end to end)

One server (offline, `level-seed` fixed, small view-distance), SAME player name
for all clients so the spawn position is identical:
`ref_dump (A) -> firn mcclient (B) -> ref_dump (A2)`; pass = A == A2 (reference
stable) and B == A (byte-identical dump file, same checksum).

## Packet/field naming of the generated module (worker G)

Constants `<STATE>_<DIR>_<NAME>` in upper snake case with the protocol.json
packet name, e.g. `HANDSHAKE_C2S_SET_PROTOCOL`, `LOGIN_S2C_SUCCESS`,
`PLAY_S2C_MAP_CHUNK`, `PLAY_C2S_KEEP_ALIVE`; STATE in HANDSHAKE|STATUS|LOGIN|PLAY,
DIR in C2S|S2C (`toServer` = C2S, `toClient` = S2C). Plus
`packet_name(state, dir, id) -> u64` (NUL-terminated C string or 0) and
`packet_schema(state, dir, id) -> u64` (handle for `schema.walk`, 0 if unknown).
