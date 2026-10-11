# demos/mcclient -- M0 design (protocol prototype, Minecraft Java 1.20.1, protocol 763)

Goal: a Firn program that joins a REAL 1.20.1 server (offline mode), decodes the
Play packets, reads chunk data into palette sections and writes a world dump
with a checksum. No GPU, no rendering. Clean room: only the public protocol
documentation / `minecraft-data` (PrismarineJS, MIT) are used. The server JAR is
a test subject only (downloaded to a temp dir, never read, never in the repo).

1.20.1 has NO configuration phase: Handshake -> Login -> Play.

## Layout (final, after the library round)

```
demos/mcclient/
  main.fi          thin glue: version by protocol number, mc_client, schema walk of every packet, chunk store, dump
  dump.fi          the canonical dump + checksum (test artefact)
  selftest.fi      offline test of mc_chunk + dump against tools/mc/chunk/expected.py
lib/mc/            the libraries (docs/MC-LIBS.md): mc_client, mc_codec, mc_schema, mc_chunk, mc_world, mc_dimension, mc_version ...
tools/mcgen/       generators (packet tables + registries from minecraft-data)
tools/mcclient/    end-to-end harness: real 1.20.1 server + reference client (run.sh, ref_dump.cjs, compare.sh, fetch_server.sh, mock_server.cjs)
```

## CLI contract of the Firn client

```
mcclient <host-ipv4> <port> <name> <dump_file> [view_distance=4] [idle_ms=2500] [protocol=763]
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

## Packet naming of the generated modules

Each version has `lib/mc/vNNN/mc_proto_vNNN.fi` with constants `<STATE>_<DIR>_<NAME>` (upper snake case of the protocol.json packet name),
`packet_name`, `packet_schema` and `version_make()`. Engine code does not use these constants: it asks the Version object for the wire id of a
SEMANTIC packet (`mc_version.version_pid(&v, mc_sem.SEM_PLAY_S2C_MAP_CHUNK)`), see docs/MC-VERSIONS.md.
