# Firn Playground

Browser page to try Firn without installing anything: editor with syntax highlighting, Run button (Ctrl+Enter), output pane with stdout, stderr and compiler errors (clickable, jump to the line), example dropdown.

## Run

    bash tools/playground/run.sh

Listens on `127.0.0.1:8123` only. Settings by environment: `PLAYGROUND_HOST`, `PLAYGROUND_PORT`, `PLAYGROUND_FIRNC`, `PLAYGROUND_FIRNLIB`, `PLAYGROUND_RUN_TIMEOUT` (default 10 s), `PLAYGROUND_COMPILE_TIMEOUT` (20 s), `PLAYGROUND_JOBS` (4 parallel), `PLAYGROUND_RUNTIME`.

Do not expose it publicly without a reverse proxy that adds rate limiting.

## How it works

`POST /api/run {"source": ...}` compiles and runs the program on the server (firnc has no WASM host build yet).

- Source is written to a fresh `mktemp` directory, removed after the request.
- Compile and run each happen in `bwrap` (all namespaces unshared: no network, own PID/IPC/UTS/mount), as uid 65534, with `prlimit` caps on address space, CPU seconds, file size, process count and no core files.
- The sandbox sees only a read-only copy of `/usr`, the staged compiler and `lib` (`/var/tmp/firn-playground-runtime`, rebuilt when firnc or lib changes) and its own `/work`.
- Run timeout kills the whole process group; stdout and stderr are cut at 64 KiB (the run is killed on overflow); source is limited to 64 KiB.
- Compiler errors are parsed (`--> main.fi:LINE:COL`) into `diagnostics` for the editor.
- No `heavy` wrapper: a request is a sub-second compile plus a capped run, at most `PLAYGROUND_JOBS` at once.

## Test

    bash tools/playground/test.sh

Covers hello, compile error with line, panic, endless loop stopped, output flood cut, empty and huge source, file isolation, all six examples, static files, path traversal.

## Files

- `server.py` HTTP endpoints, `sandbox.py` compile and run, `runtime.py` staged compiler copy, `diagnostics.py` error parser, `config.py` limits
- `static/` page: `index.html`, `style.css`, `app.js`, `highlight.js`
- `examples/*.fi` dropdown examples (served by `/api/examples`)
