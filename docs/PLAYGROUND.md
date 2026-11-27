# Firn Playground

Browser page to try Firn without installing anything: editor with syntax highlighting, Run/Stop button (Ctrl+Enter), interactive terminal pane (live output, typing into `io.read_line()`), compiler errors (clickable, jump to the line), example dropdown.

## Editor

- Highlighting: `static/tokenizer.js` (f-string interpolation as code, `{{`/`}}` escapes, format specs, calls incl. methods and generic calls, types, constants, attributes, numbers with suffixes, nested block comments).
- Typing: auto-close for `{ ( [ "` with skip-over, pair backspace, selection wrapping, smart Enter between braces, auto-indent, Tab/Shift+Tab (also for multi-line selections), Ctrl+/ comment toggle, Ctrl+D duplicate line, Alt+Up/Down move lines, Ctrl+Shift+K delete lines, smart Home, Ctrl+A. Logic in `static/editing.js` (pure functions), applied through `document.execCommand`, so every action is one native undo step (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z).
- Completion (Ctrl+Space or while typing): keywords, prelude, std module names, `module.` members, methods after `.` by inferred receiver type (Vec/Map/Set/str/number), symbols of the current file. Data comes from `static/completions.json`, generated from `lib/` and `compiler/src/{lsp,prelude}.rs` by `python3 tools/playground/gen_completions.py` (run it after lib changes; `--check` is part of the test).
- Live errors: 500 ms after the last key the page posts the source to `/api/check`; red wavy underline at the exact line, column and length, tooltip on hover, error counter and list in the status bar (click jumps to the position). Positions of the old result are shifted while typing and dropped when the edit touches them; stale answers are discarded by a request counter.

## Terminal

- Run starts a session: the program output streams live into the terminal pane (stdout white, stderr orange, typed input blue). Prompts without a line feed (`print("Name: ")`) show at once: `print` writes straight to fd 1 and the sandbox uses pipes, so nothing is buffered.
- Typing goes into a hidden input that the pane mirrors with a blinking cursor (editing keys, paste and mobile keyboards work natively). Enter sends the line and echoes it into the scrollback, Ctrl+D sends EOF (or the unfinished line), Ctrl+C or the Stop button kills the program, Ctrl+Enter restarts. The last line shows the exit code or why the run was stopped.
- Code: `static/terminal.js` (pane and input), `static/session.js` (EventSource for output, queued POSTs for input), wiring in `static/app.js`.

## Sessions API

- `POST /api/session {"source"}` -> `{"id"}` (400 invalid source, 503 busy). Compile and run happen in a background thread.
- `GET /api/session/<id>/events` (SSE, resumable with `Last-Event-ID` or `?last=N`): `status`, `started` (with `limits`), `out`, `err`, then exactly one of `exit {code, reason, compile_ms, run_ms}` or `failed` (compile error, same shape as `/api/run`). `reason`: `exit`, `signal`, `stopped`, `timeout`, `idle`, `output_limit`, `disconnected`. A comment heartbeat every 5 s keeps proxies open.
- `POST /api/session/<id>/stdin {"data"}` or `{"eof": true}` (409 not running, 413 over 16 KiB total); `POST /api/session/<id>/stop`.
- Limits: 60 s total (`PLAYGROUND_SESSION_TIMEOUT`), 30 s without output or input (`PLAYGROUND_SESSION_IDLE`), 64 KiB output, 20 s CPU, `PLAYGROUND_SESSIONS` (4) parallel, `PLAYGROUND_SESSIONS_PER_CLIENT` (2, keyed by `X-Real-IP`). Same bwrap/prlimit sandbox as `/api/run`.
- Cleanup: the process group is killed and the mktemp directory removed when the program ends, a limit hits, Stop is sent, or no event stream is attached for `PLAYGROUND_SESSION_ABANDON` (8 s). Finished sessions stay replayable for 60 s.
- Code: `session.py` (one session), `sessions.py` (registry and slots), `session_http.py` (routes, SSE), `sandbox.compile_program` shared with `/api/run`.

## Run

    bash tools/playground/run.sh

Listens on `127.0.0.1:8123` only. Settings by environment: `PLAYGROUND_HOST`, `PLAYGROUND_PORT`, `PLAYGROUND_FIRNC`, `PLAYGROUND_FIRNLIB`, `PLAYGROUND_RUN_TIMEOUT` (default 10 s), `PLAYGROUND_COMPILE_TIMEOUT` (20 s), `PLAYGROUND_JOBS` (4 parallel), `PLAYGROUND_RUNTIME`.

## Service

Production runs as systemd unit `firn-playground` (`tools/playground/firn-playground.service`, installed to `/etc/systemd/system/`, enabled at boot) on `0.0.0.0:8110` from the copy in `/root/jarvis/data/firn-playground` (`app/` = `tools/playground/`, `firnc`, `lib`). After compiler, lib or playground changes: copy again, `systemctl restart firn-playground`.

Public access goes through the reverse proxy (`playground.fleitec.com` -> `192.168.1.54:8110`, NPM in LXC 101, blocks in `/opt/npm/data/nginx/custom/http.conf`). `/api/check`, `/api/run` and `/api/session` have their own `limit_req` zones (new sessions 10/min per IP); `/api/session/` has `proxy_buffering off`, `proxy_read_timeout 90s` and its own zone for the event stream and input posts.

## How it works

`POST /api/run {"source": ...}` compiles and runs the program on the server (firnc has no WASM host build yet).

`POST /api/check {"source": ...}` runs only the front end (`firnc --emit=fir-raw`, output discarded, no assembler, linker or run) in the same sandbox and returns `{"ok", "diagnostics": [{severity, message, line, col, length, hint}]}`. It has its own concurrency limit (`PLAYGROUND_CHECKS`, default 4) and timeout (`PLAYGROUND_CHECK_TIMEOUT`, 8 s). The proxy rate limit applies to `/api/run` only; add `/api/check` there if it should be limited too.

- Source is written to a fresh `mktemp` directory, removed after the request.
- Compile and run each happen in `bwrap` (all namespaces unshared: no network, own PID/IPC/UTS/mount), as uid 65534, with `prlimit` caps on address space, CPU seconds, file size, process count and no core files.
- The sandbox sees only a read-only copy of `/usr`, the staged compiler and `lib` (`/var/tmp/firn-playground-runtime`, rebuilt when firnc or lib changes) and its own `/work`.
- Run timeout kills the whole process group; stdout and stderr are cut at 64 KiB (the run is killed on overflow); source is limited to 64 KiB.
- Compiler errors are parsed (`--> main.fi:LINE:COL`) into `diagnostics` for the editor.
- No `heavy` wrapper: a request is a sub-second compile plus a capped run, at most `PLAYGROUND_JOBS` at once.

## Test

    bash tools/playground/test.sh

Runs Node tests (`tests/*.test.mjs`: tokenizer, editing, completion), the server tests (`test.py`) and the headless Chromium tests (`browser_test.py`, real key events over CDP, needs `chromium` and the Python `websockets` module, skipped otherwise). `PLAYGROUND_SHOT=<png>` saves a screenshot at the end of the browser test.

Covers sessions (prompt visible before input, two-line dialog, EOF, stop, idle timeout, total timeout, output flood, disconnect cleanup, limits, compile error), browser typing into the terminal (Enter, Backspace, Ctrl+D, Ctrl+C, Stop button, restart), hello, compile error with line, panic, endless loop stopped, output flood cut, empty and huge source, file isolation, all seven examples, static files, path traversal.

## Files

- `server.py` HTTP endpoints, `session*.py` interactive sessions, `sandbox.py` compile and run, `runtime.py` staged compiler copy, `diagnostics.py` error parser, `config.py` limits
- `static/` page: `index.html`, `style.css`, `app.js` (wiring), `tokenizer.js`, `highlight.js`, `editing.js`, `textedit.js`, `completion.js`, `popup.js`, `problems.js`, `statusbar.js`, `tooltip.js`, `metrics.js`, `livecheck.js`, `terminal.js`, `session.js`, `completions.json` (generated)
- `examples/*.fi` dropdown examples (served by `/api/examples`)
