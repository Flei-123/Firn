# Firn pin policy

All consumers build with ONE Firn version. The pin is a published toolchain version (`0.1.<rev-count>`), never a loose commit.

## Who pins
| Repo | Pin file(s) | Smoke (tools/pin/smoke/) |
|---|---|---|
| Osum / OrientOS | `firn.toolchain` + `vendor/firn/COMMIT` (= `COMMIT` of the toolchain) | fetch-firnc.sh served by the installed toolchain, `tools/build-kernel.sh` |
| Certus | `firn.toolchain` (exact, `tools/firnc` refuses ranges) | `lib/js/run_main.fi` release-safe build + one JS run |
| fUi apps (firn repo) | `examples/fui_apps/firn.toolchain` | notes + settings release-safe, counter wasm32-browser |
| FirnChat | `firn.toolchain` | `build.sh` through the `firnc` shim |

The registry is `tools/pin/repos.txt` (name, default path, directory of `firn.toolchain`). A new consumer = one line there + one `smoke/<name>.sh <repo> <version>`.

## Which version
1. Stable: the newest version on the `stable` channel. Default for every consumer.
2. Nightly: only when a consumer needs a fix that is not on stable yet; go back to stable at the next stable release.
3. All consumers move together. Never leave two consumers on different versions.

## How to bump
```
bash tools/pin/bump.sh 0.1.N --repo osum=<wt> --repo certus=<wt> --repo fui=<wt> --repo firnchat=<wt> [--commit]
```
- Installs the version if missing (`firn install`), writes the pin files, runs every smoke, commits only with `--commit` and only where the smoke passed. Exit code 1 lists the failing repos.
- Use worktrees (`--repo NAME=PATH`), never dirty main checkouts. Push each repo from its worktree after the smoke and the repo's own gate.
- `--only osum,certus` limits the run, `--no-smoke` only writes the files.
- If `<repo>/docs/firn-pin.md` exists, its line `Firn toolchain pin VER (COMMIT)` is rewritten and committed with the pin; a missing file is skipped. Test: `bash tools/pin/test.sh` (includes a mutant that must be caught).

## Stable channel
- `tools/release/gate.sh --out STAMP [--keep-logs DIR]` runs all stages of `tools/teststages/stages.txt` plus the fixpoint. Items listed in `tools/release/known_red.txt` do not fail the gate; every line there must have a roadmap item.
- `tools/release/publish.sh --channel stable --gate-stamp STAMP` publishes only a green stamp for HEAD.
- Then `firn upgrade --channel stable`, `firn doctor`, and `tools/pin/bump.sh <stable version> --commit` for all consumers.
