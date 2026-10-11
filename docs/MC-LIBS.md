# MC-LIBS -- the libraries a Minecraft client / server needs, in Firn

Status: 2026-10-11. Source of requirements: `GDD-SDD.md` (chapters 4, 6, 8, 14,
appendix A) and `FIRN-GAPS.md` in `/root/projects/minecraft-neu/docs/`.
Decision of the owner (2026-10-11): **libraries first, the client is only the final
integration test.** `demos/mcclient` stays a thin program on top of the libraries.

Clean room: protocol and registry data come from `minecraft-data` (PrismarineJS, MIT),
reference implementations are python / node (`node-minecraft-protocol`, `crypto`) or
independent re-implementations from public format descriptions. No Mojang code or files
are copied, read or decompiled; the official server JAR is only a *test subject*.

## Design rule: version-modular (binding for every lib, decided 2026-10-11)

Further game versions (1.12.2, 1.8.9, 1.16.5, newer releases, "26.x") must be addable later
without touching the core. Version = data + small version modules; see
**`docs/MC-VERSIONS.md`** (principle, checklist, theoretical 1.12.2 walk-through, status).
For library authors that means:

* No protocol number, packet id, field layout, block id or chunk layout in an engine lib.
  Take a `mc_version.Version` (flags, semantic packet ids `mc_sem.SEM_*`, chunk format) or
  work on the version-neutral model (`mc_chunk.Chunk`, `mc_world.World`).
* Version dependent behaviour is a FLAG or a strategy chosen from the Version, with a test
  per branch. Version dependent tables are GENERATED from `minecraft-data` (`tools/mcgen/`).
* If a type genuinely differs per version (slot, entity metadata, NBT root name, UUID in
  login) the lib takes the version/flag as a parameter -- no hard coded 1.20.1 assumption.
* Exactly one version is a join target now (1.20.1); a second/third version only proves
  the modularity (tables + walker conformance).

## Conventions

* Libraries live in `lib/mc/`. **File names are `mc_<name>.fi`**: Firn names a module by
  the LAST path component, so `lib/mc/text.fi` would clash with `std.text` (measured:
  "module 'text' is imported more than once"). Import as `import mc.mc_chunk`, use as
  `mc_chunk.name`.
* Generated, version-specific code: `lib/mc/v763/` (one directory per protocol version,
  produced by `tools/mcgen/`). Nothing in the engine libraries names a packet id.
* Tests: `tests/NNNN_mc_<lib>*.fi` (first line `// expect_exit: 0`), numbers reserved per
  worker below; cross-checks against a reference: `tools/mc/<lib>/run.sh` (prints
  `RESULT mc-<lib>: ok`). `bash tools/mc/run.sh [lib]` runs everything,
  `bash tools/mc/testmc.sh [pattern]` only the `tests/*_mc_*.fi` in three build stages.
* API doc per library: `docs/mc/<lib>.md` (functions, error cases, measured numbers).
* Every `.fi` file must be in the canonical shape of `firnfmt` (`test.sh` section 23 fails
  otherwise): build it with `firnc -o /tmp/x/firnfmt tools/fmt/firnfmt.fi`, then `firnfmt -w <files>`;
  `bash tools/mc/run.sh` checks it. Generated files are emitted in that shape.
* Parsers get a fuzz test (mutated / truncated / oversized input: 0 crashes, 0 hangs).
* Wrapping arithmetic (hashes, LCGs) uses `*% +% -%`; plain `*` aborts on overflow.
* Platform code only in `platform/` or `*.windows.fi`. Everything here is portable Linux/Windows.
* Merge: `bash tools/mc/merge_main.sh <branch> "<message>"` once the tests are green
  (standing rule: own projects, no asking; serialised by a lock; pushes to origin/main).
* Heavy runs (server runs, big builds): `/root/jarvis/bin/heavy <cmd>` only. Clean up
  (`mktemp -d` + `trap`, remove worktree + branch after the merge).

## The list

Status 2026-10-11 (end of the library round). "Cross-check" = against an INDEPENDENT reference (python / node / java), never against
Mojang code. All of it runs with `bash tools/mc/run.sh` (about 10 minutes).

| Lib | Purpose | Status | Cross-check | Test numbers |
|---|---|---|---|---|
| `std.bytes`, `std.net`, `lib/async`, `std.deflate`, `std.crypto.*`, `std.json`, `std.nbt` | building blocks | existed | - | - |
| `mc_version` + `mc_sem` + `mc_versions` + `vNNN/mc_proto_vNNN.fi` + `tools/mcgen` | Version object (protocol, FLAG_*, chunk format, semantic packet ids), generator `protocol.json` -> tables for **1.8.9, 1.12.2, 1.16.5, 1.20.1, 1.20.2, 1.21.1, 1.21.4, 1.21.8, 1.21.11, 26.1** | **done** | ~9500 reference packets consumed exactly, 10 versions x 4 build stages (nmp up to 1.21.4, protodef + own lpVec3 beyond) | 2650 |
| `mc_schema` | generic packet walker / validator (any packet of any generated version) | **done** (~0.8 M packets/s) | same fixtures + truncation / extension / mutation / random input | 2650 |
| `mc_registry` + `vNNN/mc_reg_vNNN.fi` | blocks, ALL block states with properties, items, entities, biomes per version | **done** | minecraft-data + prismarine-block, 5 versions, 24135 states for 1.20.1 | 2690 |
| `mc_packet` | bounds-checked Reader / Writer for protocol types (sub-worker B) | **done** (1.20.1 / 1.20.2 hooks) | node protodef serializer | 2660-2662 |
| `mc_nbt` | NBT tree, byte-identical write-back, SNBT | **done** | 60 random trees written by prismarine-nbt, both root forms, fuzz | 2741 |
| `mc_text` | chat components, legacy codes, ANSI, translations | **done** | 200 random components vs a python re-implementation | 2742 |
| `mc_codec` | frames, zlib threshold, AES-128-CFB8 both directions | **done** | independent python peer, compression x encryption, hostile frames | - |
| `mc_crypto` | RSA SPKI parse, PKCS#1 v1.5 encrypt, shared secret, CFB8 stream (sub-worker A) | **done** | node crypto (RSA decrypt, aes-128-cfb8) | 2670-2672 |
| `mc_auth` | offline UUID, server hash, session join, MSA review (sub-worker A) | done, **session join only against a mock**, MSA never run against the live service | python mock server, published hash vectors | 2675 |
| `mc_chunk` + `mc_world` | palette sections, heightmaps, light, coordinate math | **done** | independent python decoder / dump; real 1.20.1 server (109 chunks) | - |
| `mc_flat` (+ `v340/mc_flat_v340`) | flattening table 1.12.2 `id<<4\|meta` -> 1.20.1 state ids, container remap | **done** | independent Node implementation (4096 entries), `tests/2746_mc_flat_remap.fi` | other legacy versions (1.8.9) need their own table |
| `mc_dimension` | min_y / height from the Join Game codec | **done** | real 1.20.1 server | 2691 |
| `mc_client` | Handshake -> Login -> (Configuration) -> Play for any version | done for 1.20.1 (real server); configuration phase **only compiled** | real 1.20.1 server | - |
| `mc_net` | connect with retry / back-off, server list ping | **done** | python status server | - |
| `mc_math` | Vec3 / AABB / voxel raycast / mat4 / frustum (sub-worker C) | **done** | numpy, brute force | 2680 |
| `mc_mesh` | CPU greedy meshing + AO (sub-worker C wrote it, lead finished) | **done** | 128 meshes vs python brute force | 2730 |
| `mc_noise` | RNGs, Perlin, simplex, octaves | **done** | real `java.util.Random`, python bit-identical, published vectors | 2700 |
| `mc_encode` | schema driven packet WRITER: value store (`path=value` text) -> body, any packet of any generated version (same bytecode as the walker, SCHEMA_FORMAT 2 incl. holder sets, lpVec3, recursion) | **done** (byte-identical to the reference serializer for every fixture of 10 versions, both directions) | `tools/mc/encode/run.sh`: decode (`mc_schema.walk_values`) -> encode == fixture, reversed store, damaged stores | 2651 |
| `mc_anvil` | .mca region files read / write | **done** | independent python reader / writer, fuzz | 2710 |
| `demos/mcclient` | **final integration**: join a real server, dump + checksum == node-minecraft-protocol | **done** (M0 accepted, see ROUND-M0.md) | node-minecraft-protocol on a real 1.20.1 server | - |

Not built (explicitly): rendering / GPU (blocked by H3), Wasm mod host, physics, inventory, entity decoding, online-mode login against
the live Microsoft / Mojang services (needs a real account), an epoll based `mc_net`, chunk
strategies other than 1.18+ (see MC-VERSIONS.md).

## Work split (sub-workers, separate git worktrees)

| Worker | Libs | Worktree / branch |
|---|---|---|
| G (done) | generator, `mc_schema` (first version), later reworked by the lead into `lib/mc` + `tools/mcgen` | `/root/firn-w-mcclient`, `w-mcclient` |
| T | test harness: real 1.20.1 server + reference client (`tools/mcclient/`) | same |
| A | `mc_crypto`, `mc_auth` | `/root/firn-w-mc-auth`, `w-mc-auth` |
| B | `mc_packet`, `mc_nbt`, `mc_text` | `/root/firn-w-mc-packet`, `w-mc-packet` |
| C | `mc_math`, `mc_mesh` | `/root/firn-w-mc-mesh`, `w-mc-mesh` |
| D (not started: sub-worker cap per run) | `mc_noise`, `mc_anvil` | `/root/firn-w-mc-noise`, `w-mc-noise` (worktree exists) |
| lead | `mc_version`/generator rework, `mc_chunk`, `mc_world`, `mc_codec`, `mc_net`, `mc_registry`, merging, docs, the final client | `/root/firn-w-mcclient` |

## Status log

* 2026-10-11: `mc_chunk` (palette sections) + `mc_world` + first `mc_codec` done; `mc_chunk`
  self-test equals an independent python decoder/dump in three build stages
  (`bash tools/mc/run.sh chunk`). Generator output for v763 exists (worker G).
  Firn finding: `size_of[T]()` of a struct of an imported module is "unknown type"
  (BLOCKER.md); module names must be unique by basename (hence `mc_` prefix).

## Firn pitfalls found while building the first libraries (read before writing code)

* Build: `FIRNLIB=<worktree>/lib /root/firn/compiler/target/release/firnc [--no-opt|--opt-level=dev-fast] -o OUT file.fi`.
  Run the three build stages; they differ (checked arithmetic is on in the safe stages).
* No implicit integer conversions; `as` between integers is a CHECKED value cast. Literals
  need a type context (`0 as u8`, `let x: u64 = 5`). Reinterpret signed/unsigned with
  `bytes.bits_i32 / bits_u64 ...`. Plain `+ - *` abort on overflow; hashes/LCGs use
  `+% -% *%`.
* Pointers are mostly `u64` addresses read/written with `rt.ld8/ld16/ld32/ld64(p, index)` and
  `rt.st*` (index in ELEMENTS of that width); structs are used through `*mut T` and `(*p).field`.
* Error unions have no payload: `fn f() -> BytesError!i32`, `try g()` only inside a function
  with the SAME error set, otherwise `g() catch <fallback>`. Error/enum values are written
  bare, `BytesError::Range`, also for types from imported modules.
* `size_of[T]()` fails for a struct declared in an imported module ("unknown type", see
  BLOCKER.md). Use the stride of a two element array:
  `((&a[1]) as u64 - (&a[0]) as u64)` (see `lib/mc/mc_chunk.fi`, `section_size`).
* Module names are the last path component and must be unique in a program (`std.text` vs
  `mc.text` clash) -> `mc_` prefix for every file in `lib/mc`.
* Text output: `io.fmt_print(f"a={x}\n")` (f-strings build an `io.Fmt`, integers only),
  raw bytes `io.print_bytes(ptr, n)`, files `rt.write_file(cstr, ptr, n)`, `rt.read_file`.
  Append an f-string to an `rt.Buf` with the `put_f` helper in `demos/mcclient/dump.fi`.
* Command line: `fn main(start: u64) -> i32` with `rt.arg_count(start)`, `rt.arg_ptr(start, i)`.
* Style reference for a finished library with tests and an independent cross-check:
  `lib/mc/mc_chunk.fi`, `demos/mcclient/selftest.fi`, `tools/mc/chunk/`.

* 2026-10-11 (later): version-modular rework. `lib/mc/mc_version.fi` (Version object),
  generated `mc_sem.fi`, `mc_versions.fi`, `v47/v340/v754/v763/v764/mc_proto_vNNN.fi`;
  generator moved to `tools/mcgen/` and extended (configuration state, anonymous container
  scope); schema walker moved to `lib/mc/mc_schema.fi`; conformance for 5 versions passes
  (`bash tools/mc/schema/run.sh`). Generator gaps for newer versions recorded in
  `docs/MC-VERSIONS.md` (`registryEntryHolderSet`, `lpVec3`).

* 2026-10-11 (worker B): generator/walker extended (`registryEntryHolderSet`, `lpVec3`, recursive
  types, SCHEMA_FORMAT 2); tables for 1.21.1/1.21.4/1.21.8/1.21.11/26.1; `bash tools/mc/schema/run.sh`
  now compiles the check program once per build stage and runs 10 versions (about 35 s).
