# MC-LIBS -- the libraries a Minecraft client / server needs, in Firn

Status: 2026-10-11. Source of requirements: `GDD-SDD.md` (chapters 4, 6, 8, 14,
appendix A) and `FIRN-GAPS.md` in `/root/projects/minecraft-neu/docs/`.
Decision of the owner (2026-10-11): **libraries first, the client is only the final
integration test.** `demos/mcclient` stays a thin program on top of the libraries.

Clean room: protocol and registry data come from `minecraft-data` (PrismarineJS, MIT),
reference implementations are python / node (`node-minecraft-protocol`, `crypto`) or
independent re-implementations from public format descriptions. No Mojang code or files
are copied, read or decompiled; the official server JAR is only a *test subject*.

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
* Parsers get a fuzz test (mutated / truncated / oversized input: 0 crashes, 0 hangs).
* Wrapping arithmetic (hashes, LCGs) uses `*% +% -%`; plain `*` aborts on overflow.
* Platform code only in `platform/` or `*.windows.fi`. Everything here is portable Linux/Windows.
* Merge: `bash tools/mc/merge_main.sh <branch> "<message>"` once the tests are green
  (standing rule: own projects, no asking; serialised by a lock; pushes to origin/main).
* Heavy runs (server runs, big builds): `/root/jarvis/bin/heavy <cmd>` only. Clean up
  (`mktemp -d` + `trap`, remove worktree + branch after the merge).

## The list

Legend for "Exists today": **yes** = nothing to build, only cross-checks; **part** =
extend; **no** = build.

| Lib | Purpose | Exists today? | Effort | Order | Test numbers |
|---|---|---|---|---|---|
| `std.bytes` (VarInt/VarLong, ZigZag, string, uuid, position, BE ints/floats) | wire primitives | **yes** (`tests/1601`) | - | 0 | - |
| `std.net` + `lib/async` (epoll loop, stream, TLS stream), `lib/net/dns` | sockets, non-blocking, DNS | **yes** | - | 0 | - |
| `std.deflate` (zlib/gzip), `std.crypto.aes` (CFB8 with in-place shift register), `sha1/sha256/md5/hmac/random`, `std.json`, `std.nbt` | building blocks | **yes** | - | 0 | - |
| `mc_packet` | bounds-checked Reader/Writer for protocol types: string(max len), uuid, position, angle, bitset (long array), optional, array-prefixed, rest-buffer, f32/f64, slot (version hook), anonymous-NBT glue; sticky error flag | part (`std.bytes` has the scalars) | M | 1 | 2660-2669 |
| `mc_protocol` + `mc_schema` + `tools/mcgen` | generator `protocol.json` -> packet tables and schema programs; generic schema walker that decodes/validates/dumps ANY packet of a version ("version = data") | **no** (in progress: `demos/mcclient/versions/v763`, `schema.fi`) | L | 1 | 2650-2659 |
| `mc_codec` | frame reader/writer: length VarInt, compression threshold, zlib, AES-CFB8 stream (both directions), packet-size limits, state machine Handshake/Status/Login/Play, stats | part (first version from `demos/mcclient/wire.fi`, no CFB8 yet) | M | 2 | 2690-2699 |
| `mc_crypto` | RSA public key from X.509 SubjectPublicKeyInfo DER, PKCS#1 v1.5 encryption for the login, shared-secret generation, CFB8 wrapper | part (`rsa.fi` verify only, `tls/der.fi`, `x509.fi`, `random.fi`) | M | 2 | 2670-2679 |
| `mc_auth` | offline UUID (md5), server-id digest (SHA-1 as signed hex), session `join` call (HTTPS), profile; MSA/Xbox chain (`lib/auth/msa.fi`) re-checked | part (`md5_offline_uuid`, `msa.fi` exist, never run against the live services) | M | 3 | 2670-2679 |
| `mc_nbt` (extension) | tree/DOM reader over `std.nbt` pull reader, typed getters, long-array helpers, writer from tree, SNBT print (optional) | part (`std.nbt` pull API) | S-M | 3 | 2660-2669 |
| `mc_chunk` | chunk column -> palette sections (block states + biomes), heightmaps, light, block-entity count; normalised dense palette storage | **done** (this branch, cross-checked against an independent python decoder) | M | 2 | 2690-2699 |
| `mc_world` | chunk store by (x,z), coordinate math (floor div), section index, later change lists, Anvil region read/write | part (store + math done; Anvil: no) | M | 3 | 2700-2709 |
| `mc_registry` + `tools/mcgen` | block states (id <-> name + properties, default state, flags), items, entities, biomes, particles as compact generated tables | **no** | L | 3 | 2650-2659 |
| `mc_text` | chat components: JSON (`std.json`) and legacy `§` codes -> styled spans, plain text, ANSI; translate keys with a caller-provided table | **no** | M | 4 | 2660-2669 |
| `mc_math` | Vec3/IVec3/AABB, ray-AABB, voxel raycast (DDA), frustum planes from a matrix, 4x4 matrices | **no** (`lib/math/core_math.fi` is scalar only) | M | 4 | 2680-2689 |
| `mc_mesh` | chunk -> vertex/index buffers on the CPU: culling of hidden faces, greedy meshing, ambient occlusion, no GPU | **no** | L | 5 | 2680-2689 |
| `mc_noise` | deterministic RNGs (splitmix64, xoshiro256**, Java-style 48-bit LCG as documented), Perlin (improved), Simplex, octave / fBm | **no** | M | 5 | 2700-2709 |
| `mc_net` | non-blocking client on `lib/async`: connect, reconnect with backoff, status ping (server list, latency), keep-alive handling | part (async stream exists) | M | 6 | 2710-2719 |
| `demos/mcclient` | **final integration**: Handshake -> Login (offline) -> Play -> chunk dump + checksum == node-minecraft-protocol, against a real 1.20.1 server | in progress (works against the libs above) | S | 7 (last) | - |

Not planned (explicitly): rendering, GPU (blocked by H3), Wasm mod host (M8), physics,
inventory logic -- those are later milestones of the GDD.

## Work split (sub-workers, separate git worktrees)

| Worker | Libs | Worktree / branch |
|---|---|---|
| G | `mc_protocol`, `mc_schema`, `tools/mcgen` (+ `mc_registry` after) | `/root/firn-w-mcclient`, `w-mcclient` |
| T | test harness: real 1.20.1 server + reference client (`tools/mcclient/`) | same |
| A | `mc_crypto`, `mc_auth` | `/root/firn-w-mc-auth`, `w-mc-auth` |
| B | `mc_packet`, `mc_nbt`, `mc_text` | `/root/firn-w-mc-packet`, `w-mc-packet` |
| C | `mc_math`, `mc_mesh` | `/root/firn-w-mc-mesh`, `w-mc-mesh` |
| D | `mc_noise`, Anvil part of `mc_world` | `/root/firn-w-mc-noise`, `w-mc-noise` |
| lead | `mc_chunk`, `mc_world`, `mc_codec`, `mc_net`, merging, docs, the final client | `/root/firn-w-mcclient` |

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
