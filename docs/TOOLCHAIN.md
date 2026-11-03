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
1. Finish r68: merge the Certus fork `/root/firnc-gc` (Windows target, integrated assembler, regalloc_a64, xmm-ra) into main; then publish includes everything Certus needs.
2. Certus: add `firn.toolchain` with `firn >=<first merged version>`; build scripts call `firnc` from PATH (shim) instead of `/root/firnc-gc/...`; CI runs `firn upgrade` before building; delete the fork.
3. OrientOS: replace the pin `5e447fc8` by an exact `firn 0.1.N` in `firn.toolchain` (reproducible kernel builds); bump the pin by a PR after its own gates pass on the new version.
4. Server: install once into `/root/.firn` from `/root/firn-publish/channels`, nightly for workers, stable for releases.
