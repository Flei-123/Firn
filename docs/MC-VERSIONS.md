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
| **1.8.9** | **47** | yes | yes (448 packets x 4 stages) | `CF_LEGACY_8` **implemented** (`mc_chunk.chunk_decode_legacy8`, `legacy8_bulk_*`) | **yes** (11.10.2026: real 1.8.9 server, dump byte-identical to the reference client, 81 chunks) |
| **1.12.2** | **340** | yes | yes (500) | `CF_SECTIONS_19` **implemented** (`mc_chunk.chunk_decode_19`) | **yes** (11.10.2026: real 1.12.2 server, dump byte-identical to the reference client, 81 chunks) |
| **1.16 / 1.16.1** | **735 / 736** | yes | yes (612) | `CF_SECTIONS_114` (ignoreOldData bool, 1024 biome ints, non-spanning longs) | **yes** (15.10.2026: real 1.16 + 1.16.1 servers, byte-identical, 121 chunks, entities equal) |
| **1.16.5** | **754** | yes | yes (616) | `CF_SECTIONS_114` **implemented** (`mc_chunk.chunk_decode_114`, protocols 751-754) | **yes** (11.10.2026: real 1.16.5 server, dump byte-identical to the reference client, 121 chunks) |
| **1.14.4** | **498** | yes | n/a | `CF_SECTIONS_114` (dense longs, 256 biome ints behind the sections, `chunk_decode_114n`) | **chunks yes** (14.10.2026: real 1.14.4 server, dump byte-identical to the reference client, 121 chunks, walk_fail 0); **entities yes** (15.10.2026: checksum c060ac148e36, entities EQUAL 3 vs 3) |
| **1.15.2** | **578** | yes | n/a | `CF_SECTIONS_114` (dense longs, 1024 biome ints as own field) | **chunks yes** (14.10.2026: real 1.15.2 server, byte-identical, 121 chunks, walk_fail 0); **entities yes** (15.10.2026: checksum 0d868aa1bd4f, entities EQUAL 3 vs 3) |
| **1.17 (1.17.0)** | **755** | yes | yes (660) | `CF_SECTIONS_114` (like 1.17.1; NO state id in window packets, destroy = ONE entity id) | **real server yes** (15.10.2026: A == A2, B == A, 121 chunks, checksum 2e260e004201, entities EQUAL 3 vs 3, walk_fail 0) |
| **1.17.1** | **756** | yes | n/a | `CF_SECTIONS_114` (BitSet mask, no groundUp, sections from the dimension) | **chunks yes** (14.10.2026: real 1.17.1 server, byte-identical, 121 chunks, walk_fail 0); **entities yes** (15.10.2026: checksum 6639bc9b4f14, entities EQUAL 3 vs 3) |
| **1.18 / 1.18.1** | **757** | yes | yes (664) | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` | **yes** (15.10.2026: real 1.18 + 1.18.1 servers, byte-identical, entities equal) |
| **1.18.2** | **758** | yes | yes | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` **implemented** | **yes** (14.10.2026: real 1.18.2 server, dump byte-identical to the reference client, 121 chunks) |
| **1.19** | **759** | yes | yes | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` **implemented** | **yes** (14.10.2026: real 1.19 server, dump byte-identical to the reference client, 121 chunks, light lines + entities equal) |
| **1.19.2** | **760** | yes | yes | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` **implemented** | **yes** (14.10.2026: real 1.19.2 server, dump byte-identical to the reference client, 121 chunks, light lines + entities equal) |
| **1.19.3** | **761** | yes | yes | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` **implemented** | **yes** (14.10.2026: real 1.19.3 server, dump byte-identical to the reference client, 121 chunks, light lines + entities equal) |
| **1.19.4** | **762** | yes | yes | `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES` **implemented** | **yes** (14.10.2026: real 1.19.4 server, byte-identical, 121 chunks, entities equal) |
| **1.20.1** | **763** | yes | yes (704) | `CF_CONTAINERS_118` **implemented** | in progress (M0) |
| **1.20.2** | **764** | yes (has the Configuration state) | yes (788) | `CF_CONTAINERS_118` (same, nameless NBT roots) | **yes** (11.10.2026: real 1.20.2 server incl. configuration phase, dump byte-identical, 109 chunks) |
| **1.21.1** | **767** | yes | yes (904) | `CF_CONTAINERS_118` **verified** | **yes** (11.10.2026: real 1.21.1 server, Known Packs answered, dump byte-identical to the reference, 109 chunks) |
| **1.21.4** | **769** | yes | yes (948) | `CF_CONTAINERS_118` **verified** | **yes** (11.10.2026: real 1.21.4 server, client information with particle status, dump byte-identical, 117 chunks) |
| **1.21.5** | **770** | yes | yes | `CF_CONTAINERS_118` + 1.21.5 shape (`FLAG_HM_MAP`) | Firn joins a real 1.21.5 server (15.10.2026, `tools/mc/play/run_hash.sh`: chunks decoded, inventory bad=0). No byte-identical dump: the reference client (node-minecraft-protocol) stops at 1.21.4. |
| **1.21.8** | **772** | yes | yes (980) | `CF_CONTAINERS_118` + 1.21.5 shape (`FLAG_HM_MAP`) **verified** | **yes** (11.10.2026: real 1.21.8 server, heightmap map + containers without length, dump byte-identical, 117 chunks) |
| 1.21.11 | 774 | yes | yes (1008) | `CF_CONTAINERS_118` + 1.21.5 shape: Firn joins and decodes all 117 chunks without a decode error, byte equality **proven 14.10.2026** (see below) | **yes** (14.10.2026: real 1.21.11 server, `MC_SETTLE_MS=90000` + `tick freeze`, A == A2, B == A, 117 chunks) |
| 26.1 | 775 (in minecraft-data) | yes | yes (1028) | `CF_CONTAINERS_118` + 1.21.5 shape + `FLAG_SEC_FLUID` (a second i16 after every block count): layout FOUND 14.10.2026 from raw bytes (see below); Firn decode rule built + unit tested (tests/2931), real-server byte equality **proven 14.10.2026** | **yes** (14.10.2026: real 26.1 server, `MC_SETTLE_MS=90000` + `tick freeze`, A == A2, B == A, 117 chunks incl. the fluid count) |

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
proven for these versions (join and chunk check: see the section below).

### 1.21.x chunk round (11.10.2026, real servers, `MC_VERSION=<rel> bash tools/mcclient/run.sh <client>`)

`CF_CONTAINERS_118` stays one function (`chunk_decode_118`); 1.21.5 changed two things, both behind
`FLAG_HM_MAP` (versions.json: 1.21.8, 1.21.11, 26.1): heightmaps are `varint count` + (`varint type`,
`varint longCount`, longs) instead of NBT (type 0 world_surface_wg, 1 world_surface, 2 ocean_floor_wg,
3 ocean_floor, 4 motion_blocking, 5 motion_blocking_no_leaves, names from minecraft-data), and a paletted
container carries no data array length (derived from the bits per entry; a single value container
ends after the value). Offline: `tools/mc/chunk/run.sh` builds both shapes by hand (`selftest map`) and
compares with `expected.py`. Join fixes found by the real servers: 1.20.5+ **Select Known Packs** must be
answered (empty list) or the server never sends the registries; 1.21.2+ **Client Information** has a trailing
particle status (without it the server falls back to view distance 2: 49 instead of 117 chunks).

| Version | Result |
|---|---|
| 1.18.2 (758) | **PASS (14.10.2026)**: A == A2, B == A, 121 chunks, checksum cddbe507a6b4. **Cause of `chunk_bad=121`**: map_chunk and update_light of 1.18 - 1.19.4 carry a `trustEdges` bool before the four light masks (minecraft-data lists it); the decoder did not read it, so the packet was not consumed exactly. Fix: `FLAG_TRUST_EDGES` (versions.json 1.18.2/1.19.4 -> `mc_chunk.Scratch.trust`, `lightdata_decode_118(.., trust_edges)`); test 2952. 1.20 (763) dropped the bool. |
| 1.19 (759) | **PASS (14.10.2026)**: A == A2, B == A, 121 chunks, walk_fail 0, checksum af3c8fcff1df, L (light) lines and entities equal; added by r125 (versions.json only + generator, no decoder change). |
| 1.19.2 (760) | **PASS (14.10.2026)**: A == A2, B == A, 121 chunks, walk_fail 0, checksum 07e46fcf26c5, L (light) lines and entities equal; added by r125 (versions.json only + generator, no decoder change). |
| 1.19.3 (761) | **PASS (14.10.2026)**: A == A2, B == A, 121 chunks, walk_fail 0, checksum e6ac09767d8c, L (light) lines and entities equal; added by r125 (versions.json only + generator, no decoder change). |
| 1.19.4 (762) | **PASS (14.10.2026)**: A == A2, B == A, 121 chunks, checksum 0fb83124ffed, entities equal; same fix. The earlier A != A2 did not reproduce with the existing gamerule block (randomTickSpeed 0, doMobSpawning false, doDaylightCycle false) and the default settle run. |
| 1.21.1 (767) | PASS: A == A2, B == A, 109 chunks, byte identical |
| 1.21.4 (769) | PASS: 117 chunks, byte identical |
| 1.21.8 (772) | PASS: 117 chunks, byte identical (reference reads the new shape: `MC_PROTOCOL >= 770` in `ref_dump.cjs`) |
| 1.21.11 (774) | **PASS (14.10.2026)**: A == A2, B == A, 117 chunks, checksum df43cabe8ba1 (`MC_VERSION=1.21.11 MC_ENTITIES=0 MC_SETTLE_MS=90000 JAVA=/root/tools/jdk25/bin/java`). Earlier A != A2 came from a still-running world (fluids / scheduled ticks), not from the decoder: `run.sh` now sends `tick freeze` after the settle run (protocol >= 765, default with `MC_ENTITIES=0`, `MC_FREEZE=0` turns it off) and kills natural mobs once (`MC_CLEAR_MOBS`). |
| 1.21.11 (774) | (14.10.2026: A vs A2 still differ in the SAME few sections with a 30 s settle run (`S -2 -2 1`, `S 1 0 1`, `S 5 4 1`: block count and distinct grow from session to session = something still flows / grows underground; 1.21.8 shows the same sections with a 20 s settle) -> settle time, not the gamerule names; see the settle note below) Firn: 117 chunks, no decode or walk error. The reference needed an `lpVec3` reader for nmp 1.54 (added in `ref_dump.cjs`). A vs A2 differ in a few sections (world not frozen: the game rules were renamed to snake_case in 1.21.11; `run.sh` now sends both spellings) -> equality **not proven** |
| 26.1 (775) | **PASS (14.10.2026)**: A == A2, B == A, 117 chunks, checksum c060f071ed56 (same flags as 1.21.11); this proves `FLAG_SEC_FLUID` (second i16 per section) against a real server. |
| 26.1 (775) | **Cause found (14.10.2026)**: every section starts with `blockCount` (i16) followed by a **second i16 = non-empty fluid count** (e.g. 0 deep down, 31 / 146 / 1415 in water sections), then the two paletted containers exactly as in 1.21.5+ (no data array length). Proof: the raw `chunkData` of a real 26.1 server (18773 octets, `MC_REF_DEBUG_FILE`) parses with 24 sections and is consumed to the LAST octet only with this extra i16; without it the reference fails with "palette index out of range" / "long array past end". Implemented as `mc_version.FLAG_SEC_FLUID` (versions.json flag of 26.1, read in `mc_chunk.read_sections`, skipped in `ref_dump.cjs` for protocol >= 775); the dump contract is unchanged (fluid count is not dumped). |
| 1.21.5 (770), 1.21.6/7 | no table in this repo (versions.json has 1.21.8 for 772); not tested |
| 26.2 | protocol number unknown here; jar exists in the piston manifest, minecraft-data does not know it |

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

## 7. Schema-driven packet writer (`lib/mc/mc_encode.fi`, roadmap r16: DONE)

`mc_encode.encode(schema, values, len, out, err)` builds a packet body from the same generated programs the
walker reads, for every version in `versions.json` (no hand written per-version writer). Value model
(full description at the top of `mc_encode.fi`): a text store of `path=value` lines in the walker's dump paths
(`a.b[2].c`); integers decimal/hex (range checked), bool, floats as raw bits `0x..`, `uuid:..`, strings `"..."`
(`\xHH` escapes), buffers / NBT / lpVec3 as `hex:..`, array counts `path=#N` (optional, else counted), options
`path?=0|1` (else present iff some line starts with the path), holders `path.<idName>=N` or the inline value under
`path.<inlineName>`, holder sets `path.<tag>` or `path.<list>=#N` + `[i]`. The encoder takes the lines in order
(cursor) and searches the store when the cursor misses, so hand written stores may be unordered; unused lines
are ignored (one store can serve several versions, e.g. Client Information). The result is checked with
`mc_schema.walk` before it is returned. `mc_schema.walk_values` is the lossless decoder (same format).
* Test: `bash tools/mc/encode/run.sh` -- per version every fixture of `gen_fixtures.cjs` (both directions):
  decode -> encode must give the reference bytes, also with the store reversed, plus damaged stores
  (never crash). Known answers and refusals: `tests/2651_mc_encode_basic.fi`.
* Client: `mc_client.client_send_values(c, state, sem, &store)` (sem -> wire id and schema per version).
  Handshake, Login Start, Encryption Response, Teleport Confirm, Position+Look, Chunk Batch Received and Client
  Information all use it (14.10.2026, r32: the hand built bodies and fallbacks are gone; a layout the version's schema
  does not fit is refused, never sent).
* Limits: topBitSetTerminatedArray patches bit 7 of the first octet of an element (an i8 in all versions);
  unlisted bits of a bitflags field are written as 0.

## 8. Round 2026-10-11 (worker voxel-client-1-12-2-chunks-join-1-20)

Measured against the OFFICIAL servers (jar from piston-meta, SHA-1 checked, offline mode, Java 17, seed 4242,
view distance 4), `MC_VERSION=<release> bash tools/mcclient/run.sh <firn client>`: reference client A (node-minecraft-protocol
+ independent decoder in `tools/mcclient/ref_dump.cjs`) == A2 and Firn client B == A.

* **1.12.2 (340)**: PASS, checksum `cea711cdf4a3...`, 81 chunks, walk_fail 0. New: `mc_chunk.chunk_decode_19`
  (bits per block u8, palette, packed longs **that may span two longs**, block light, sky light only in the overworld,
  256 biome octets of a ground-up column). The dimension comes from Join Game / Respawn
  (`mc_dimension.dim_legacy_id`, `mc_chunk.scratch_set_sky`). Values stay `id << 4 | meta` (the `mc_reg_v340` states).
  Client Information is sent in the 1.9-1.16 layout (`client_send_settings` tries the 1.17+ layout first; the schema check
  refuses what does not fit). A section-only update (`groundUp = false`) is counted
  (`partial_chunks`) and dropped in both clients; the library merge exists since the flattening round (see below).
  **Since r33 (14.10.2026)** both clients MERGE a section update into the stored column (demo: `mc_world.world_apply`, reference:
  `mergeUpdate` in `ref_dump.cjs`); the metrics show `merged_chunks` next to `partial_chunks` (update for an unknown column = dropped).
  `MC_FLATTEN=1` (1.12.2): the demo remaps `id << 4 | meta` to 1.20.1 states with `mc_flat`, the reference with the independent
  `tools/mc/flat/ref_flat.cjs`.
* **1.20.2 (764)**: PASS, checksum `87ad3f91c1ad...`, 109 chunks. Bug found by the real server and fixed: from 1.20.2 the
  NBT roots in the chunk packet (heightmaps, block entities) have **no name** (`FLAG_NBT_ANON`);
  `chunk_decode_anon` honours the flag, `chunk_decode_for` passes it. The configuration phase
  (Login Acknowledged -> Configuration -> Finish) worked unchanged against the real server.
* Dump format for `CF_SECTIONS_19` (demos/mcclient/DESIGN.md): `S` lines without `biomes=`, one `B x z <h64>` line per ground-up
  column (FNV-1a over the 256 biome octets as u16 LE), `count` = number of non-air blocks, absent sections = all air.
* Harness: `MC_VERSION` (+ `MC_NMP`, `MC_PROTOCOL` taken from `tools/mcgen/versions.json`); the protocol number is also the
  7th argument of the Firn client.

### Flattening (id<<4|meta -> 1.13+ states): done (2026-10-11, worker mc-w1)
* Rules: `tools/mcgen/legacy_flat.json` (143 rules, own words, no GPL tables). Generator: `tools/mcgen/gen_flat.py <minecraft-data/data> lib/mc/v340/mc_flat_v340.fi`
  (1.12.2 `blocks.json` ids/names + 1.20.1 `blocks.json` states). Reader: `lib/mc/mc_flat.fi` (`flat_open` -> heap `u16[4096]`).
* Target = the **1.20.1 global block state id** (max 19216 for 1.12.2 blocks, fits the 15 bit limit). State id = `minStateId + sum(index * stride)`, first property most
  significant, bool order `[true, false]`.
* Decisions: a legacy id without a block -> 0 (air); a block without a rule keeps its name + default state; properties listed in a rule with several targets
  (`names`/`by_meta`) are skipped for targets that lack them (torch: `facing` only on `wall_torch`); `props` without `map`/`offset` use the bits as the value.
  **Unknown meta combination** (index beyond `names`/`by_meta`/`map`, or illegal value) -> the result for **meta 0 of the same id** (91 of 4096 entries, e.g. sapling meta 6/7).
  Lost by design: properties not in the rules (redstone wire power/shape, fence/wall connections, stair shape) keep the default.
* Application: `mc_chunk.container_remap` / `chunk_remap_blocks(c, sc, table, n)` map the palette (rebuild with distinct entries and fewer bits when two legacy states
  collapse; a value >= table size -> air) and recount `Section.count`. Opt-in: the dump path of the demo client still prints the raw `id << 4 | meta`.
* Tests: `bash tools/mc/flat/run.sh` (Firn table == independent Node implementation `ref_flat.cjs`, 4096 entries, three build stages, counter-check),
  `tests/2746_mc_flat_remap.fi`.

### Section updates (CF_SECTIONS_19, groundUp = false): merged in the library
`Chunk.sec_mask` says which sections a packet carried. `mc_world.world_apply(w, p)` stores a whole column (`APPLY_STORED`), merges an update into the stored column
(`APPLY_MERGED`, `mc_chunk.chunk_merge_update`: the sections of the mask replace the stored ones; biomes/heightmaps/block entities stay, `light_hash` stays that of
the original packet) or drops an update for an unknown column (`APPLY_DROPPED`). `chunk_decode_19` now also requires the section data to be consumed exactly.
`tests/3211_mc_chunk19.fi` covers decode (5/13/4 bit, spanning longs, sky/no sky, biomes, truncation) and the merge. **Open**: the demo client + `ref_dump.cjs` still drop updates
(`partial_chunks`); switching both to merge changes the dump contract (done in r33, both clients at once, see 1.12.2 above).

## 9. Round 2026-10-11 (worker mc-w3-1-16-5-join-cf-sections-114)

* **1.16.5 (754)**: PASS against the OFFICIAL 1.16.5 server (`MC_VERSION=1.16.5 MC_SETTLE_MS=40000 bash tools/mcclient/run.sh <client>`),
  checksum `6670360b0dd2...`, 121 chunks, walk_fail 0, A == A2 and B == A. No change was needed in the state machine or the schema walker.
* New: `mc_chunk.chunk_decode_114` (CF_SECTIONS_114, protocols 751-754 = 1.16.2 - 1.16.5): groundUp, bitMap (VarInt, bit i = section i),
  heightmaps as a named NBT compound (37 longs of 9 bits, 7 per long), biomes as `varint count (1024) + varints` ONLY in a ground-up column,
  chunkData with `blockCount` (i16) + paletted blocks (indirect 4-8 bit, direct 15 bit) whose longs **never span** (same layout as
  `container_decode`), block entities as named NBT. Block ids are already flat states: no flattening.
* Fixed 16 sections (a 1.16 column is 256 high; the dimension height of Join Game is not read for this format).
  The 1024 biome ints are ordered `(y*4+z)*4+x`, so section i owns entries `i*64 .. i*64+63` = the neutral `Section.biomes`;
  the dump keeps the per-section `biomes=` hash (no `B` line).
* Light is NOT part of the chunk packet (separate `update_light`, not decoded): `L` lines show `sky=0 block=0` and the FNV offset basis
  in both clients. A section-only update (`groundUp = false`) is dropped in both (`partial_chunks`), like for 1.12.2.
* NOT covered (decoder refuses with Range): 1.14 / 1.15 (spanning longs, 256 biome ints in 1.14), 1.16.0 / 1.16.1 (extra ignoreOldData bool),
  1.17+ (long-array bitmask; 1.17 still uses this format family). Offline test: `tests/2745_mc_chunk114.fi` (hand-built packet, every block,
  biome, heightmap value, counters, error and truncation paths).

## 10. Round 2026-10-11 (worker mc-w4-1-8-9-join-cf-legacy-8): 1.8.9

* **Chunk format `CF_LEGACY_8`** (`lib/mc/mc_chunk.fi`, own self contained functions): `map_chunk` = x, z (i32), groundUp, bitMap
  (**u16**), chunkData (varint length + octets); **no block entities** in the packet. Data order is per ARRAY, not per section:
  all present sections' blocks (4096 x u16 **little endian**, `id << 4 | meta`), then all block light (2048 per section), then all sky light
  (only if the dimension has a sky), then (groundUp) 256 biome octets. `map_chunk_bulk` (0x26): skyLightSent, count, count x
  {x, z, bitMap}, then the columns' data back to back (every column ground-up, so every column has biomes). **There is no unload
  packet**: an unload is a `map_chunk` with groundUp = true, bitMap = 0 and EMPTY data (no biome octets either);
  `chunk_is_unload_legacy8(c)` tells, the client removes the column (`mc_world.world_remove`). A block value >= 32768 (id >= 2048)
  is rejected (the neutral model's id table), vanilla 1.8 stays far below.
* API: `chunk_decode_legacy8(b, sc, c)` for `map_chunk` (reached through `chunk_decode_for`), for the bulk
  `legacy8_bulk_open(b, &h)` -> `legacy8_bulk_next(b, &h, sc, c)` x `h.n` -> `legacy8_bulk_complete`; `legacy8_bulk_free`. Sky light of a
  single `map_chunk` comes from the dimension (`scratch_set_sky`, Join Game `dimension` is an **i8** in 1.8, Respawn an i32:
  `mc_dimension.dim_legacy_id` takes both); the data length is checked EXACTLY (a wrong sky assumption is an error, never a guess).
  Light is hashed in wire order (all block light, then all sky light) in both clients.
* Login: Set Compression, Login Start has no UUID, Login Success carries the UUID as a string (not parsed). Client Information has no
  main hand (3rd layout tried by `client_send_settings`). No teleport confirm in 1.8 (`SEM_PLAY_C2S_TELEPORT_CONFIRM` is -1); the
  Position packet is answered with Position+Look. New semantic packet `SEM_PLAY_S2C_MAP_CHUNK_BULK` (`semantic.json`).
* **Server setup**: the official 1.8.9 server runs on Java 17 only with `use-native-transport=false` in `server.properties`
  (otherwise Netty epoll dies with "Unable to access address of buffer" and every connection is reset). `tools/mcclient/run.sh` sets it for 1.8.x.
* Tests: `tests/2747_mc_chunk_legacy8.fi` (hand built packets: values, light hash, biomes, no-sky, wrong sky, truncations, unload, bulk).
  Real server (official 1.8.9, Java 17, seed 4242, view 4, `MC_SETTLE_MS=40000`): PASS, A == A2 and Firn B == A byte identical,
  checksum `56c7050923f4...`, 81 chunks (80 columns in 8 `map_chunk_bulk` packets + 1 single `map_chunk`), walk_fail 0.
* Section updates (groundUp = false) are counted and dropped in both clients, like for 1.12.2 (merge still open, r30).


## 11. Round 2026-10-14 (worker mc-f2-chunk-decode-1-14-1-15-1-17-r3): 1.14.4 / 1.15.2 / 1.17.1

* `mc_chunk.chunk_decode_114n(b, sc, c, protocol, nsec)` decodes four wire variants of `CF_SECTIONS_114` by protocol: 1.14 (477-498), 1.15 (573-578),
  1.16.2-1.16.5 (751-754, unchanged), 1.17 (755-756). 1.14/1.15: entries SPAN two longs (also the 36-long heightmap), block bits < 4 = 4, direct ids >= 9 bits
  (no palette). 1.14: biomes = 256 ints behind the sections (kept in `Chunk.bio`, dump has a `B` line, cells take the first column); 1.15: 1024 ints as an
  own field. 1.17: no groundUp, bitMap = varint count + longs, biomes varint count = sections * 64, sections from the dimension (`mc_dimension.dim_from_join`
  now reads the `dimension` element NBT of 1.16.2-1.17.1; 1.14-1.16 = 0..256). 1.17 update_light uses long-array masks (`world_update_light`).
* Real servers (official jars, `MC_VERSION=<v> heavy bash tools/mcclient/run.sh`): 1.14.4 checksum 7c79e75439d9, 1.15.2 1d09db01f71f, 1.17.1 ea1e01fbc284:
  A == A2 and B == A (full file, C/S/H, L lines), 121 chunks, walk_fail 0. run.sh still ends FAIL(1) because the entity scenario creates too few entities
  (server: "Incorrect argument for command" for some scenario commands on 1.14-1.17) -> entities NOT proven for these versions (open).
* Tests: `tests/3040_mc_chunk_old.fi` (hand-built packets of all three variants, error and truncation paths); `tests/2745` now refuses 735 instead of 755.
* NOT covered: 1.16.0 / 1.16.1 (ignoreOldData bool, 1024 biome ints).

## Hashed item stacks (1.21.5+, protocol 770+)

Click Container sends the changed slots and the carried stack as *hashed* stacks: `bool present, varint item, varint count, varint n_added +
(varint component type, i32 CRC32C)*, varint n_removed + varint type*`. Set Creative Slot keeps the plain *untrusted* stack (components with
their real values), so it needs no hashes. The server hashes ITS stack the same way and, when the hash differs, re-sends the slot (never a kick).

* `lib/mc/mc_hash.fi`: CRC32C, the tagged hashing ops (empty / bool / byte / short / int / long / float / string (UTF-16LE with length) / list /
  map (entries sorted by key hash, then value hash, unsigned)) and the wire struct `HStack` (`hstack_put` / `hstack_get`). Test 3300.
* `lib/mc/mc_inv.fi`: while a Window Items / Set Slot / Set Cursor Item is parsed, the value of every component we can hash is hashed
  (`hc_data`: max_stack_size, max_damage, damage, repair_cost = int; unbreakable = empty map; custom_name / item_name = plain string name;
  rarity = enum name; enchantment_glint_override = bool; enchantments = map registry-key-string -> int level). The patch (types + hashes + removed
  types) is kept per stack signature (`inv_patch_of`), so a stack that is moved, split or swapped keeps its hashes. Round s7 added the NBT based ones: `custom_data` (the NbtOps -> HashOps
  conversion keeps every tag type: byte / short / int / long / float / double / string, list = list, compound = map), `lore` (list of text components) and
  styled `custom_name` / `item_name` (compound). A text component is hashed by the component codec straight into the hashing ops, so its style flags
  (`bold`, `italic`, `underlined`, `strikethrough`, `obfuscated`) are booleans (tag 0D), `extra` is a list of components. At most 8 components per stack
  are hashed; byte / int / long arrays and compounds with > 64 entries are not (the slot is then re-sent). `food`, `tool` and `attribute_modifiers` (round s9) are hashed too, see section 14. NOT hashed yet: the rest (a visible resync only). Test 3390 (expected values from an independent Python implementation), live set B of `run_hash.sh`.
* The enchantment key hashes come from the Registry Data (`mc_client.client_enchant_keys`, index = registry id).
* Live proof: `bash tools/mc/play/run_hash.sh [version]` (a pickaxe with six components + a plain stack, pick up / put back). Run 1 must see 0 server
  re-sends, run 2 (`MCPLAY_HASH_OFF=1`, no hashes) must see some, so the check measures something. 1.21.5 / 1.21.8 / 1.21.11 / 26.1 see below.
* 1.16.0 / 1.16.1: see section 12.

## 12. Round 2026-10-15 (worker mc-s4-versionen-1-18-x-1-19-4-1-16-0): 1.18 / 1.18.1 / 1.16.0 / 1.16.1

* r141 (`chunk_bad=121` on 1.18.2 / 1.19.4) was already fixed on 14.10.2026 (trustEdges bool, section 10); not reproduced, only re-checked.
* New versions in `tools/mcgen/versions.json`: 1.18 + 1.18.1 (both protocol 757, same wire as 1.18.2: `CF_CONTAINERS_118` + `FLAG_TRUST_EDGES`), 1.16 (735) and
  1.16.1 (736) (`CF_SECTIONS_114`, flags like 1.16.5, resource pack_format 5). Several game versions may share one protocol: `gen_protocol.py --all` keeps the FIRST key per
  protocol (1.18 -> v757). nmp has no 1.18 / 1.16.0 / 1.16.1 in `supportedVersions`, but minecraft-data has them and the reference client runs with `MC_NMP=<version>`.
* 1.16.0 / 1.16.1 chunk wire (`chunk_decode_114n`, `p114_is_16o` 735-736): x, z, groundUp, **ignoreOldData** bool, bitMap varint, heightmaps (sparse, NOT spanning),
  1024 biome ints without a count (ground-up only), chunkData, block entities. `ref_dump.cjs` takes 735/736 into `SECTIONS_114`. 1.16.2 (751) dropped the bool and made the biomes a varint array.
* Real servers (`MC_VERSION=<v> heavy bash tools/mcclient/run.sh <client>`), A == A2, B == A, entities EQUAL (3 vs 3), walk_fail 0:
  1.18.1 checksum e24dc48561e1 (121 chunks), 1.18 d9a4afcdc7ff (97 chunks, the 1.18.0 server sends fewer), 1.16.1 ee18effa76bd (121), 1.16 17f3cc301a6a (121).
* Gate: `tools/mc/schema/run.sh <firnc> 757 | 735 | 736` ok (664 / 612 / 612 fixtures x 4 stages). Tests: `tests/3040` variant 16 (1.16.1 hand built, error paths);
  `tests/2745` and `3040` now refuse 734 instead of 735.

## 13. Round 2026-10-15 (worker mc-s6-versionen-1-17-0-entity-szenar): entity scenario 1.14.4 / 1.15.2 / 1.17.1, 1.17.0, r210

* Entity scenario on old versions: it already works (`BOT_TP` next to the dumper); the "Incorrect argument for command" lines are only the gamerule
  names that do not exist yet (`doInsomnia`, snake_case names ...) and `No entity was found` is `kill @e[type=item]` with no item. `MC_SETTLE_MS` must stay at the default (45 s):
  with 20 s the fluids are still moving and A != A2. Real servers, full run, `RESULT: PASS` (A == A2, B == A, entities EQUAL 3 vs 3, 121 chunks):
  1.14.4 c060ac148e36, 1.15.2 0d868aa1bd4f, 1.17.1 6639bc9b4f14.
* **1.17.0 (755)** in `versions.json` (`CF_SECTIONS_114`, pack_format 7, nmp "1.17"; `gen_protocol.py --all` -> `lib/mc/v755`, only that dir + layout/versions/blocktex tables changed).
  Differences to 1.17.1: no state id in Window Items / Set Slot / Click (`mc_inv` already used `protocol >= 756`), and **Destroy Entity (`destroy_entity`) carries ONE entity id**
  (1.17.1 is back to the `entity_destroy` array). Bug found by the live run: the semantic id `PLAY_S2C_ENTITY_DESTROY` was -1 for 755, so no entity was ever removed.
  Fix: `semantic.json` also lists `destroy_entity`, `gen_entity.py` emits the layout token `Ev` for it (array = `Av`), `mc_entity.do_destroy(.., single)` reads one id,
  `ref_entities.cjs` (reference tracker) handles `destroy_entity`. Test `tests/3370_mc_v755.fi` (lookup, layouts, spawn + single destroy). Gate `tools/mc/schema/run.sh <firnc> 755` ok (660 fixtures x 4 stages).
* **r210 (B has +11 chunks in the rider run), cause found**: a first-time player spawns at a RANDOM spot within the spawn radius, so the fixed ride (+3/+4, then -2/+1) crosses a chunk border in
  some runs. The server then sends one extra chunk column, but only if its load / send is fast enough while the dumper is away (timing) - and both dumpers keep the UNION of every column
  that was sent (unload packets ignored by design, `ref_dump.cjs` header). B (fast, no connect delay) got the column (`chunk_packets=131 unloads=22`), A / A2 did not. Fix in
  `tools/mcclient/run.sh`: the ride moves away from the nearer chunk border on both axes (start position from the server log), so it never leaves the start chunk. `MC_ENT_RIDER=1` is deterministic now.

## 14. Round 2026-10-15 (worker mc-s9-hash-rest-gesamtgate): food / tool / attribute_modifiers hashes, 1.21.5 byte dump, containers

* **Hashes (r214)**, `lib/mc/mc_hash.fi` (`hash_food`, `hash_tool_rule`, `hash_tool`, `hash_attr_entry`, `hash_blocks_tag/ids`) + `mc_inv.xs_line/xs_finish` (dump lines of the
  component are collected in `Inv.xscr` and hashed when the next component or the end of the stack comes). Records are maps; a field of `optionalFieldOf(name, default)` is
  omitted when it equals the default (`can_always_eat` false, `default_mining_speed` 1.0, `damage_per_block` 1, `can_destroy_blocks_in_creative` true, attribute `slot` any,
  `display` default), an empty `Optional` (rule `speed`, `correct_for_drops`) too. Tool block sets: tag = string `"#<tag>"`, direct ids = the single key string or a list of
  key strings (names via the item registry, `minecraft:` prefix). Attribute `type` = registry key (`lib/mc/mc_attr.fi`, generated by `tools/mcgen/gen_attr.py` from
  minecraft-data `attributes.json`, order = registry id, verified live with `water_movement_efficiency` = id 31), `id` = string, `amount` = double, `operation` =
  `add_value | add_multiplied_base | add_multiplied_total`, `slot` = `any | mainhand | offhand | hand | feet | legs | chest | head | armor | body | saddle`, `display`
  (1.21.6+) hidden = `{type:"hidden"}`; `override` (text component) is not hashable (slot re-sent). Component ids: 770/772 attribute_modifiers 13, tool 25, food 20;
  774/775 16, 28, 23. Live `run_hash.sh` sets C (everything set) and D (all omitted defaults, other slots / operations, hidden display) plus a control run without hashes:
  0 re-sends on 1.21.5 / 1.21.8 / 26.1, the control re-sends. Test `tests/3430` (values from an independent Python implementation).
* **r208 (1.21.5 byte dump)**: `MC_VERSION=1.21.5 MC_ENTITIES=0 JAVA=jdk25 tools/mcclient/run.sh <client>` PASS (A == A2, B == A, 117 chunks). The only difference was the
  heightmap NAME in the reference dump (minecraft-data 1.21.5 has no name mapper, it printed the numeric enum id 1 / 4 / 5); `ref_dump.cjs` maps the ids now.
* **r215 containers**: crafting table (10 slots, result 0) and furnace / blast furnace / smoker (3 slots, result 2) are tracked (menu ids 11 / 13 / 9 / 21, +1 from protocol 765),
  `Inv.win_res` = the result slot: nothing can be placed into it, clicks on it and shift clicks in these windows are not predicted (the server answers). Shulker box (27) and
  double chest (54, "Large Chest") live in `run_cont.sh`.
* **Gate**: stage `ents` of `tools/mc/run.sh` now also runs the real-server entity scenario with `MC_ENT_RIDER=1` (`tools/mcclient/run.sh`, 1.20.1).
