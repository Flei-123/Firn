# Firn Playground

Browser page to try Firn without installing anything: editor with syntax highlighting, Run button (Ctrl+Enter), output pane with stdout, stderr and compiler errors (clickable, jump to the line), example dropdown.

## Editor

- Highlighting: `static/tokenizer.js` (f-string interpolation as code, `{{`/`}}` escapes, format specs, calls incl. methods and generic calls, types, constants, attributes, numbers with suffixes, nested block comments).
- Typing: auto-close for `{ ( [ "` with skip-over, pair backspace, selection wrapping, smart Enter between braces, auto-indent, Tab/Shift+Tab (also for multi-line selections), Ctrl+/ comment toggle, Ctrl+D duplicate line, Alt+Up/Down move lines, Ctrl+Shift+K delete lines, smart Home, Ctrl+A. Logic in `static/editing.js` (pure functions), applied through `document.execCommand`, so every action is one native undo step (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z).
- Completion (Ctrl+Space or while typing): keywords, prelude, std module names, `module.` members, methods after `.` by inferred receiver type (Vec/Map/Set/str/number), symbols of the current file. Data comes from `static/completions.json`, generated from `lib/` and `compiler/src/{lsp,prelude}.rs` by `python3 tools/playground/gen_completions.py` (run it after lib changes; `--check` is part of the test).
- Live errors: 500 ms after the last key the page posts the source to `/api/check`; red wavy underline at the exact line, column and length, tooltip on hover, error counter and list in the status bar (click jumps to the position). Positions of the old result are shifted while typing and dropped when the edit touches them; stale answers are discarded by a request counter.

## Run

    bash tools/playground/run.sh

Listens on `127.0.0.1:8123` only. Settings by environment: `PLAYGROUND_HOST`, `PLAYGROUND_PORT`, `PLAYGROUND_FIRNC`, `PLAYGROUND_FIRNLIB`, `PLAYGROUND_RUN_TIMEOUT` (default 10 s), `PLAYGROUND_COMPILE_TIMEOUT` (20 s), `PLAYGROUND_JOBS` (4 parallel), `PLAYGROUND_RUNTIME`.

## Service

Production runs as systemd unit `firn-playground` (`tools/playground/firn-playground.service`, installed to `/etc/systemd/system/`, enabled at boot) on `0.0.0.0:8110` from the copy in `/root/jarvis/data/firn-playground` (`app/` = `tools/playground/`, `firnc`, `lib`). After compiler, lib or playground changes: copy again, `systemctl restart firn-playground`.

Public access goes through the reverse proxy (`playground.fleitec.com` -> `192.168.1.54:8110`); the proxy should add rate limiting.

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

Covers hello, compile error with line, panic, endless loop stopped, output flood cut, empty and huge source, file isolation, all six examples, static files, path traversal.

## Files

- `server.py` HTTP endpoints, `sandbox.py` compile and run, `runtime.py` staged compiler copy, `diagnostics.py` error parser, `config.py` limits
- `static/` page: `index.html`, `style.css`, `app.js` (wiring), `tokenizer.js`, `highlight.js`, `editing.js`, `textedit.js`, `completion.js`, `popup.js`, `problems.js`, `statusbar.js`, `tooltip.js`, `metrics.js`, `livecheck.js`, `completions.json` (generated)
- `examples/*.fi` dropdown examples (served by `/api/examples`)
