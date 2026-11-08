# M7b plan: `fui build` / `fui new`

Goal: one codebase, one command, native + web (+ android) output, proven by a pixel gate.

## Today

| Target | Script | Notes |
|---|---|---|
| native | `firnc -o X examples/fui_apps/<app>/main.fi`, run `X --theme T --size WxH --data DIR --shot P` | no wrapper script; each gate builds by hand |
| web | `tools/wasm/screenapps.sh <app>...` | builds wasm32-browser (`FIRN_PRUNE_FIR=1`), writes page, serves, headless Chromium shot, compares with native shot via `tools/wasm/screencmp.py` |
| android | `tools/android/screenapp.sh <app> [--abi]` | fonts + `examples/fui_apps/data` as APK assets, calls `tools/android/build.sh` |

Problems: app path and data dir are hard-wired to `examples/fui_apps`; page generation, static server and Chromium shot live inside one script; no manifest for apps.

## Manifest

- `firn.package` (existing Firn format, read with `firnc --package-info`): `package`, `version`, `start`, `source`. Gives name, version, entry file.
- `fui.app` (new, same `key value` style, only fUi keys):
  - `title <text>` window/page title (default: package name)
  - `size WxH` shot size (default 1100x700)
  - `data <dir>` assets directory, by NAME only (default `data`)
  - `targets native web [android]` default targets
  - `android_id <reverse-dns>` (default `com.fleitec.fui.<package>`)
  - `opt <level>` default `release-safe`

## CLI

```
fui new <name> [--dir D]
fui build [dir] [--targets native,web,android] [--out dist] [--verify] [--limit PCT] [--opt LEVEL]
fui targets
```

- Thin dispatcher `tools/fui/cli/fui`; one module per verb in `tools/fui/cli/verbs/`, one per target in `tools/fui/cli/targets/` (`native.sh`, `web.sh`, `android.sh`). A target is any file there that defines `target_build` and `target_available`; `fui targets` lists the directory (registry = directory).
- Shared helpers: `tools/fui/cli/lib/{manifest,log,env}.sh`.
- Web building blocks move out of `tools/wasm/screenapps.sh` into `tools/wasm/weblib.sh` (site build, static server, headless shot). `screenapps.sh` sources it; behaviour unchanged.
- Android target calls `tools/android/screenapp.sh` with `APP_MAIN` and `ASSETS_DIR` overrides.
- Verify (`--verify`, needs native + web): native `--shot` light+dark vs web shot via `tools/wasm/screencmp.py`, limit 0.5 % (as `screenapps.sh`).

## Output layout

```
dist/<package>/native/<package>            executable
dist/<package>/native/data/...             assets
dist/<package>/web/index.html + <package>.wasm + firn.js + fonts + data
dist/<package>/android/<package>-<abi>.apk
dist/<package>/verify/{web.png,native-*.png}   only with --verify
```

## Exit codes

| code | meaning |
|---|---|
| 0 | ok |
| 1 | build failed |
| 2 | usage error |
| 3 | pixel gate failed |
| 4 | target unavailable (no NDK / Chromium / device) |
| 5 | manifest error |

## Gate

`tools/fui/fuibuild.sh` (registered in `tools/fui/run.sh`): `fui new` in a `mktemp -d`, build + verify native,web; `fui build --verify` for editor, notes, files from `examples/fui_apps`; checks exit codes and layout. Heavy builds only through `/root/jarvis/bin/heavy`.
