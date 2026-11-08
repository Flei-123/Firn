# fui build -- one source, native + web (+ android)

`fui` (tools/fui/cli/fui) creates a fUi app and builds it for every target from the SAME source.

## Quickstart

One-time setup: `fui` on the PATH and a compiler. `FIRN` is your Firn checkout.

```
export PATH="$FIRN/tools/fui/cli:$PATH"
export FIRNC="$FIRN/compiler/target/release/firnc"
```

From an empty folder:

```
fui new hello
cd hello
fui build --targets native,web --verify
```

Result (`hello/dist/`):

- `native/hello` -- X11 executable (+ `native/data/`)
- `web/index.html` -- serve with `python3 -m http.server -d dist/web 8000`
- `verify/` -- native shots (light, dark) and the Chromium shot; `--verify` fails with exit 3 when more than the limit of pixels differ

Run it: `./dist/native/hello`.

## Project layout

```
hello/
  firn.package   Firn manifest: package, version, start, source
  fui.app        fUi manifest (below)
  main.fi        the app
  data/          assets, read by NAME only (kitdata.load("greeting.json"))
```

`fui.app` keys (all optional, `key value` per line, `#` starts a comment line):

| key | default | meaning |
|---|---|---|
| `title` | package name | page title, Android label |
| `size` | `1100x700` | window size for the verify shots |
| `data` | `data` | assets directory, relative to the project |
| `targets` | `native web` | targets built without `--targets` |
| `limit` | `0.5` | allowed differing pixels in percent for `--verify` |
| `opt` | `release-safe` | firnc optimisation level |
| `android_id` | `com.fleitec.fui.<package>` | Android application id |

Data is found by name: native via `$FUI_DATA_DIR`, then `<exe>/data`; web and Android as page/APK assets. Never open paths from app code.

## Commands

```
fui new <name> [--dir DIR]
fui build [DIR] [--targets native,web,android] [--out DIR] [--verify] [--limit PCT] [--opt LEVEL] [--abi ABI]
fui verify [DIR] [--out DIR] [--limit PCT]
fui targets
```

- `--out` defaults to `<DIR>/dist`; layout `native/`, `web/`, `android/`, `verify/`.
- `android` needs the NDK and SDK build-tools (`NDK`, `SDK`, see tools/android/build.sh); output `android/<package>-<abi>.apk`, default ABI x86_64, `--abi arm64` for a phone.
- `fui targets` lists the registered targets (one file per target in `tools/fui/cli/targets/`) and whether they work on this machine.

## Exit codes

| code | meaning |
|---|---|
| 0 | ok |
| 1 | build failed |
| 2 | usage error |
| 3 | pixel gate failed |
| 4 | tool or target unavailable (firnc, chromium, Android SDK) |
| 5 | manifest error |

## Under the hood

- native: `firnc --package DIR`; web: `tools/wasm/weblib.sh` (shared with `tools/wasm/screenapps.sh`), `wasm32-browser`; android: `tools/android/screenapp.sh`.
- Pixel compare: `tools/wasm/screencmp.py` (the better of the light and dark native shot counts).
- Gate: `bash tools/fui/fuibuild.sh` (stage 18p8 of `tools/fui/run.sh`).
- The apps `editor`, `notes`, `files` in `examples/fui_apps/` carry the same two manifests: `fui build examples/fui_apps/editor --targets native,web --verify --out /tmp/x`.
