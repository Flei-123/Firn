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

`bash tools/cache/run.sh`: hit and miss, byte-identity against `--no-cache` (executables, x86-64 and aarch64 assembler, kernel objects), import change, flag change, environment switch, damaged entry, size cap, eight parallel builds, unwritable cache directory, assembled object (kernel, aarch64, static, `--asm-intern`, other assembler version, damaged object, parallel builds, cap 0). firnc1 has its own cache (`lib/firnc1/cache.fi`, `cachekey.fi`, `cachefs.fi`, `cacheprune.fi`, proof `bash tools/cache/run_c1.sh`, 44 checks). Same switches, same key recipe, same file format and directory; the compiler binary is part of the key, so firnc0 and firnc1 never share entries. Differences: `<output>.s` is always written (also on a hit, firnc1 leaves it next to the output); the object identity is the SHA-256 of `/usr/bin/as` (plus `/usr/bin/ld` for split builds) and `--64`; the entry also records the split count, and a split build without a stored object is a miss; `--timings` and `--env-log` switch the cache off; the entry checksum is the same 64-bit sum as firnc0 (wrapping multiply). firnc0 now hashes keys with `keyhash.rs` instead of SHA-256 (firnc1 still SHA-256; the two never share entries).

`bash tools/cache/run_wasm.sh`: 48 checks for the wasm cache (hit/miss, byte identity against `--no-cache`, run under node, flag/opt-level/target/env changes, other compiler binary, include file and `__env_or` changes, damaged entry, failed build, parallel builds, cap 0, resolve manifest incl. shadowing file).

(firnc0 on kmain.fi: cold 13.8 s, text hit 4.2 s, object hit 0.6 s on a busy host).
