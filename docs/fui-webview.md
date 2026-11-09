# fUi WebView widget: API contract v0

Status 09.11.2026 (ecosystem M11). Contract version constant: `webview.API_VERSION = 0`. Additive changes keep the version; a changed meaning of an existing call bumps it.

Two sides meet in one struct, `webview.Backend` (a table of function pointers plus `ud`). fUi never imports Glass; Glass never imports fUi. The adapter that fills `Backend` from Glass calls lives on the Certus side (`lib/embed/webview_fui.fi` in the Certus repo), because it imports both.

```
 app  --  fui.webview (widget model, tick, input routing, services, a11y)
              |  Backend (this document, section 3)
 adapter (Certus repo)  --  glass.webview (docs/WEBVIEW-COMPONENT.md in Certus)
```

## 1. Goals and non-goals

- A rectangle in a fUi window shows a page painted by an embedded engine; input, focus, IME, clipboard and dialogs work like in any other widget.
- Engine-agnostic: the stub backend (test pattern) proves the contract without Certus; Certus Glass is the first real backend; a native OS web engine could be a second.
- v0 non-goals: several pages per process, scripts bridge, downloads, popups, devtools, touch, drag and drop, text selection export, WASM target (a WebView in WASM is a nested browser).

## 2. fUi side

### 2.1 Widget

| Item | v0 |
|---|---|
| Module | `fui.webview` (model, tick, input), `fui.webview_stub` (stub backend), `fui.webview_ax` (a11y), `fui.webview_svc` (services adapter), `fui.webview_node` (window glue for `fui.app`) |
| Node | `app.custom` node of kind `widget.KIND_TILE` with a painter (`scene.node_set_draw`); focusable, in the Tab chain |
| Model | `webview.WebView`, value struct owned by the app; one per node; holds the `Backend`, the pixel buffer, state, event queue, caret rect |
| Create | `webview.webview_open(&wv, &backend, w, h)` -> bool; `webview_close(&wv)` frees the buffer and calls `Backend.destroy` once |
| Load | `webview.webview_load(&wv, p, n, kind)`; `kind` = `LOAD_URL` or `LOAD_HTML` (v0 backends may support only HTML) |
| States | `ST_NEW`, `ST_LOADING`, `ST_READY`, `ST_FAILED` (backend dead or `create`/`load` failed); the widget paints a neutral error surface in `ST_FAILED`, the app lives on |

### 2.2 Pixel surface hand-off

Default mode `SURFACE_BUFFER`: the widget owns an `0xAARRGGBB` straight-alpha buffer (`w*h*4` bytes, `stride` in bytes, `fui.uiimage` format) and passes `(px, stride, cap)` to `Backend.paint`. The backend writes only inside `[0, stride*h)` and only during `paint`; it keeps no pointer. Return values (same numbers as Certus `webview_render`): `RENDER_UNCHANGED = 0`, `RENDER_CHANGED = 1` (damage rectangle set), `RENDER_BUFFER_TOO_SMALL = 2`. The widget wraps the buffer with `image_wrap` and draws it 1:1 (`FIT_NONE`) at the node rect; only damaged frames request a repaint (`tick` returns true).

Reserved mode `SURFACE_CANVAS` (capability `CAP_CANVAS`, not implemented in v0): the backend paints straight into the window painter through `painter_set_canvas` / `painter_raster` (r630, `docs/fui-painter-backend.md`), saving the copy. The capability bit is part of the contract now so that adapters can announce it later.

### 2.3 Raw input hook

The widget forwards input to the backend untransformed, as one record per event (`webview.Input`); coordinates are widget-local device pixels (origin = top-left of the node rect, the widget subtracts its own offset).

| `kind` | fields |
|---|---|
| `IN_MOVE`, `IN_DOWN`, `IN_UP` | `x`, `y`, `button` (0 left, 1 middle, 2 right), `mods` |
| `IN_LEAVE` | none |
| `IN_WHEEL` | `x`, `y`, `dx`, `dy` (notches, positive = down/right), `mods` |
| `IN_KEY_DOWN`, `IN_KEY_UP` | `key` (`editor.KEY_*`), `mods` |
| `IN_CHAR` | `cp` (code point) |
| `IN_PREEDIT` | `text` (UTF-8), `cursor` (byte offset, -1 = hidden) |
| `IN_COMMIT` | `text` (UTF-8) |
| `IN_FOCUS`, `IN_BLUR` | none |
| `IN_REPLY` | `a` = request id, `b` = status (1 ok, 0 cancelled/failed), `text` = payload |

`Backend.input` returns true when the page used the event. Unused keys stay with the app (shortcuts). Reserved keys are never forwarded in v0: Tab / Shift+Tab (focus leaves the widget, no keyboard trap) and Escape while no page text input is active. `mods` use the `editor` modifier bits. Events are forwarded only while the widget is focused (keys, chars, IME) or under the pointer / captured (pointer, wheel).

### 2.4 Tick hook

`webview.webview_tick(&wv, now_us) -> bool` is called once per frame by the host (window pump, `app.set_extra_wake`); `now_us` is an argument (virtual clock in tests, never read from the wall clock inside). It: (1) applies a pending resize, (2) calls `Backend.tick(ud, now_us)` (animations, loads, timers inside the engine), (3) paints when the backend says it is dirty, (4) drains up to `EVENT_BUDGET` (64) events and dispatches them, (5) returns true when the picture or the caret rect changed. A backend that reports `TICK_DEAD` moves the widget to `ST_FAILED`. `Backend.wait_ms(ud, now_us)` is optional (-1 = no timer) and feeds the window's sleep limit.

### 2.5 IME routing

While focused and the backend has `CAP_IME`: the host IME layer (`fui.ime`) sends preedit and commit to the widget (`webview_preedit`, `webview_commit`), which forward `IN_PREEDIT` / `IN_COMMIT`. The backend reports the caret as a rectangle in widget-local pixels (`Backend.caret(ud, &rect) -> bool`, polled after each tick and after every forwarded key); the widget converts it to window coordinates and calls `ime.ime_set_spot` (through the glue hook `spot(ud, x, y, h)`). Without `CAP_IME`, committed text arrives as `IN_CHAR` per code point.

### 2.6 Services adapter

The page never reaches fUi directly. The backend emits request events; `fui.webview_svc` maps them to `fui.services` and answers with `IN_REPLY` (the request id is echoed).

| Backend event | `fui.services` call | Reply |
|---|---|---|
| `EV_CLIP_WRITE` (text) | `clip_copy` | none |
| `EV_CLIP_READ` | `clip_request`, answer arrives as `EV_CLIP` | `IN_REPLY` text |
| `EV_CONFIRM` / `EV_ALERT` (title, text in `text` split by `\n`) | `confirm` / `message` | `IN_REPLY` status |
| `EV_OPEN_FILE` | `open_request` -> `EV_OPENED` | `IN_REPLY` text = file bytes |
| `EV_SAVE_FILE` (name `\n` data) | `save_request` -> `EV_SAVED` | `IN_REPLY` status |

Permissions are default-deny per WebView: `webview_set_perms(&wv, bits)` with `PERM_CLIP`, `PERM_DIALOG`, `PERM_FILES`; a denied request is answered with status 0 without calling the service. `EV_STORE_*` (key-value) is reserved for v1 (key prefix per WebView id).

Navigation: `EV_NAV` (absolute href of a clicked link, or a load request from the page) is a REQUEST. The app sets `webview_on_nav(&wv, hook)`; the hook returns `NAV_ALLOW` (widget calls `Backend.load`), `NAV_DENY` or `NAV_EXTERNAL` (the app opens it elsewhere). No hook = `NAV_DENY`.

### 2.7 Other backend events (information, no reply)

`EV_TITLE`, `EV_LOADED` (a = ok, b = document height), `EV_HOVER` (href under pointer or empty), `EV_CURSOR` (cursor kind, forwarded to `fui.cursorreq`), `EV_SCROLLED`, `EV_RESIZED`, `EV_OVERFLOW`. Numbers 1..8 equal the Certus `EV_*` constants of `glass.webview`, so the adapter needs no mapping. The app reads the last title / hover / scroll / document height from the model (`webview_title`, `webview_hover`, `webview_scroll`, `webview_doc_height`).

### 2.8 Size, DPI, focus, lifecycle

- Size: the widget measures its node rect in device pixels each frame; a change calls `Backend.resize(ud, w, h, scale_milli)` once per tick (at most) and reallocates the buffer; the next paint is full. `scale_milli` = 1000 * device pixel ratio; backends without scaling (Certus v0) ignore it and the widget still draws device pixels.
- Focus: `webview_focus(&wv, on)` sets the focus ring state and sends `IN_FOCUS` / `IN_BLUR`. A click on the node takes focus. Focus loss ends any preedit (`IN_PREEDIT` with empty text).
- Lifecycle: `open` -> (`load`)* -> `close`. `close` is idempotent. After `ST_FAILED` the app may `close` and `open` again.

### 2.9 Accessibility

New role `a11y.ROLE_DOCUMENT` (number 35, `ROLE_COUNT` 36; AT-SPI `DOCUMENT_WEB` 95, UIA `Document` 50030, ARIA `document`). The node is focusable, named by the page title (fallback: the app's label), described by the hovered link while there is one. v0 exports exactly this node (`webview_ax.fi`); the page's own tree is the Glass side's v1 `Backend.ax` (flat list: role, name, rect, state, parent index), which the widget would attach as children without a contract change.

## 3. The `Backend` table (the seam)

| Field | Signature | Required |
|---|---|---|
| `ud` | `u64`, passed back to every call | yes |
| `caps` | `u32` bits: `CAP_IME`, `CAP_CLIP`, `CAP_FILES`, `CAP_DIALOG`, `CAP_CANVAS`, `CAP_URL`, `CAP_AX` | yes |
| `create` | `fn(ud, w, h) -> bool` | yes |
| `destroy` | `fn(ud)` | yes |
| `load` | `fn(ud, kind, p, n) -> bool` | yes (HTML-only backends return false for `LOAD_URL`) |
| `resize` | `fn(ud, w, h, scale_milli) -> bool` | yes |
| `paint` | `fn(ud, px, stride, cap, damage) -> u32` | yes |
| `input` | `fn(ud, input) -> bool` | yes |
| `tick` | `fn(ud, now_us) -> u32` (`TICK_IDLE`, `TICK_DIRTY`, `TICK_DEAD`) | yes |
| `next_event` | `fn(ud, ev) -> bool` | yes |
| `caret` | `fn(ud, rect) -> bool` | with `CAP_IME` |
| `wait_ms` | `fn(ud, now_us) -> i64` | no |

The call set is small and pointer-free on purpose: every call can be expressed as a `glass_wire` message, so the Certus side can move from in-process (v0) to a host process (v1) without changing this contract.

## 4. Glass side: what Certus must implement

Reference: Certus `docs/WEBVIEW-COMPONENT.md` and `lib/glass/webview.fi` (v0 prototype, landed 08.11.2026). The adapter maps `Backend` onto it:

| `Backend` | `glass.webview` today | Status |
|---|---|---|
| `create` / `destroy` | `webview_create` / `webview_destroy` | present |
| `load` (HTML) | `webview_load_html` | present |
| `load` (URL) | none (v1: wire `WM_LOADURL`, needs `glass_net`) | MISSING |
| `resize` | `webview_resize` (no scale) | present, scale MISSING |
| `paint` | `webview_render(dst, stride, cap, damage)` (same ARGB + RENDER_* values) | present |
| `input` move / down / up / wheel / key | `webview_mouse_move`, `webview_mouse_button`, `webview_wheel`, `webview_key` | present (adapter maps button kinds and key codes) |
| `input` char / preedit / commit / focus | none | MISSING (`WM_TEXT`, `WM_IME_PREEDIT`, `WM_IME_COMMIT`, focus enter/leave) |
| `tick` | none (paint on demand; no timers without scripts) | adapter returns `TICK_DIRTY` when `webview_render` would change, `TICK_IDLE` otherwise; cheap dirty flag MISSING (render is O(w*h) per call) |
| `next_event` | `webview_next_event` (`EV_TITLE`..`EV_OVERFLOW`, numbers equal) | present |
| `caret` | none | MISSING (engine must report the text-input caret rect) |
| services events (`EV_CLIP_*`, `EV_CONFIRM`, ...) | none (needs scripts and a bridge `window.certus.request`) | MISSING (v1) |
| `ax` | none | MISSING (v1) |
| crash isolation | `glass_proc` host process, `gp_alive()` | v1: map `!gp_alive` to `TICK_DEAD` |

Rules for the Glass side: single-threaded API called from the UI thread; `WebView` lives in the adapter's stack frame or a caller-owned cell (collector scans frames); event text is valid until the next `next_event`; the engine never keeps a pointer into the host buffer between calls; content is untrusted (HTML/CSS only in v0, no scripts, no network).

## 5. Test and demo

| Check | File |
|---|---|
| Contract and widget model against the stub backend | `tools/fui/webview_main.fi` (in `tools/fui/run.sh`) |
| Demo app (stub backend, flag `--certus` for the Glass adapter once present) | `examples/fui_apps/webview_demo` |

## 6. Versioning and open points

- v0 (this document): buffer surface, raw input, tick, IME routing, services map, one a11y node, stub backend.
- v1: URL load and navigation policy per origin, `Backend.ax`, `SURFACE_CANVAS`, scale, touch, `EV_STORE_*`, script bridge (`postMessage` <-> `EV_MESSAGE`), crash page.
- Licensing: the Glass side is GPL-2.0, fUi is MPL-2.0; the adapter sits in the Certus repo so that no Glass code enters the Firn repo (roadmap r26).
