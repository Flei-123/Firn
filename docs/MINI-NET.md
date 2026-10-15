# JARVIS mini chat box: network pieces

| Piece | File | Proof |
|---|---|---|
| fUi + network in ONE thread | `lib/fui/looppump.fi` (`attach(app, &loop)`), `app.set_pump`, `apphost.hooks_set_pump` (`lib/@linux/fui/apphost.fi`; web: no-op) | `tools/mini/pump_main.fi`: 3 loop timer rounds reach a running `app.run` window (Xvfb); Windows build compiles |
| JARVIS client | `lib/net/jarvisc.fi`: login (`POST /api/login`, token from the body), WebSocket `message/stop/confirm_result/ask_result`, events `token/done/tool/thinking/conversation/error/info/user/confirm/ask`, history GET, send queue while the socket is down | `tools/mini/check.py` + `client_main.fi` against `tools/mini/fake_jarvis.py` (41 checks + what the server saw) |
| Remember the login | `jc_save/jc_restore/jc_forget` (`std.secret`); the token goes out as `Cookie: fleitec_session=<urlencoded>`. Set-Cookie need not be read: the login answer carries `token` | same check (second client connects without login) |

Run all: `FIRNC=<firnc> tools/mini/run.sh`.
Limits: see the HONEST blocks in the two library headers (blocking DNS in ahttp freezes the window; with an IME the pump is polled every 10 ms).
