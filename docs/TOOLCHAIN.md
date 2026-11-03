# Firn toolchain: install once, `firn upgrade` forever

## Layout
```
~/.firn  (Windows: %LOCALAPPDATA%\firn, FIRN_HOME overrides)
  versions/<X.Y.Z>/{bin/firnc, bin/firn, lib/, VERSION, PLATFORM, COMMIT, .ready}
  current -> versions/<X.Y.Z>     (Windows: current.txt)
  bin/firn, bin/firnc             shims (symlinks to current/bin/firn; argv[0] "firnc" = proxy)
  config                          url=, channel=, pin=, floor=, previous=
  projects                        firn.toolchain files seen by the shims
```
`firnc` finds `lib/` next to itself (`<exe>/../lib`), so every version carries its own library.
The object cache key already hashes the compiler binary (`compiler/src/buildcache.rs` `base_hasher`, field `compiler`), so an upgrade never reuses stale objects.

## Commands (`tools/toolchain/firn_main.fi`, library `lib/toolchain/`)
| command | does |
|---|---|
| `firn install [VER] [--channel C] [--force]` | download, verify, selftest, install; switches when it is the first or the latest |
| `firn upgrade` | newest of the channel; no-op when pinned; lists projects whose pin is older |
| `firn upgrade --pin VER` / `--unpin` | hold / release a version |
| `firn list`, `firn use VER`, `firn version`, `firn doctor` | inspect / switch / check (PATH, shims, selftest) |
| `firn channel URL [NAME]` | channel address (`file://`, `http://`, `https://`) |
| `firn build ARGS`, `firn run FILE ARGS` | pass-through to the chosen `firnc` |

Toolchain choice: `FIRN_TOOLCHAIN=VER` > nearest `firn.toolchain` upwards from the cwd (`firn 0.1.5123` exact, `firn >=0.1.5000` range, highest installed match, current preferred) > `current`.
`firn.toolchain` is a separate file (like `rust-toolchain`), not a `firn.package` key: both compilers' manifest parsers stay untouched and old compilers keep reading new manifests.

## Upgrade, atomic
manifest -> size + SHA-256 of the archive -> unpack into `versions/.stage-VER` -> selftest (compile + run hello with the staged firnc, FIRNLIB unset) -> rename to `versions/VER` + `.ready` -> `current` switched by symlink+rename -> selftest through `current`, on failure switch back. Any failure removes the stage; `current` is untouched.
Downgrade protection: every manifest has `min_version`; the client stores the highest as `floor` and refuses any release below it (`--force` overrides). Signature: per-platform `signature` field reserved (Ed25519, `lib/std/crypto` + appkit catalog scheme), empty today.

## Release channel (`tools/release/`)
- `publish.sh --channel stable|nightly [--version X.Y.Z] [--min-version] [--firnc BIN] [--firnc-for PLATFORM=BIN] [--gate-stamp F]`: version default `0.1.<git rev-list --count HEAD>`; lib from `git archive HEAD`; archive per platform (`.tar.gz`, Windows `.zip`), `manifest-<ver>.json` + `manifest.json` (written last, atomically). Refuses versions not newer than the channel.
- `stable` needs a gate stamp (`commit=<HEAD>`, `result=green`): test.sh stages against the known-red list + fixpoint. `nightly` does not.
- Output default `/root/firn-publish/channels/<channel>/` (serving it over HTTPS is open).
- `install.sh URL [channel]`: POSIX bootstrap (curl or file), checks SHA-256, then lets `firn` install itself and writes the shims.
- Platforms: linux-x86_64 built here; linux-aarch64 / windows-x86_64 only via `--firnc-for` (no cross-built firnc0 yet).

## Tests
`bash tools/toolchain/run.sh` (temp dirs, trap cleanup): two+ fake versions, bootstrap over `file://` and `http://127.0.0.1`, shims, run/build, upgrade, list, project pins and ranges, stale-pin report, wrong hash, broken compiler (selftest), pin/unpin, use, floor/replayed manifest. Windows under Wine: not yet (no Windows firnc archive, `fs` has no Windows twin).

## Plan: Certus and OrientOS on `firn upgrade` (not switched yet)
Inventory 03.11.2026:
| project | compiler choice today | pin |
|---|---|---|
| Certus (`/root/certus-ui`, active repo) | 48 scripts `FIRNC=${FIRNC:-/root/firnc-gc/compiler/target/release/firnc}`, 2 hard-coded, `build-c073.sh` | none; fork binary |
| Certus (`/root/certus`, old split repo) | `vendor/firn/COMMIT` 52a13e75 + `hole-firn.sh`; scripts use `compiler/target/release/firnc` | stale |
| OrientOS (`osum`) | single point `vendor/firn/fetch-firnc.sh` builds `vendor/firn/bin/{firnc0,firnc1}` + `lib` from `vendor/firn/COMMIT` (b1e7f22c, 80 commits behind main 03.11.); check `tools/firnpin/check.sh [REF]` | commit |
| OrientOS Certus app | `tools/certus/ensure.sh`, `kernel/user/certus/build.sh`: `CERTUS_FIRNC` default fork | fork |

Fork `/root/firnc-gc` vs main: no source file only in the fork; Windows target, integrated assembler, `regalloc_a64`, inliner, ownership are on main
(docs/CERTUS-COMPILER-MERGE.md). Fork-only: GC runtime rounds after the freeze (card verifier `gccards.fi` + `--gc-cards`, perm realm
`gc_perm_seal` (no Certus caller), `block_of` fast/slow split, 2 MiB chunk arena with `MADV_HUGEPAGE`, step hook, `gcpar.fi`), Windows seam
0024 (poll over 256 sockets, F_GETFL) and 0025 (thread CPU clock); German census names (`tools/certus/gc_api_rename.sh`).

Steps:
1. Port the fork-only GC rounds and seams to main (r68 rest), then the gate of docs/CERTUS-COMPILER-MERGE.md section 5.
2. Certus: `firn.toolchain` with `firn >=<first merged version>`; scripts take `FIRNC` > `firnc` from PATH (shim) > fork as explicit fallback.
3. OrientOS: bump `vendor/firn/COMMIT` after `tools/firnpin/check.sh origin/main` and its gates; then `fetch-firnc.sh` reuses an installed toolchain
   whose `COMMIT` equals the pin (firnc1 is still built from `bin/firnc1.fi` of that commit).
4. Server: installed in `/root/.firn` from `/root/firn-publish/channels` (nightly 0.1.4808 on 03.11.), PATH via `/etc/profile.d/firn.sh`; stable needs a
   green `tools/release/gate.sh` stamp.
