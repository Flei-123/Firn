# fUi screen apps on Android and in the browser

The apps in `examples/fui_apps/` (`lib/fui/screen.fi`) are ONE source for X11, Android and WebAssembly.

| Need | Linux/Windows | Android | Browser |
|---|---|---|---|
| Inter (4 weights) | Linux: `lib/fui/fonts` via `plat.uifont`; Windows: embedded in the exe (`lib/@windows/plat/fontbundle.fi`), a `fonts/` folder next to the exe or `$FUI_FONT_DIR` still wins | APK `assets/` via `lib/@android/plat/uifont.fi` | `data-fonts` slots 0-3 (`lib/@web/plat/uifont.fi`) |
| Monospace | DejaVu Sans Mono (`lib/fui/fonts`, embedded on Windows) | `/system/fonts/DroidSansMono.ttf` | `data-fonts` slot 4 |
| Data and pictures | `plat.assetio` over `fui.datadir` (`$FUI_DATA_DIR`, `<exe>/data`, `examples/fui_apps/data`) | AssetManager | `data-assets` (`firn_web_asset`, delivered before `main`) |
| Headless shot | `--shot` | works | refused (`lib/@web/fui/safeout.fi`) |

Apps load data by NAME: `kitdata.load("profiles.json")`, `kitfx.pic_load("logo-green.webp")`, `mdedit.load(m, i, "notes.md")`.

## Build and prove

```sh
FIRNC=compiler/target/release/firnc bash tools/wasm/screenapps.sh flei settings notes   # wasm + Chromium + pixel compare with the native shot
bash tools/android/screenapp.sh flei [--abi arm64]                                      # APK in /root/jarvis-android-builds/fui-screen/
/root/jarvis/bin/heavy bash tools/android/screensmoke.sh <apk> com.fleitec.fui.flei <outdir>   # emulator start, screenshot, crash check
```

The browser follows the page's colour scheme (`prefers-color-scheme`): `screen.fi` re-applies the skin once on the first paint.
The layouts are desktop layouts (1100x700): on a phone they overlap, a responsive layout is a task of its own.
