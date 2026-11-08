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

| Consumer | How it gets Firn | State |
|---|---|---|
| fUi | lives in the Firn repo (`lib/fui`) | source of truth |
| Osum | `vendor/firn/COMMIT` = `33b67b5e7`, `firn.toolchain` = `firn 0.1.4894`, `tools/firnpin/check.sh`, `vendor/firn/fetch-firnc.sh` | patches all upstream |
| Certus (certus-ui) | `vendor/firn/COMMIT` = `7893ea2c8d74...` (hash not in the current Firn history), `firn.toolchain` = `firn 0.1.4893`, own partial fUi copy `.fui-win` (17 files vs 249 in `lib/fui`) for the Windows build | stale copy, r63 |
| Certus (/root/certus, old checkout) | `COMMIT` = `52a13e75...` (old private history) | do not use |
| FirnChat | no pin file; `build.sh` takes `$FIRNC`, a firnc in PATH, or a Firn checkout (`$FIRNLIB`, `~/firn`) | unpinned |

What fUi needs for one shared pin (nightly 0.1.4889+; newest toolchain installed: 0.1.4898):
1. A version stamp consumers can read from fUi itself (revision of `lib/fui`), so `check.sh` can compare it with `firn.toolchain`.
2. No consumer-only patches: every Certus patch (`0001-fui-painter-fontreq` and others) must be upstream in `lib/fui`, then `.fui-win` is deleted and Windows builds use `$FIRNLIB`.
3. One pin file format for all four consumers (Osum's `firn.toolchain` + `COMMIT`); FirnChat gets one.
4. One bump script that sets `COMMIT` + `firn.toolchain` in every consumer (Firn plan, step 3 of "Top 10").
5. fUi API freeze list (stable names in `fui.app`, `fui.kit`, `fui.services`) so a bump does not break consumers silently.
6. Compatibility smoke per consumer after a bump: Osum `fuiscene` build, Certus Windows build, FirnChat build.
