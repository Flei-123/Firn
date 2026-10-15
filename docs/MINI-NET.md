# JARVIS mini chat box: network pieces

| Piece | File | Proof |
|---|---|---|
| fUi + network in ONE thread | `lib/fui/looppump.fi` (`attach(app, &loop)`), `app.set_pump`, `apphost.hooks_set_pump` (`lib/@linux/fui/apphost.fi`; web: no-op) | `tools/mini/pump_main.fi`: 3 loop timer rounds reach a running `app.run` window (Xvfb); Windows build compiles |
| JARVIS client | `lib/net/jarvisc.fi`: login (`POST /api/login`, token from the body), WebSocket `message/stop/confirm_result/ask_result`, events `token/done/tool/thinking/conversation/error/info/user/confirm/ask`, history GET, send queue while the socket is down | `tools/mini/check.py` + `client_main.fi` against `tools/mini/fake_jarvis.py` (41 checks + what the server saw) |
| Remember the login | `jc_save/jc_restore/jc_forget` (`std.secret`); the token goes out as `Cookie: fleitec_session=<urlencoded>`. Set-Cookie need not be read: the login answer carries `token` | same check (second client connects without login) |
| Window handle + hotkey in `app.run` | `app.set_window_hooks(a, ud, opened, hotkey)` (`apphost.hooks_set_window`; web: no-op): `opened(ud, win)` once after the window is open, `hotkey(ud, id)` for `window.EV_HOTKEY`; `app.set_secret(a, id, on)` masks a text field | `tools/mini/app_check.sh` |
| The app | `examples/jarvis_mini/main.fi` (~250 lines): frameless, topmost, bottom right, markdown chat (`fui.markdownview` in a `scene.node_set_draw` node), login (user, password, stored token), Enter sends, Esc hides, Ctrl+Alt+J toggles, top strip drags, window height follows the chat | `tools/mini/app_check.sh`: X11 (Xvfb + openbox + xdotool) against the fake server -- frameless / topmost / corner, login, message -> answer in the window (pixels change, height grows), hotkey hide/show, second start without login |

Run the pieces: `FIRNC=<firnc> tools/mini/run.sh`; the app: `FIRNC=<firnc> APP_SHOTS=<dir> tools/mini/app_check.sh` (screenshots as PNG).
Both need Python `websockets` (+ Xvfb), so they are NOT in tests/ (the test runner has no such prerequisite); they skip with a notice when a tool is missing.
Not verified: Windows (the app builds for `x86_64-windows`, never run; Wine 8 refuses `RegisterHotKey`), Wayland, a real wss server, the title strip drag under a real WM (xdotool cannot drag a `_NET_WM_MOVERESIZE`).
Limits: see the HONEST blocks in the two library headers (blocking DNS in ahttp freezes the window; with an IME the pump is polled every 10 ms).
