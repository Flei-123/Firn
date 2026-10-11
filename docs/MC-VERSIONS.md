# MC-VERSIONS -- how the Minecraft libraries stay version-modular

Rule of the owner (2026-10-11): the libraries are built so that further game versions
(1.12.2, 1.8.9, 1.16.5, newer releases) can be added later **without touching the core**.
Version = data + small version modules (GDD 8.6). What is built today: 1.20.1 is the
focus; the other versions only prove the modularity.

## 1. What is version independent, what is not

| Version independent (the core) | Version dependent (data or a small module) |
|---|---|
| VarInt / VarLong, ZigZag (`std.bytes`) | packet ids and packet layouts (per state and direction) |
| frame reader/writer, compression threshold, zlib, AES-CFB8 (`mc_codec`, `mc_crypto`) | feature flags (`mc_version.FLAG_*`) |
| NBT basic types (`std.nbt`, `mc_nbt`) | the login flow (configuration phase from 1.20.2, UUID field, signatures) |
| network layer (`std.net`, `lib/async`, `mc_net`) | the chunk decoding strategy (`mc_version.CF_*` -> `mc_chunk`) |
| schema walker (`mc_schema`), math, mesh, noise, Anvil | block / item / entity / biome registries and the map "network id -> internal id" |
| the internal world model: `mc_chunk.Chunk/Section/Container`, `mc_world.World` | entity metadata, slot layout, chat component details |

The **internal world model is version neutral**: a Section is a palette of global ids
plus a dense index array, whatever the wire looked like. Version adapters translate
from/to the wire format; the engine never sees a wire format.

## 2. The pieces

```
tools/mcgen/
  versions.json       hand facts per version: data key, nmp version, protocol, FLAG_*, chunk format
  semantic.json       the semantic packet names the engine uses (SEM_*) -> minecraft-data names
  gen_protocol.py     the generator (minecraft-data protocol.json -> Firn)
  gen_fixtures.cjs    example packets for EVERY packet from the reference serializer
lib/mc/
  mc_version.fi       Version object, FLAG_*, CF_*, ST_*, DIR_*          (hand written, small)
  mc_sem.fi           SEM_* indexes                                      (generated)
  mc_schema.fi        generic packet walker / validator                  (hand written, version free)
  mc_versions.fi      registry: version_by_protocol(n, &v)               (generated)
  vNNN/mc_proto_vNNN.fi  packet ids, names, schema programs, version_make()   (generated)
tools/mc/schema/run.sh   conformance of the walker for every version in versions.json
```

At run time: `mc_versions.version_by_protocol(763, &v)` fills a `Version`. Engine code asks
`mc_version.version_pid(&v, mc_sem.SEM_PLAY_S2C_MAP_CHUNK)` for a wire id and
`mc_version.version_has(&v, mc_version.FLAG_CONFIG_PHASE)` for behaviour. Several versions
live in one binary (every module in `mc_versions.fi` is linked: ~60 KB of schema data each).
The client picks the version from the protocol number in the server-list ping.

## 3. Adding a version -- checklist

1. **Data present?** `data/dataPaths.json` of `minecraft-data` has the key (`pc` section)
   and `version.json` gives the protocol number. If not: update the `minecraft-data` copy
   (`npm --prefix tools/mcserver update minecraft-data`); do not type tables by hand.
2. **`tools/mcgen/versions.json`**: add the entry: `data` (key in dataPaths), `nmp` (version
   string of node-minecraft-protocol, for fixtures; omit if unsupported), `protocol`,
   `flags` and `chunk_format`. The flags are facts from the public protocol history; a
   wrong flag is a one-line fix here, never a code change.
3. **Generate**: `python3 tools/mcgen/gen_protocol.py --all <minecraft-data>/data lib/mc all`
   -> `lib/mc/vNNN/mc_proto_vNNN.fi`, `mc_versions.fi`, `mc_sem.fi`. If the generator
   stops with "native type X is not handled" or "compareTo not found": a new protodef
   construct appeared. Add it to the generator (one opcode or one scoping rule) and the
   walker; the old versions' tests catch regressions. (Example from this round: 1.12.2
   needed "the fields of an anonymous container are visible to the following siblings".)
4. **Conformance**: `bash tools/mc/schema/run.sh [firnc] [protocol]`. The reference
   serializer writes ~4 examples of every packet of the version; the Firn walker must
   consume each exactly, reject every truncation and every extension, and survive
   mutations. Failures are generator or walker gaps, never "the fixture is wrong" unless
   shown.
5. **Semantic ids**: the engine needs `v.sem[SEM_*] >= 0` for the packets it uses. If the
   new version calls one of them differently, add the new name to the entry in
   `semantic.json` (the list of names is tried in order). A packet that does not exist in
   the version stays -1; the state machine must be written against the flag, not against
   the number.
6. **Behaviour flags**: login (`FLAG_LOGIN_UUID_*`, `FLAG_CONFIG_PHASE`, `FLAG_SIGNED_CHAT`),
   NBT root (`FLAG_NBT_ANON`), chunk batching. New behaviour = a new flag + one branch in
   the state machine, with a test per branch.
7. **Chunk strategy**: if `chunk_format` is a known `CF_*` there is nothing to do. A new
   format = one decoding function in `mc_chunk` that fills the neutral `Chunk`
   (`chunk_decode_for` dispatches), a python reference decoder for the cross-check
   (`tools/mc/chunk/`), and a fixture generator.
8. **Registries** (planned `mc_registry`): generated from `blocks.json`, `items.json`,
   `entities.json`, `biomes.json` of the version into compact tables; the version adapter
   gets a map "network id -> internal id" (stable internal block / item model). Pre-flattening
   versions need an extra mapping (see 5).
9. **Real server**: `tools/mcclient/run.sh` against the official server of that version,
   dump compared with the reference client. Only then the version is "supported"; before
   that it is "tables generated, walker-conformant".
10. **Docs**: list the version in the table below.

## 4. Status of the versions

| Version | Protocol | Tables generated | Walker conformance (reference fixtures) | Chunk strategy | Joins a real server |
|---|---|---|---|---|---|
| 1.8.9 | 47 | yes | yes (448 packets x 4 stages) | `CF_LEGACY_8` not implemented | no |
| **1.12.2** | **340** | yes | yes (500) | `CF_SECTIONS_19` **implemented** (`mc_chunk.chunk_decode_19`) | **yes** (11.10.2026: real 1.12.2 server, dump byte-identical to the reference client, 81 chunks) |
| 1.16.5 | 754 | yes | yes (616) | `CF_SECTIONS_114` not implemented | no |
| **1.20.1** | **763** | yes | yes (704) | `CF_CONTAINERS_118` **implemented** | in progress (M0) |
| **1.20.2** | **764** | yes (has the Configuration state) | yes (788) | `CF_CONTAINERS_118` (same, nameless NBT roots) | **yes** (11.10.2026: real 1.20.2 server incl. configuration phase, dump byte-identical, 109 chunks) |
| **1.21.1** | **767** | yes | yes (904) | `CF_CONTAINERS_118` **verified** | **yes** (11.10.2026: real 1.21.1 server, Known Packs answered, dump byte-identical to the reference, 109 chunks) |
| **1.21.4** | **769** | yes | yes (948) | `CF_CONTAINERS_118` **verified** | **yes** (11.10.2026: real 1.21.4 server, client information with particle status, dump byte-identical, 117 chunks) |
| **1.21.8** | **772** | yes | yes (980) | `CF_CONTAINERS_118` + 1.21.5 shape (`FLAG_HM_MAP`) **verified** | **yes** (11.10.2026: real 1.21.8 server, heightmap map + containers without length, dump byte-identical, 117 chunks) |
| 1.21.11 | 774 | yes | yes (1008) | same | no |
| 26.1 | 775 (in minecraft-data) | yes | yes (1028) | same | no |

Tables for 1.20.5+ needed four generator/walker extensions (all in this round, no data typed by hand):
`registryEntryHolderSet` (op 18), `lpVec3` (op 19, 1.21.9+; format from the public protocol
description: u8 a; a == 0 -> zero vector, else u8 + u32 + varint when bit 2 of a is set),
recursive types (op 20 RCALL: a slot component that holds a slot; the walker saves/restores its
registers around the call, depth limit 64), and name lookup in enclosing containers (1.21.11
`DebugSubscriptionUpdate`). Two `minecraft-data` inconsistencies are worked around and counted by
the generator (`MCGEN_VERBOSE=1` lists them): a switch case whose key the mapper never produces
(`minecraft:crafting_special_banneraddpattern` in 1.20.5/1.21.1, `zombie_nautilus/variant` in 26.1)
is dropped. Fixtures for versions that node-minecraft-protocol (1.54) does not support
(>= 1.21.5) come from the same protodef compiler on the minecraft-data protocol plus a
`lpVec3` type written in `gen_fixtures.cjs`. node-minecraft-protocol's INLINE `registryEntryHolder`
is broken in reader, writer and sizeOf (no leading 0, wrong offset, `holder.data`); corrected
versions are used for the reference (documented in `gen_fixtures.cjs`). Only the walker is
proven for these versions; no join, no chunk check (1.21.5 changed the heightmap encoding).

"26.2" (named by the owner): the `minecraft-data` copy on the server knows `26.1` (protocol
775) but not `26.2`; its protocol number is therefore **not known here and not guessed**.
When a 26.2 server exists, the protocol number comes from its status ping and the data from a
newer `minecraft-data`. Both generator gaps (`lpVec3`, `registryEntryHolderSet`) are closed, so a 26.2 entry
is one `versions.json` block once its data exists (r24 stays open until then).

## 5. Theoretical walk-through: adding 1.12.2 (protocol 340) all the way to a join

What exists after step 4 of the checklist (measured): the tables (125 packets), the schema
program blob (11 KB, 18 registers) and a walker that decodes every 1.12.2 packet.

Files that would be added or touched:

| File | What |
|---|---|
| `lib/mc/mc_chunk.fi` (+ `chunk_decode_19`) | the decoder for `CF_SECTIONS_19` |
| `tools/mc/chunk/` | python reference decoder + fixtures for that format |
| `tools/mcgen/versions.json` | already present (flags: compression, palette chunks, no flattening) |
| `lib/mc/mc_registry` data for 340 | block table from `pc/1.12.2/blocks.json` |
| `mc_client` login branch | nothing new for the encryption/compression handshake; no UUID field in Login Start; no plugin messages in login |

Chunk strategy `CF_SECTIONS_19` (public format description, to be verified against a
real 1.12.2 server before use): the packet carries a section bitmask, a ground-up flag
and a data buffer; each present section has bits-per-block, a palette (for 4..8 bits,
direct global ids above), a packed long array, then 2048 octets block light and (only in
the overworld, which is a property of the **dimension**, not of the packet) 2048 octets
sky light; a ground-up chunk ends with 256 biome octets. There is no heightmap and no
separate light packet. The decoded model stays the neutral `Chunk`; the biome array
becomes a `Container` per section only if the engine wants it (the dump format can say so).

The real gaps for a join (honest list):
* **Block ids**: 1.12.2 block states are `id << 4 | meta`, not flattened state ids. The
  neutral model needs a stable internal block identity. `minecraft-data` lists blocks per
  version by name; mapping `(id, meta)` to *properties* (facing, variant, age...) needs
  curated data. The widely used community conversion tables of other projects are
  GPL-licensed and must not be used here (clean room) -- the table has to be generated
  from `minecraft-data` per version or hand-curated.
* **Client-written packets**: today a hand written function builds each client packet
  (Settings, teleport confirm, keep-alive). Their layout differs per version. The proper fix
  is a **schema-driven writer** in `mc_schema` (`encode(schema, values)`) fed by the same
  generated programs; that does not exist yet (planned with `mc_packet`).
* **Entity metadata / slots / chat**: out of scope for M0; the walker consumes them, a typed
  decoder per version is future work.
* **Dimension info**: `CF_SECTIONS_19` needs to know whether the dimension has sky light;
  the version adapter must remember the dimension from Join Game / Respawn.

## 6. What the tests prove today

* `bash tools/mc/schema/run.sh`: for 1.8.9, 1.12.2, 1.16.5, 1.20.1, 1.20.2 the generator
  output equals the committed files; ~3000 reference-serializer packets are consumed
  exactly in four build stages; truncated / extended / mutated / random packets never crash.
  The walker was not changed for the four extra versions; the generator needed one rule
  (anonymous container scope).
* `tests/2650_mc_schema_basic.fi`: the version objects (flags, semantic ids, chunk format,
  function-value lookups) of 763, 340, 764 coexist in one binary and are selected at run time.
* Not proven: any join with a version other than 1.20.1; any chunk format other than
  `CF_CONTAINERS_118`.

## 7. Schema-driven client packet writer (design, NOT built yet, roadmap r16)

Goal: `mc_schema.encode(schema, values, out)` builds serverbound bodies from the same generated
programs, so no per-version hand written writer is needed. Design decided in this round:
* Values are a flat text store `path=value` (the walker's dump paths: `a.b[2].c`), tokens: decimal,
  `true/false`, `0x..` raw bits (floats), `"..."` strings, `hex:..` buffers/NBT/lpVec3, `uuid:..`;
  prefix counts as `path#=N`, options as `path?=0|1`; holder id form `path.<baseName>=N`.
* The encoder interprets the same bytecode (registers for switch keys and CK_REG counts, TOPBIT bit 7
  patched afterwards, RCALL register save) and finishes with `mc_schema.walk` on its own output.
* Test: `gen_fixtures.cjs` writes a flattened `<n>.pv` next to each reference body; Firn must
  reproduce every `.bin` byte for byte, in all versions (then hand writers become fallbacks).

## 8. Round 2026-10-11 (worker voxel-client-1-12-2-chunks-join-1-20)

Measured against the OFFICIAL servers (jar from piston-meta, SHA-1 checked, offline mode, Java 17, seed 4242,
view distance 4), `MC_VERSION=<release> bash tools/mcclient/run.sh <firn client>`: reference client A (node-minecraft-protocol
+ independent decoder in `tools/mcclient/ref_dump.cjs`) == A2 and Firn client B == A.

* **1.12.2 (340)**: PASS, checksum `cea711cdf4a3...`, 81 chunks, walk_fail 0. New: `mc_chunk.chunk_decode_19`
  (bits per block u8, palette, packed longs **that may span two longs**, block light, sky light only in the overworld,
  256 biome octets of a ground-up column). The dimension comes from Join Game / Respawn
  (`mc_dimension.dim_legacy_id`, `mc_chunk.scratch_set_sky`). Values stay `id << 4 | meta` (the `mc_reg_v340` states).
  Client Information is sent in the 1.9-1.16 layout (`client_send_settings` tries the 1.17+ layout first; the schema check
  refuses what does not fit). A section-only update (`groundUp = false`) is NOT merged: it is counted
  (`partial_chunks`) and dropped in both clients -- a real merge is open.
* **1.20.2 (764)**: PASS, checksum `87ad3f91c1ad...`, 109 chunks. Bug found by the real server and fixed: from 1.20.2 the
  NBT roots in the chunk packet (heightmaps, block entities) have **no name** (`FLAG_NBT_ANON`);
  `chunk_decode_anon` honours the flag, `chunk_decode_for` passes it. The configuration phase
  (Login Acknowledged -> Configuration -> Finish) worked unchanged against the real server.
* Dump format for `CF_SECTIONS_19` (demos/mcclient/DESIGN.md): `S` lines without `biomes=`, one `B x z <h64>` line per ground-up
  column (FNV-1a over the 256 biome octets as u16 LE), `count` = number of non-air blocks, absent sections = all air.
* Harness: `MC_VERSION` (+ `MC_NMP`, `MC_PROTOCOL` taken from `tools/mcgen/versions.json`); the protocol number is also the
  7th argument of the Firn client.

### Flattening (id<<4|meta -> 1.13+ states): state of the work
`tools/mcgen/legacy_flat.json` holds the hand written rules (legacy block name -> modern name + properties as a function of the
metadata nibble; 143 rules, own words, no GPL tables). The generator that turns it plus `blocks.json` of 1.12.2 and 1.20.1 into
`lib/mc/v340/mc_flat_v340.fi` and the application step (`mc_chunk` container remap) are **not written yet** (roadmap r18).
Known loss by design: properties that are not in the rules fall back to the default state of the matched block.
