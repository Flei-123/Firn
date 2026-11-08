# fUi editor (M5 proof app)

One source, `examples/fui_apps/editor/`, builds to an X11 binary and a wasm32-browser page. The native and the WASM first frame are pixel-equal.

## Parts

| File | Role |
|---|---|
| `main.fi` | entry: `editor_app.build`, then `appshot.finish` (`--theme`, `--size`, `--shot`, `--data`) |
| `editor_app.fi` | builds the window: menu, toolbar, find bar, text area, status bar, services poll |
| `editor_menu.fi` | File and Edit menus with shortcuts, toolbar, go-to-line dialog |
| `editor_files.fi` | New / Open / Save / Save as through `fui.services` events, discard confirm, recent file in the store |
| `editor_find.fi` | find next / previous (wraps), replace, replace all, go to line |
| `editor_status.fi` | file name, language, line:col, modified flag |
| `editor_highlight.fi` | `lib/highlight` spans drawn through the `fui.textarea` tint hook (up to 64 KB) |
| `editor_state.fi` | the shared state record |
| `editor_check_main.fi` | model test, no display (memory services backend) |

The text area is `app.textarea_big(a, hint, rows, cap)` (256 KB). All IO goes through `fui.services`; on the web, Open is the browser file picker and Save is a download.

## Build

```sh
export FIRNLIB=$PWD/lib
firnc -o editor examples/fui_apps/editor/main.fi
./editor
firnc --opt-level=release-safe --target=wasm32-browser -o editor.wasm examples/fui_apps/editor/main.fi
```

The sample document is `examples/fui_apps/data/editor_sample.fi` (loaded by name; native: `--data DIR` or `FUI_DATA_DIR`, web: page asset).

## Gates

- `bash tools/wasm/screenapps.sh editor` (use `LIM=0`): native shot vs headless Chromium, 0 differing pixels.
- `tools/fui/run.sh` stage 18p6: model check, `FUI_AUDIT=1`, golden `tools/fui/golden/editor_light.png`.
