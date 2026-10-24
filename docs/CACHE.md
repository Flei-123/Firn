# Compile cache (firnc0)

`firnc` stores the generated assembler text of a successful build and reuses it when nothing that could change the output has changed. The assembler and linker still run on a hit; the front end, the optimizer and the code generator are skipped.

## Switches

| What | Effect |
|---|---|
| `--no-cache` | neither read nor write the cache |
| `FIRN_NO_CACHE=1` | same, from the environment |
| `FIRN_CACHE_DIR=<dir>` | cache location (default `~/.cache/firn`) |
| `FIRN_CACHE_MAX_MB=<n>` | size cap, default 512; the oldest entries (by last use) go first, down to 80 % of the cap; 0 keeps nothing |
| `FIRN_CACHE_LOG=1` | one line per build on stderr: `cache: miss`, `cache: store`, `cache: hit`, `cache: skip`, `cache: off` |

## Key

SHA-256 over: the bytes of the running compiler binary, the working directory, the command line (without `-o` and `--no-cache`), every `FIRN*` environment variable (except the cache switches above), path and content of every source file the module resolver read (root plus all transitive imports), and the manifests of a `--package` build.

## What is stored

The assembler text plus the link state that the front end would otherwise provide: kernel profile, presence of a `static`, `#[link_lib]` names, `#[export_c]` names. Entries are written to a temporary file and renamed into place, so concurrent builds see either a whole entry or none. Every entry carries its length and SHA-256; a damaged entry is a miss and is overwritten.

## Not cached

- `--emit` other than `exe` and `asm`, `--stats`, `--timings`, `--env-log`, `--header`, `--def`, `--deps-out`, `--target=wasm32-browser`
- a build that read a build time environment variable (`__env_or`, `__env_has`), used `__include_str`, ran a `comptime` block or a comptime file read (hidden inputs the key cannot see): logged as `cache: skip`
- failed builds

## Proof

`bash tools/cache/run.sh`: hit and miss, byte-identity against `--no-cache` (executables, x86-64 and aarch64 assembler, kernel objects), import change, flag change, environment switch, damaged entry, size cap, eight parallel builds, unwritable cache directory. firnc1 has no cache yet.
