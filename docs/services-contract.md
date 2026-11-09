# Services contract v1 (`lib/services`)

Roadmap r1411 (M2, docs/FOUNDATION-PLAN in projects_kb). ONE API for the platform services an app needs beyond a
window: **file**, **clipboard**, **dialog**, **storage**. Notifications stay in `std.toast` (later `svcnotify`).

The contract WRAPS what exists; it builds nothing twice:

| service | native (Linux / Windows / Android) | browser (`wasm32-browser`) |
|---|---|---|
| file open | `std.dialog.open_file(s)` + `std.fs.read_file` | `plat.webdialog.web_dialog_file` (upload, `<input type=file>`) |
| file save | `std.dialog.save_file` + `std.fs.write_atomic`; Android `plat.fileio.fio_share` | `plat.webdownload.web_download` (download) |
| clipboard | `std.clipboard` (copy_text / paste_text) | `plat.webclip` (write sync, read async) |
| dialog | `std.dialog.message` / `confirm` | `plat.webdialog.web_dialog_message` |
| storage | one file per key under `appkit.platform.plat_dir(DIR_DATA, app)`/`services/` | host `store_get` / `store_set` (localStorage), `plat.web` |

## Modules

| module | path | role |
|---|---|---|
| `services.svc` | `lib/services/svc.fi` | status codes, capabilities, app name, request ids, the reply queue (`poll`), `host_event` |
| `services.svcfile` | `lib/services/svcfile.fi` | `open`, `save` |
| `services.svcclip` | `lib/services/svcclip.fi` | `copy_text`, `paste_text` |
| `services.svcdialog` | `lib/services/svcdialog.fi` | `message`, `confirm` |
| `services.svcstore` | `lib/services/svcstore.fi` | `get`, `set`, `remove`, `has` |
| `services.svcdir` | `lib/services/svcdir.fi` | `list`, `pick`, `next_entry`, `push_entry`, `DirEntry` |
| backend (native) | `lib/services/svcos.fi` | wraps std.dialog / std.clipboard / std.fs / plat.fileio |
| backend (Windows) | `lib/services/svcos.fi` (same file as native) | `std.dialog` win32 (comdlg32 / user32), `std.clipboard` win32, `plat.fileio` |
| backend (Android) | `lib/services/svcos.fi` with `lib/@android/std/*` | dialogs answer `NO_BACKEND` (no windowless picker), clipboard `std.clipboard`, `plat.fileio` share |
| backend (browser) | `lib/@web/services/svcos.fi` | wraps plat.webdialog / webclip / webdownload / plat.web store; `from_web_event` |
| backend (OrientOS) | spec only (below) | |

Apps import the four service modules (and `svc` for `poll`); `svcos` is picked by the compiler's platform directory
(`lib/@web/` first for `wasm32-browser`, like `std.clipboard`). fUi binds through its own adapter `lib/fui/services*.fi`.

## Error model

Every call returns an `i32` status, never crashes, never blocks longer than the user's answer to a dialog.
Numbers 0-3 are the ones of `std.dialog`.

| const | value | meaning |
|---|---|---|
| `svc.OK` | 0 | done (sync) / request started (async: a reply follows) |
| `svc.CANCELED` | 1 | the user closed the dialog |
| `svc.NO_BACKEND` | 2 | this system cannot do it (no display, no helper, no host import); see `caps()` |
| `svc.FAILED` | 3 | tried, did not work (I/O error, host refused) |
| `svc.BUSY` | 4 | an async request of the same kind is still open (browser: one file open, one clipboard read) |
| `svc.NOT_FOUND` | 5 | storage key absent; clipboard empty or of another type |
| `svc.INVALID` | 6 | bad argument (key/app name rules, size limit) |

`svc.status_name(s) -> str` gives `"ok"`, `"canceled"`, `"no-backend"`, `"failed"`, `"busy"`, `"not-found"`, `"invalid"`.

## Sync vs async

| call | kind | why |
|---|---|---|
| `svcfile.save(name, mime, p, n) -> i32` | sync | download / save dialog answer at once |
| `svcfile.open(accept, multiple, id: *mut u32) -> i32` | **async** | browser upload arrives later |
| `svcclip.copy_text(s) -> i32` | sync | browser: allowed inside a user gesture |
| `svcclip.paste_text(id: *mut u32) -> i32` | **async** | browser clipboard read is a promise |
| `svcdialog.message(kind, title, text, buttons, answer: *mut i32) -> i32` | sync | |
| `svcdialog.confirm(title, text, yes: *mut bool) -> i32` | sync | |
| `svcstore.get(key, out: *mut rt.Buf) -> i32`, `set(key, value: str)`, `remove(key)`, `has(key) -> bool` | sync | |
| `svcdir.list(path, out: *mut rt.Buf) -> i32` | sync | `OK`, `NOT_FOUND`, `DENIED`, `NOT_DIR`, `INVALID`, `NO_BACKEND` (web) |
| `svcdir.pick(id: *mut u32) -> i32` | **async** | web only: the user picks a directory, its tree becomes listable (see below); native `NO_BACKEND` |
| `svcdir.next_entry(buf, pos: *mut usize, e: *mut DirEntry) -> bool` | sync | records `name`, `is_dir`, `is_link`, `broken`, `size`, `mtime_ns`; name valid while `buf` lives |

ASYNC = request + reply. The request returns `OK` and a request id (> 0) and the result comes as one or more
`svc.Reply` through `svc.poll`. Native backends answer at once: the replies are queued before the request returns,
so the SAME loop works everywhere:

```
import std.rt
import services.svc
import services.svcfile

var id: u32 = 0 as u32
if svcfile.open(".txt,.md", true, &id) == svc.OK { ... }
var r: svc.Reply = svc.reply_new()
while svc.poll(&r) {                 // each frame / after each host event
    if r.op == svc.OP_FILE_ITEM { /* r.index, rt.buf_str(&r.name), r.data */ }
    if r.op == svc.OP_FILE_END { /* r.status OK (r.index = count) | CANCELED | FAILED */ }
    if r.op == svc.OP_CLIP_TEXT { /* r.status OK: r.data = UTF-8 | NOT_FOUND */ }
}
svc.reply_free(&r)
```

```
struct Reply { id: u32, op: u32, status: i32, index: u32, name: rt.Buf, data: rt.Buf }
fn reply_new() -> Reply
fn reply_free(r: *mut Reply)
fn poll(r: *mut Reply) -> bool          // false = queue empty; r's buffers are cleared and refilled
fn pending() -> usize                   // queued replies
fn open_requests() -> usize             // async requests still waiting for the host
```

Ops: `OP_FILE_ITEM` (1) one opened file, `OP_FILE_END` (2) end of a file open, `OP_CLIP_TEXT` (3) clipboard text.

Host events (browser): the page loop hands every `web.WE_DIALOG` / `web.WE_CLIP` to
`svcos.from_web_event(e: *mut web.WebEvent) -> bool` (true = consumed). The portable form, present in every backend,
is `svc.host_event(kind, status, id, q: str, p: u64, n: usize) -> bool` (kind `svc.HOST_DIALOG` / `svc.HOST_CLIP`,
fields as in `plat.web.WebEvent`); native backends never need it and it answers `false` there when nothing is open.
A file item is any WE_DIALOG while a file open is waiting, except `"color"`, `"notify-click"`, `"notify-close"`.

## Capabilities

`svc.caps() -> u32`, a bit set, decided at run time (a native program without a display has no dialogs):

| bit | const | native Linux | Windows | Android | browser | OrientOS (spec) |
|---|---|---|---|---|---|---|
| 1 | `CAP_FILE_OPEN` | dialog backend (zenity/kdialog/dialogd) | yes | no | yes | dialogd |
| 2 | `CAP_FILE_SAVE` | dialog backend | yes | `fio_available()` | yes (download) | dialogd |
| 4 | `CAP_CLIP_WRITE` | `std.clipboard` backend != none | yes | no (window layer only) | yes | clipd |
| 8 | `CAP_CLIP_READ` | same | yes | no | yes (async, may ask permission) | clipd |
| 16 | `CAP_DIALOG` | dialog backend | yes | no | yes (alert/confirm) | dialogd |
| 32 | `CAP_STORAGE` | yes | yes | yes | yes | yes |
| 128 | `CAP_DIR_LIST` | yes | yes | yes (picked tree only) | no | no |
| 256 | `CAP_DIR_PICK` | no | no | yes | no | no |
| 64 | `CAP_ASYNC_HOST` | no | no | no | yes: replies need host events | yes |

`svc.has(cap) -> bool`, `svc.backend_name() -> str` (`"native"`, `"web"`).
A call whose capability is missing answers `NO_BACKEND` and changes nothing (the conformance test checks exactly that).

## Windows and Android

- Windows: the native backend, unchanged. `tools/services/conformance.sh` builds the conformance program with
  `--target=x86_64-windows` and runs it under Wine on a private Xvfb with `FIRN_SERVICES_HEADLESS=1`
  (that env var removes the dialog capabilities on every native system): caps 172 (storage, dir list, clipboard),
  25 checks. The real comdlg32 dialogs are covered by `tools/dialog/dialog_check_win.py`.
- Android: compile check for `x86_64-android` and `aarch64-android` only. Android has no windowless dialog, so a
  document picker needs the window layer (`onActivityResult`) and is not built; there has been no emulator run.

## Directory listing in the browser

A page has no file system, so `svcdir.list` works on a VIRTUAL tree the user hands over with `svcdir.pick(&id)`:

- Path form: `/<picked name>/sub/file`; `/` lists the picked directory. Relative paths answer `NOT_FOUND`.
- Reply `svc.OP_DIR_PICKED`: `status` `OK` (`index` = entries, `name` = `/<picked name>`) or `CANCELED` (the old tree stays).
- Picker: `showDirectoryPicker` when the browser has it, else `<input webkitdirectory>` (empty directories are missing there).
- Host answers (`svc.HOST_DIALOG`): `status` `DIALOG_DIR_ITEM` (3: `q` = path relative to the pick, `id` 1 = directory,
  `p/n` = 16 octets size + mtime in ms, little endian), `DIALOG_DIR_END` (4, `id` = count), `DIALOG_DIR_CANCEL` (5).
- Limits: 100000 entries per pick; one pick at a time (`BUSY`). Host import `firn_web_dialog_dir` (module `plat.webdir`).

## Rules and limits

- `svc.set_app(name) -> i32`: app name 1-64 octets `[A-Za-z0-9._-]` (default `"firn-app"`). Storage is per app.
- Storage key: 1-64 octets `[A-Za-z0-9._-]`, not starting with `.`; value UTF-8 text up to 64 KiB (localStorage
  stores strings; binary data: encode with `std.base64`). An empty value is the same as `remove`.
- Native storage: file `<plat_dir(DIR_DATA, app)>/services/<key>`, written with `fs.write_atomic`.
  Browser storage: localStorage key `firn.svc.<app>.<key>`.
- `accept` of `svcfile.open`: `""` (every file) or a comma list of extensions `".png,.jpg"`. Native turns it into the
  std.dialog filter `"Files|*.png;*.jpg|All files|*"`; the browser hands it to `<input accept>`.
- A browser gives a file's NAME and CONTENT, never a path; so does the contract on every backend (`r.name` = base name).
- File size: native reads up to 256 MiB per file (more = `FAILED` item).
- Reply queue: 28-octet record head, then name and data, each padded to 4 octets, so every head is 4-aligned
  (aarch64 / wasm loads stay aligned for names of any length).

## Firn-page contract (Certus) compatibility

No new host import. The browser backend uses only imports that `demos/webdemo/firn.js` already answers
(`firn_web_dialog_file`, `firn_web_dialog_message`, `firn_web_download`, `firn_web_clip_write/read`,
`firn_web_store_get/set`) and the exports `firn_web_dialog` / `firn_web_clip` of `plat.web`. A page that does not import
`lib/services` has exactly the imports it had: Firn-page contract v1 (6 `firn`, 1 `env`, 5 `certus.fetch*`) is
unchanged. Certus does not answer the `firn_web_*` service imports yet, so a services page does not start there.

### Proposal (not agreed with Certus): capability `services`, contract v1.1

- ADDITIVE: contract v1 pages stay valid; nothing changes for a page that does not ask for it.
- A page declares it (meta tag `firn-capabilities` containing `services`); Certus then answers the seven imports above
  and calls the exports `firn_web_dialog` / `firn_web_clip` for the async answers. Undeclared use is refused at load.
- Certus answers: file open through its own file chooser (name + content, never a path), save as a download, clipboard
  text only, storage per page origin and app name, message/confirm as modal browser dialogs.
- Tracked for Certus in projects_kb/certus/FIRN-WEB.md; until it lands, `svc.caps()` on a v1 host is what the page sees.

## OrientOS backend (spec, not built)

`lib/@orientos/services/svcos.fi` once Osum has a platform directory: file open/save and dialogs through the fUi
dialog service `dialogd` (the same line protocol `std.dialog` uses on Linux, `FIRN_DIALOG=service`), clipboard through
a `clipd` service (text only in v1), storage as files under `/user/<app>/services/`. Async like the browser
(`CAP_ASYNC_HOST`): requests go to the service, answers come as host events into `svc.host_event`.

## Tests

- `tools/services/conformance.fi`: the conformance test, ONE source: per capability "works" or "answers
  NO_BACKEND"; storage round trip; async file/clip replies (native: queued at once; browser: host events simulated
  through `svc.host_event`).
- `tests/6705_services_headless.fi` (test.sh section 3, every build level): starts itself with its own `APPKIT_HOME`
  and no display; headless capabilities, storage inside `APPKIT_HOME`, reply queue with odd name/data lengths.
- `tools/services/conformance.sh`: runs it natively (no display; stand-in zenity + clipboard helper) and as
  `wasm32-browser` under `tools/wasm/run.mjs` (which answers the service imports with in-memory stand-ins).
