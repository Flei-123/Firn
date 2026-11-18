# One app, every platform (web, Android, Windows, Linux)

Goal: one program written in Firn + fUi builds for the browser (wasm), Android, Windows and Linux (later OrientOS) from the same sources. Platform code lives behind `lib/plat/host` and `lib/plat/hostwin` (twin rule: `lib/plat/X.fi` = no backend, `lib/@web/plat/X.fi`, `lib/@android/plat/X.fi`), never in the app. Orbit (firnchat) is the first user, not a special case.

## 1. What works today (measured 18.11.2026)

Program: `examples/fui/hello_window.fi` (10 lines, `fui.app`). Compiler `firnc 0.1.0 (stage 0)`, library = origin/main + the Android host backend. Same source, no `#if`.

| target | command | build | size | run | result |
|---|---|---|---|---|---|
| Linux | `firnc -o hello examples/fui/hello_window.fi` | 4.4 s | 2.44 MB | Xvfb + `scrot` | window painted |
| Web | `firnc --target=wasm32-browser ...` + `demos/webapp/index.html` | 3.7 s | 1.31 MB wasm | headless Chromium (Playwright) | page painted |
| Windows | `firnc --target=x86_64-windows --win-subsystem=windows ...` | 14.9 s | 3.04 MB exe | Wine 64-bit on Xvfb, still running after 45 s | window painted |
| Android | `tools/android/screenapp.sh` (`build.sh --abi both`) | 20 s | 3.40 MB apk (arm64 + x86_64, signed) | emulator `a31e` (x86_64, API 35): start 2.2 s | window painted, Close button |

Findings:
- All four targets run from one source file; no target needed a code change.
- Android: a cold emulator often shows "System UI isn't responding" over the app. `tools/android/screensmoke.sh` now dismisses it (`dismiss_anr`) before the screenshot.
- Windows: only Wine was available here (no real Windows); the window comes up and stays alive, input was not tested.
- Linux `--shot` works for `fui.appshot` apps (the generated template), not for the plain `examples/fui` programs.

## 2. One command per target (E2)

```
firn new myapp                       # fUi app: firn.package, fui.app, main.fi, data/
cd myapp
firn build --target web              # dist/web/index.html + myapp.wasm + firn.js + fonts + data
firn build --target android          # dist/android/myapp-x86_64.apk (signed; --abi arm64|x86_64|both)
firn build --target windows          # dist/windows/myapp.exe + data/
firn build --target linux            # dist/native/myapp + data/
firn build --target all              # all four
firn targets                         # which targets this machine can build
```

- `firn` is the front door; the work is done by the existing `fui` CLI (`tools/fui/cli`): one file per target in `tools/fui/cli/targets/`. New there: `windows.sh`; `linux` is an alias of `native`.
- `firn new NAME` makes the fUi app (template `counter`, others: `--template blank|settings|files`); `firn new NAME --console` keeps the old print program, `--lib` the library.
- The app only imports `fui.app`, `fui.appshot`, `fui.kitdata`; assets are read by name (`kitdata.load`), never by path.
- Test: `bash tools/appkit/newapp_test.sh` (section 6) and `bash tools/firn/test_firn.sh`.

## 3. The host interface on Android (E3): done, stub, missing

Two interfaces carry everything an app needs from its platform. Browser twin = `lib/@web/plat/`, Android twin = `lib/@android/plat/` (since the merge of the 2b branch), Windows = missing (E5).

`plat.host` (`lib/plat/host.fi`, Android: `lib/@android/plat/host.fi` + `lib/plat/android/host*.fi`):

| call | Android state |
|---|---|
| `fetch` | done: `async.ahttp` on the UI loop, relative URLs against the origin (`hostorigin`), JSON body |
| `stream_open/close` | done: SSE parser (`ssefeed`, test 5101), reconnect after 3 s |
| `store_get/set` | done: file in the app directory (`services.svcosstore`) |
| `tz_offset`, `dark`, `wake_in` | done (JNI `TimeZone`, theme event, timer) |
| `clip_write/read` | done (`plat.android.share`, text only, 64 KB) |
| `navigate` | external URLs only (`fileio.fio_open_url`); in-app routes (`#/...`) not handled |
| `location` | STUB (returns 0): needs an in-app route string plus the back key |
| `keyboard`, `edit_mode`, `edit_wrap`, `edit_set` | STUB. The pieces exist: `window.text_input/text_sync` + `fui.textbuf` + `lib/android/inputmethod.fi` (InputConnection as in-memory dex). Missing: `lib/plat/android/hostedit.fi` that maps one `TextBuf` to `WE_EDIT` (text, caret, anchor, composing range) and to `WE_TEXT`/Backspace when only `keyboard()` is on |
| `font_extra` | STUB (0): emoji font as APK asset |
| `tz_reset` | no-op (correct on Android) |

`plat.hostwin` (`lib/plat/hostwin.fi`, Android: `lib/@android/plat/hostwin.fi`): window, frame loop (on demand), touch, keys (web names), text, wheel, resize, theme change: done (demo app `tools/android/hostwin_demo.fi` builds for both ABIs and starts). `cursor`, `damage`, `use_gpu`, `gpu_enable`, `font_apply` are no-ops (software present, faces through `fui.face`).

Existing Firn Android libs that are NOT behind `plat.host` yet (needed by Orbit, today `firnchat_*` externs): file pick/save (`plat/android/pickfile.fi`), photo pick (`pick.fi`), share and open (`share.fi`), notification and badge (`push.fi`, `window.notify`), back key (`plat/backkey.fi`), insets (`kbd.fi`, `window.android_set_insets`). Each becomes one call in `plat.host` with a stub, a web twin and an Android twin.

Not tested on a device or emulator (only compiled): fetch/stream against a server, store across restarts, clipboard, every `edit_*` call. Tested on the emulator: window + frame loop through the fUi path (section 1) and the `firn new` app (`dist/android` apk: greeting from the asset, counter, Count button).

Next in this order: (1) `hostedit.fi` + route string for `location/navigate`, with a login-free emulator demo that logs `store`, `fetch` (local server on 10.0.2.2), `clip`, typed text (`adb shell input text`); (2) `notify/pick/save/share/back` calls; (3) emoji and Inter as assets for `hostwin`.

## 4. Orbit on the same road (E4, concept only, no Orbit change)

Orbit (`src/gui/web_main.fi`, 14,325 lines before the r618 split) calls the browser only through `plat.web`; after the split the entry shrinks and the platform part is one module. What Orbit needs for `android_main` / `windows_main`:

1. Take the 2a branch of Orbit (`staff/fleitec-all-in-one-mwdstd1f`: `web_main`, `hotptr`, `taskctl`, `calctl`, `mailctl` use `plat.hostwin` + `plat.hostev`) and move the Firn pin to a version that contains `plat.hostev`/`plat.hostwin`. Pixel gate showed 6/6 identical screenshots.
2. New entries `src/android_main.fi` and `src/windows_main.fi`: only the platform init (origin, store path, `hostwin.app` with the same paint/event functions as the browser entry). No `#if` in the product code.
3. Token login instead of the cookie: `host.fetch` needs a header hook (`host.set_auth(token)`); the server already accepts `Cookie` or an `fkt1-` token on `/api/v1/call` (r50). Login by Fleitec-ID password (later device code/OIDC, `docs/LOGIN-OIDC.md`); token stored through `host.store_set` (later Android Keystore).
4. Assets: Inter, `emoji.ttf`, `pdffonts.pack` as APK assets (`tools/android/build.sh --assets`); `font_extra` returns the emoji face.
5. The `firnchat_*` externs (image pick, file open, copy, notify, unread, push device, url, zoom) move behind `plat.host` calls (section 3) so the product code stays platform free.
6. Push: reuse `plat/android/pushio.fi` (relay connection as foreground service, no Firebase); build flags `--push --pick` in `tools/android/build.sh`.
7. Back key and routes: `host.location/navigate` with an in-app route stack; `plat/backkey.fi` decides between "route back" and "leave".
8. Build: `fui.app` manifest key `entry host` so that `firn build --target android|windows` also builds hostwin apps (permissions and flags from the manifest), instead of Orbit's own scripts.
9. Test: emulator check like `tools/android/firnchat_check.sh` with a login-free demo account (send, receive, notification).

## 5. Windows backend (E5)

Not started: it follows the Android contract, and Android is not green yet (IME, in-app route, notification are stubs). Plan: `lib/@windows/plat/host.fi` (fetch/stream over `net.http` + `tls` on a worker, store as file in `%APPDATA%`, clipboard `std/clipboard`, URLs via the shell, time zone via Win32) and `lib/@windows/plat/hostwin.fi` (the Windows `window.window` backend with software present, same event mapping as Android). The same source then builds with `firn build --target windows`.
