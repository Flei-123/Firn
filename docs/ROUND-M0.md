# ROUND M0 -- Minecraft protocol prototype in Firn (1.20.1, protocol 763), built as libraries

Date 2026-10-11. Branch `w-mcclient`, merged to `main`. Machine: 20 cores, Linux, `firnc` release build of 2026-10-08, Java 17 for the test server.

## What was decided during the round

The first plan was a client. The owner then changed the order: **libraries first, the client is only the final integration test**, and every library
has to be **version-modular** (version = data + small version modules, docs/MC-VERSIONS.md). The result is `lib/mc/` (20 hand-written libraries,
5 generated game versions) and `demos/mcclient` as a thin program on top of it. Library list and status: `docs/MC-LIBS.md`, one API doc per library in `docs/mc/`.

## Acceptance (M0)

A Firn program joins a **real official 1.20.1 server** (offline mode, `Starting minecraft server version 1.20.1`, jar checked against the SHA-1 of the
launcher manifest, downloaded to a temp directory, never read), decodes every Play packet, turns the chunk columns into palette sections and writes a
dump with a SHA-256 checksum. An independent client (node-minecraft-protocol with its own chunk decoder, `tools/mcclient/ref_dump.cjs`) does the same on
the same server with the same player name; the dumps must be byte identical.

| Run (`tools/mcclient/run.sh`, via `heavy`) | reference A == A2 | Firn B == A | chunks | checksum |
|---|---|---|---|---|
| compression threshold 256 (default) | yes | **byte identical** | 109 | `7bf84dd7cfce...` |
| compression off (`--no-compression`) | yes | **byte identical** | 109 | `0b2e6ca6e357...` |
| earlier runs while debugging | yes | identical in the two runs after the fixes | 109 | `211a278d24b5...` |

Dump = 109 chunk columns x 24 sections (the reference client independently arrived at height / 16 from the dimension too): block and biome hashes of every section (resolved global ids), heightmaps, light arrays (FNV-1a over the bytes),
block-entity count. The light lines were stable between sessions too (`L (light) lines: EQUAL`). The server world is created fresh per run (fixed seed,
gamerules frozen by the harness), so the checksum differs between runs but never within a run.

### Measurements (this server, localhost)

| | reference (node) | Firn |
|---|---|---|
| TCP connect | 525-620 ms (node start included) | 0 ms |
| Join Game decoded | 660-810 ms | 43-62 ms (own server: 60-276 ms) |
| first chunk | 710-890 ms | 62-95 ms |
| 109 chunks (first to last chunk packet) | - | 465-487 ms = **~230 chunks/s**, ~8 MB/s of inflated data |
| total (includes the 2.5 s idle rule) | 4.9-5.3 s | 3.2 s |
| packets / bytes in / inflated | 327-378 / 534 KB / 3.89 MB | 278-304 / 533 KB / 3.88 MB |
| packets walked by the schema walker | 327 | 276-303 walked OK, **0 failed** |

The Firn numbers are dominated by the server's send rate, not by the client (the client keeps up; see the library benchmarks below).

## What ran against a real server and what did not

**Real server (1.20.1, offline mode):** handshake, login (+ Set Compression, threshold 256 and off), the whole Play start (~300 packets, every one walked with
its generated schema, 0 failures), keep-alive / teleport / settings replies, chunk decoding of 109 columns, the dimension height from the Join Game codec,
the server list ping (`mc_net`: protocol 763, "1.20.1", 0/4 players, MOTD, latency).

**Against independent references only (no Minecraft server involved):** AES-128-CFB8 and zlib framing (python peer), RSA / CFB8 (node crypto), registries of 5 versions
(minecraft-data + prismarine-block), NBT (prismarine-nbt), packet layouts of 5 versions (protodef serializer), chat components (python), noise and RNGs
(real `java.util.Random`, python), Anvil files (python), meshes (python brute force).

**Only compiled or mocked -- NOT proven:** the configuration phase of 1.20.2+ (`mc_client`), online-mode login (encryption request path of `mc_client` is not
wired; `mc_auth` session join was tested against a local mock, the Microsoft / Xbox chain was never run against the live services -- no account), any version
other than 1.20.1 joining a server (tables, walker and registries exist for 1.8.9, 1.12.2, 1.16.5, 1.20.2; their chunk formats are not implemented),
the `lpVec3` / `registryEntryHolderSet` types of 1.21.x / 26.1 (generator stops there).

## Findings

Protocol (measured on the real server):
1. **Client Information (Settings) sent right after Login Success -- before the first Play packet is read -- makes the vanilla 1.20.1 server drop the connection** with
   `DecoderException ... Index 8 out of bounds for length 3`. The same bytes after Join Game are accepted. (Reproduced with a 30 line python client.)
2. **`chunkData` is padded with zero octets after the last section** (16 octets after the 24 sections of an overworld column). The section count must come from the
   dimension (`min_y`, `height` in the Join Game `dimensionCodec`), not from "until the buffer ends".
3. The `bundle_delimiter` packet (id 0) and the empty-body packets walk fine; the real stream contained none of the schema failures the fixtures could not produce.

Firn (details with repros in `/root/projects/minecraft-neu/docs/BLOCKER.md`):
* `size_of[T]()` of a struct declared in an imported module: "unknown type" (workaround: stride of a two element array).
* Module names are the last path component, so `lib/mc/text.fi` would clash with `std.text`: every library is `mc_<name>.fi`.
* Plain `* + -` abort on overflow and `as` is a checked value cast: hashes use `*%`, reinterpretations use `bytes.bits_*`.
* String literals are not NUL terminated (`"abc".n`, not `rt.c_length`); a literal `{` is written `\x7b`.
* The escape analysis refuses storing `&local` through a pointer (heap-allocate or use `#[allow_escape]`).
* `javac` scans the current directory and dies on the stray binary-named file in the repo root: run it with `-cp <dir>`.

Generator (new protodef constructs found by the second version): the fields of an anonymous container are visible to the siblings that follow it
(1.12.2 entity metadata); newer versions need `registryEntryHolderSet` (1.21.x) and `lpVec3` (26.1).

## Size and tests

* `lib/mc`: 8949 lines hand-written in 20 libraries (+ 2219 lines / 1.4 MB generated tables for 5 versions); `demos/mcclient` 723 lines (client 345, dump 174, self test 204);
  `tools/mc` + `tools/mcgen` + `tools/mcclient` 8591 lines of generators, references and harnesses; `tests/*_mc_*.fi` 2485 lines in 16 files; 17 API docs in `docs/mc/`.
* `bash tools/mc/run.sh` (also section 38b of `test.sh`): 16 unit test files x 3 build stages, 17 cross-check scripts, format check. All green at the merge
  (`RESULT mc: ok`). Compile time of the whole 5-version library set: 0.6 s.
* Not part of `test.sh`: the end-to-end run against the real server (`tools/mcclient/run.sh`, needs Java and the official jar download).

## Library benchmarks (release-fast)

| Library | Number |
|---|---|
| `mc_schema` walker | 0.75-0.9 M packets/s on the reference fixtures, 260 MB/s on the largest packet |
| `mc_noise` | improved noise 6.2 M samples/s, simplex2 7.3 M/s, Java LCG 73 M ints/s, xoshiro256** 88 M/s |
| `mc_mesh` | terrain-like section ~1340 sections/s (800 quads), dense random section ~310/s (7700 quads) |
| `mc_codec` | 7.2 MB of mixed packets echoed in ~1.5 s including the python peer |
| `mc_chunk` + client | 109 columns decoded within the 470 ms the server needs to send them |

Worker-owned libraries (`mc_crypto`, `mc_auth`, `mc_packet`, `mc_math`) carry their own measurements in `docs/mc/{crypto,auth,packet,math}.md`.

## Next steps

1. Online login end to end (needs a real Microsoft account and a Mojang-authenticated server; cannot be done without Justin).
2. A second chunk strategy (`CF_SECTIONS_19`, covers 1.9 - 1.12.2) plus the pre-flattening block mapping -- the first real proof of the version modularity with a join.
3. Schema-driven writers for client packets (today hand built and held against the schema before sending).
4. Generator support for `lpVec3` / `registryEntryHolderSet`, then 1.21.x / 26.1 (and a newer `minecraft-data` for "26.2": its protocol number is not known here).
5. M3 (renderer) is blocked by the desktop GPU path (H3); `mc_mesh` and `mc_math` are ready for it.
