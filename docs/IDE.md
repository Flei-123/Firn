# Firn in VS Code

Open a file, write, press F5, see errors live, get completion. Extension: `editors/vscode-firn` (`.vsix`).

## Linux (5 steps)

1. Build the compiler: `cargo build --release --manifest-path compiler/Cargo.toml` (gives `compiler/target/release/firnc`).
2. Put `tools/firn` and `compiler/target/release` on `PATH` (`firn`, `firnc`), `export FIRNLIB=<repo>/lib`.
3. Pack and install the extension: `bash editors/vscode-firn/package.sh /tmp/out && code --install-extension /tmp/out/vscode-firn-*.vsix`.
4. `firn new hello && cd hello && code .` (a console project: `fn main() { println("hi") }`).
5. Open `src/main.fi`, press **F5** (or the play button top right): it runs `firn run`. Errors appear while typing.

## Windows (5 steps)

1. Get `firn-windows-x86_64.zip` (built by `bash tools/windows/ide/build_kit.sh OUT_DIR`) and unzip it.
2. Run `install.ps1` (PowerShell). It copies `bin`, `lib` to `%LOCALAPPDATA%\firn`, sets `PATH` and `FIRNLIB`, and runs `code --install-extension`.
3. Open a new terminal: `firn new hello`, `cd hello`, `code .`.
4. Open `src\main.fi`, press **F5**.
5. Check: `firn doctor` prints the tool locations.

## What the extension does

| Feature | How |
|---|---|
| Errors while typing | `firnc --lsp` compiles the buffer with the real front end (imports, prelude), debounced 150 ms |
| Completion | names in scope, keywords, prelude (`println`, `Vec`, ...), members after `mod.` (e.g. `io.`) |
| Hover, Go to Definition | locals, functions, structs, consts; `io.print_line` jumps into `lib/std/io.fi` |
| Rename | scope aware |
| Format on save | `firnfmt` through the language server |
| Run (F5, play button) | `firn run`: the project of the file (nearest `firn.package`), else the single file |
| Tasks | `Terminal > Run Task > firn`: build, test, check, run, fmt; problem matcher `$firnc` |
| Snippets | `main`, `for`, `forr`, `struct`, `impl`, `vec`, `map`, `fstr`, `pl`, `test`, ... |

## Settings

`firn.path`, `firn.compilerPath`, `firn.formatterPath`, `firn.libPath` (FIRNLIB), `firn.optLevel`, `firn.saveBeforeRun`, `firn.server.enabled`. Empty paths use `PATH`.

## Limits

- Windows: `firn run` needs the bundled MinGW `as`/`ld` (in the zip). Only x86-64 Windows, console and window programs; `firn build --target`, `firn doc`, `firn add` are Linux only for now.
- Go to Definition follows `import` of std and relative modules, not package dependencies.
- Debugging: see docs/DEBUGGER.md (not wired into the extension).

## Tests

`bash tools/lsp/run.sh` (31 checks, real LSP client), `cd editors/vscode-firn && node --test test/`, `bash tools/firn/test_firn.sh`.
