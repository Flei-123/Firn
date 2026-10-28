# Compile cache (firnc0 and firnc1)

`firnc` (firnc0) also caches `--target=wasm32-browser` (see "WebAssembly" below). `firnc` stores the generated assembler text of a successful build and, after the assembler ran, the assembled object. It reuses them when nothing that could change the output has changed. On an object hit the front end, the optimizer, the code generator and the assembler are skipped; only the linker runs (kernel objects and `-c` builds: nothing runs at all). If the object is missing the assembler text is used and the object is stored afterwards.

## Switches

| What | Effect |
|---|---|
| `--no-cache` | neither read nor write the cache |
| `FIRN_NO_CACHE=1` | same, from the environment |
| `FIRN_CACHE_DIR=<dir>` | cache location (default `~/.cache/firn`) |
| `FIRN_CACHE_MAX_MB=<n>` | size cap, default 512; the oldest entries (by last use) go first, down to 80 % of the cap; 0 keeps nothing |
| `FIRN_CACHE_LOG=1` | lines on stderr: `cache: miss`, `cache: store`, `cache: hit`, `cache: skip`, `cache: off`, plus `cache: objload` (object served) and `cache: objsave` (object stored) |

## Key

A 128-bit non-cryptographic hash (`keyhash.rs`, two chained 128-bit-multiply lanes, about 3 GB/s) over: the bytes of the running compiler binary, the working directory, the command line (without `-o` and `--no-cache`), every `FIRN*` environment variable (except the cache switches above), path and content of every source file the module resolver read (root plus all transitive imports), and the manifests of a `--package` build.

## What is stored

The assembler text plus the link state that the front end would otherwise provide: kernel profile, presence of a `static`, `#[link_lib]` names, `#[export_c]` names. Entries are written to a temporary file and renamed into place, so concurrent builds see either a whole entry or none. Every entry carries its length and SHA-256; a damaged entry is a miss and is overwritten.

## Object entry

File `objects/<key>.<id>.o` next to the text entry `<key>.fc`; `<id>` = SHA-256 (16 hex digits) of the assembler identity: first line of `as --version` (`aarch64-linux-gnu-as`, `x86_64-w64-mingw32-as` per target) plus its flags, or `intern` for `--asm-intern`. A different assembler version therefore gets its own object next to the old one. The object is written only when the text entry exists, atomically, with length and checksum; a damaged object is a miss for the object only (the text entry is used and the object is rewritten). `--keep-asm` builds never load the object (they need the `.s`). The `.s` name does not enter the object (checked: identical objects under different `-o`). Text entry and object count toward the size cap and are pruned oldest first.

## Not cached

- `--emit` other than `exe` and `asm`, `--stats`, `--timings`, `--env-log`, `--header`, `--def`, `--deps-out`, `--target=wasm32-browser` with `--emit=asm` or `--test`/`-c`
- a build that read a build time environment variable (`__env_or`, `__env_has`), used `__include_str`, ran a `comptime` block or a comptime file read (hidden inputs the key cannot see): logged as `cache: skip`
- failed builds

## Resolve manifest (hit without reading the sources)

The key needs the content of every module, and finding the modules means lexing them for their imports (60 % of a hit on a big program). So a second small entry `objects/<rkey>.rm` maps the build identity to the full key. `rkey` = hash of compiler binary, working directory, command line (without `-o`), `FIRN*` environment, package manifests. The entry lists everything module resolution touched: every source file read (path + content hash) and every path probed with its answer (exists / does not exist, `is_file` for the platform twins and the sysroot marker). On the next build the listed files are re-hashed and the probes repeated; if all still hold, resolution would give the same file list, so the stored full key is used and the front end is skipped. A new file that shadows an import changes a probe answer and is therefore a miss. A damaged `.rm` is a miss. Written after every successful store and after every full-key hit. Off with `--lock` / `--locked` (the lock check needs the file list). Applies to every target, not only WebAssembly.

## WebAssembly

`--target=wasm32-browser` (`--emit=exe`, the `.wasm`) is cached as `objects/<key>.wasm`: a header (`FIRNWASM2`, hidden inputs, length, checksum) and the module bytes. A hit writes the stored bytes to the `-o` path; the output is byte-identical to a cold build. Hidden build inputs do not disable the cache here, they are recorded and re-verified on every hit: each `__include_str` file (content hash) and each `__env_or` / `__env_has` read (name and value or presence). If any changed, the entry is a miss. `comptime` blocks and comptime file reads still skip the cache (`cache: skip`). The native targets keep treating `__include_str` and `__env_*` as a skip.

Measured (release firnc, `--opt-level=release-safe`, busy host, `bash tools/cache/measure_wasm.sh <cwd> <src>`): FirnFont web 6.9 s cold, 0.05 s hit (12 MB RSS instead of 965 MB); fUi gallery 3.6 s / 0.02 s; MC web scene 1.2 s / 0.01 s. A change of one file is a full rebuild (no per-module cache yet).

## Proof

`bash tools/cache/run.sh`: hit and miss, byte-identity against `--no-cache` (executables, x86-64 and aarch64 assembler, kernel objects), import change, flag change, environment switch, damaged entry, size cap, eight parallel builds, unwritable cache directory, assembled object (kernel, aarch64, static, `--asm-intern`, other assembler version, damaged object, parallel builds, cap 0). firnc1 has its own cache (`lib/firnc1/cache.fi`, `cachekey.fi`, `cachefs.fi`, `cacheprune.fi`, proof `bash tools/cache/run_c1.sh`, 44 checks). Same switches, same key recipe, same file format and directory; the compiler binary is part of the key, so firnc0 and firnc1 never share entries. Differences: `<output>.s` is always written (also on a hit, firnc1 leaves it next to the output); the object identity is the SHA-256 of `/usr/bin/as` (plus `/usr/bin/ld` for split builds) and `--64`; the entry also records the split count, and a split build without a stored object is a miss; `--timings` and `--env-log` switch the cache off; the entry checksum is the same 64-bit sum as firnc0 (wrapping multiply). firnc1 has the resolve manifest too (`lib/firnc1/resolvemanifest.fi`, entry `objects/<identity>.rm`, identity = SHA-256 of compiler binary, working directory, command line and `FIRN*` environment): it lists the full key, the kernel flag, the `<exe>/../lib` search directory, every module file (size and two 64-bit sums) and every `access()` probe of the module search with its answer; a hit re-hashes the files and repeats the probes and skips lexing entirely (kmain hit: 0.60 s, 17k page faults, 185 MB -> 0.31 s, 2.7k faults, 133 MB; the rest is reading and writing the 73 MB assembler text). It is not used with `--package`, a found project manifest, `--lock`/`--locked` or stdin input. firnc0 now hashes keys with `keyhash.rs` instead of SHA-256 (firnc1 still SHA-256; the two never share entries).

`bash tools/cache/run_wasm.sh`: 48 checks for the wasm cache (hit/miss, byte identity against `--no-cache`, run under node, flag/opt-level/target/env changes, other compiler binary, include file and `__env_or` changes, damaged entry, failed build, parallel builds, cap 0, resolve manifest incl. shadowing file).

(firnc0 on kmain.fi: cold 13.8 s, text hit 4.2 s, object hit 0.6 s on a busy host).

## Function cache (design, not implemented yet)

Today one changed file is a full rebuild. Measured baseline (OrientOS pin 5e447fc8, `kernel/kmain.fi`, firnc0 `--opt-level=release-safe --no-cache --timings`, host load about 28, object sha db5c81e34e2a): total 51.3 s = optimizer 35.7 s (69.7 %), `as` 9.0 s (17.6 %), codegen 3.3 s (6.4 %), sema 1.7 s, lower 0.6 s, lex+parse 0.6 s, mono 0.3 s. `FIRN_STAGE_TIMINGS=1` splits the optimizer: first batch 3.75 s, inline 1.09 s, batch after inline 25.9 s (5800 of 7918 functions re-optimized, 5545 inline-changed), inline again + batch 4.3 s, promote 0.3 s, late passes 0.4 s. So the optimizer comes first, codegen second (in Certus browser codegen is the larger share), `as` stays whole-program until the direct-object path lands.

### Layer 1: memo of `opt::optimize_func`

`optimize_func` is the single entry of every batch (`optimize_batch`, `optimize_map`, the promote closure; parfn workers call it too). `parfn.rs` documents it as a pure per-function transformation. Its inputs: the `Func`, `OptConfig`, `target::State`, the immutable-static size map (`statics::rodata_sizes`, read by `licm`), `rangecheck::LEN_GETTERS`, `gtaken::TAKEN` (read by `loadcse`), `ivsr::TOUCHED.contains(name)`, `FIRN_*` environment. Its outputs besides the mutated `Func`: the `OptStats` delta, the `UNSETTLED` push (`opt.rs`) and the `ivsr::TOUCHED` insert; both flags must be stored and replayed.

- Key: `keyhash` 128 bit over context digest (compiler binary, command line without `-o`, `FIRN*` environment, `OptConfig`, target state, sorted digests of LEN_GETTERS, gtaken set and rodata sizes) + structural hash of the input `Func` + the TOUCHED bit.
- Structural hash: `#[derive(Hash)]` on the FIR types (`Op` holds no float, only bit patterns; `secret` and `no_coalesce` are hashed sorted), fed into a `Hasher` built on `keyhash.rs`. A field added later is hashed automatically, so the key cannot go stale.
- Storage: `fir_codec.rs` with `encode(&Func)` / `decode(&[u8])` (no crates available; exhaustive `match` over `Op` and `Term` without wildcard, so a new variant fails to compile until handled). Entry `fn/o/<key>.fo`: magic, length, 64-bit checksum, flags, stats, payload; temp file + rename; pruned with the same size cap as `objects/`.
- Why this is exact without a dependency graph: when callee G changes, only the callers that inlined G get a different post-inline `Func`, so only their keys miss; every other function hits, in the first batch and in the batches after inlining. The module level phases (inline, inline_once, guardsplit, promote summary) stay uncached; they cost 1-2 s.
- Later refinement: restrict the gtaken / rodata / len-getter digests to the names a function references, so an unrelated global change does not invalidate everything.

### Layer 2: memo of the code generator per function

Hook: `codegen_x86::emit`, the loop around `emit_func` after `chosen` (recursive-caller frame budget) is selected; `codegen_a64::emit` the same way. Key = context digest + structural hash of the chosen `Func` after `phi::eliminate`. Value = the function text (hot and cold half) + a journal of the global effects of the emission:

- `fpool::intern` (labels `.Lfconst<N>` are global pool indices) and `panic_rt` message labels (`.Lpanicmsg<N>`): store placeholders for the journal ordinal, replay the interns in call order on a hit, substitute the new labels; `panic_rt::note_index_site` is replayed.
- Reads that must be re-validated on a hit: `label(name)` of every callee (`extfn` extern / export names), `statics::label_of`, `iface::table_label`, `fnval::record_label`, `gc::runtime_active`, `stackguard::enabled`, `dwarf::fn_line`.
- Anything else that touches a global during emission marks the function uncacheable (thread-local taint). Not cached: `dwarf::with_variables()`, Windows (`win::note`).
- `.loc` carries absolute line numbers (they are part of the hash), so inserting a line shifts all functions below it in that file; storing lines relative to `dwarf::fn_line` would avoid that.

### Switches and proof

`--no-cache`, `FIRN_NO_CACHE`, `FIRN_CACHE_DIR`, `FIRN_CACHE_MAX_MB` stay valid. New: `FIRN_FNCACHE=0` switches both layers off, `FIRN_FNCACHE_VERIFY=1` recomputes on every hit and aborts on any difference (run it over the whole `tests/` corpus after priming the cache with a perturbed variant), `FIRN_CACHE_LOG=1` prints `cache: fn opt hit=<n> miss=<m> store=<k>` and the same for `cg`. Proof script `tools/cache/run_fn.sh`: cold, one-line change, warm; bytes identical to `--no-cache` (exe, asm, object); counters.
