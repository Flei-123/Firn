# fUi services adapter and the single Firn pin

## fui.services (lib/fui/services.fi)

One module for apps: files, clipboard, dialogs, key-value storage. Apps import only `fui.services`.

| Call | Meaning |
|---|---|
| `open_request(title, filters)` / `save_request(title, name, p, n)` | start a file dialog; the answer is an event |
| `next(ev)` / `pending()` / `event_new` / `event_free` | pop answers: `EV_OPENED` (name, data), `EV_SAVED` (name), `EV_CLIP` (data); `status` 1 = ok, 0 = cancelled or failed |
| `clip_copy(text)` / `clip_request()` | copy now; paste answers as `EV_CLIP` (native: at once, web: after the browser answers) |
| `message(kind, title, text, buttons)` / `confirm(title, text)` | modal box; `message` returns `ANS_*`, 0 = no box possible |
| `read_path(path, out)` / `write_path(path, p, n)` | direct paths, native only (web: false) |
| `store_get(key, out)` / `store_set(key, p, n)` | small key-value storage |
| `use_memory(on)` / `script_pick(name)` / `script_answer(a)` | deterministic memory backend for tests |

The event queue holds 8 answers; further answers are refused. Native and web run the same app code: request, then poll `next` once per frame.

## Layout

| File | Role |
|---|---|
| `lib/fui/services.fi` | facade, memory switch, event record |
| `lib/fui/svcq.fi` | answer queue |
| `lib/fui/svcmem.fi` | memory backend (tests) |
| `lib/fui/svcsys.fi` | clipboard copy and message boxes through `std.clipboard` / `std.dialog` (both have web twins) |
| `lib/fui/svcfile.fi`, `lib/@web/fui/svcfile.fi` | raw path read/write; the only place that calls `rt.read_file` / `safeout` (web twin: false) |
| `lib/fui/svcplat.fi`, `lib/@web/fui/svcplat.fi` | file dialogs, paste request, storage; native: `std.dialog`, files under `$FUI_STORE_DIR` or `~/.local/share/fui`; web: `plat.webdialog`, `plat.webdownload`, `plat.host` store |
| `lib/@web/fui/svctap.fi` | web event tap: `apphost` forwards `WE_DIALOG` / `WE_CLIP` to the adapter; registered on first request, so apps that never ask import no new host function |

Gate: `tools/fui/run.sh` fails on any `rt.read_file` / `rt.write_file` in `lib/fui` or `examples/fui_apps` outside `svcfile.fi` / `safeout.fi`. Test: `tools/fui/services_main.fi`.

## Rebinding to Firn lib/services

Only `svcplat.fi` / `svcsys.fi` / `svcfile.fi` (and their web twins) call the platform. When Firn `lib/services` v1 lands on main, replace their bodies with its calls; `services.fi` and all apps stay unchanged. Status when written: `lib/services` does not exist yet.

## Single Firn pin (M1, checked 08.11.2026)

Policy and bump script exist on main: `docs/PIN-POLICY.md`, `tools/pin/bump.sh`, registry `tools/pin/repos.txt` (osum, certus, fui, firnchat). fUi's own pin is `examples/fui_apps/firn.toolchain` (`firn 0.1.4898`).

| Consumer | How it gets Firn before the first bump | Gap for fUi |
|---|---|---|
| fUi | lives in the Firn repo (`lib/fui`) | source of truth |
| Osum | `firn.toolchain` 0.1.4894, `vendor/firn/COMMIT` 33b67b5e7 | none, patches all upstream |
| Certus (certus-ui) | `firn.toolchain` 0.1.4893, `vendor/firn/COMMIT` 7893ea2c... (not in current Firn history), own partial fUi copy `.fui-win` (17 of 249 `lib/fui` files) for the Windows build | `.fui-win` must go: every Certus-only fUi patch must be upstream in `lib/fui`, then Windows builds use the shared `$FIRNLIB` (r63) |
| /root/certus (old checkout) | `COMMIT` 52a13e75... (old private history) | do not use |
| FirnChat | no pin file; `build.sh` takes `$FIRNC`, a firnc in PATH, or a Firn checkout | needs `firn.toolchain` (registry already lists it) |

What fUi still needs for one shared pin (nightly 0.1.4889+; newest installed 0.1.4898):
1. Remove `.fui-win` in Certus (above), so the pin really covers fUi code.
2. A smoke in `tools/pin/smoke/` that builds one app using `fui.services` for web and native (notes), so a bump catches adapter breakage.
3. Keep the services adapter the only platform entry of apps, so a bump of `lib/services` changes one file set, not the apps.
4. FirnChat `firn.toolchain` file.
