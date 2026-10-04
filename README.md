# Firn

Firn is a systems programming language with its own compiler. `firnc` turns
`.fi` source files into native executables without libc, without LLVM and
without a C backend: it writes the machine code itself and uses only an
assembler and a linker (or, for WebAssembly, nothing at all).

The compiler (`compiler/`, Rust, no external crates) is the production
compiler. A second compiler written in Firn (`bin/firnc1.fi`, `lib/firnc1/`)
compiles itself. The language is described in [SPEC.md](SPEC.md).

## Targets

| `--target=` | output | runs on |
|---|---|---|
| `x86_64-linux` (default) | static ELF, no libc | Linux x86-64 |
| `aarch64-linux` | static ELF, no libc | Linux ARM64 |
| `x86_64-windows` | PE/COFF `.exe`, own import table, no C runtime | Windows x64 |
| `x86_64-android`, `aarch64-android` | shared library for an APK (`tools/android/build.sh`) | Android (NativeActivity, no Java) |
| `wasm32-browser` | `.wasm` module plus a small JS host | browsers, Node |

Options that apply everywhere: `--opt-level=dev|dev-fast|release-safe|release-fast`,
`--pic`, `--cpu=avx`, `--test` (built-in test runner), `--lsp` (language
server). `firnc --help` lists all of them. `--win-subsystem=windows` builds a
Windows GUI program without a console window.

Assemblers and linkers the targets need: GNU binutils (`as`, `ld`) for
x86-64 Linux, `aarch64-linux-gnu` binutils for ARM64 and Android, and
`x86_64-w64-mingw32` binutils for Windows (used only as assembler and linker;
no MinGW library enters the image). Android packaging additionally needs the
Android SDK build tools.

## Quick start

```sh
cargo build --release --manifest-path compiler/Cargo.toml
export FIRNLIB="$PWD/lib"
FIRNC=./compiler/target/release/firnc

$FIRNC -o /tmp/tour examples/tour.fi && /tmp/tour
# hello, Firn -- dist2 25, dist 5, box 12, sum 10

$FIRNC --target=x86_64-windows -o tour.exe examples/tour.fi
$FIRNC --target=wasm32-browser -o tour.wasm examples/tour.fi && node tools/wasm/run.mjs tour.wasm
```

`FIRNLIB` points the compiler at the standard library. `ld` warns about a
`LOAD segment with RWX permissions`; the binary is freestanding and the
warning is harmless.

Hello world:

```firn
import std.io

fn main() -> i32 {
    io.print_line("Hello, Firn")
    return 0
}
```

A window ([examples/window/main.fi](examples/window/main.fi)) -- X11 on
Linux, Win32 on Windows, from the same source:

```sh
$FIRNC -o /tmp/window examples/window/main.fi && /tmp/window
$FIRNC --target=x86_64-windows --win-subsystem=windows -o window.exe examples/window/main.fi
```

`bash test.sh` runs the full acceptance suite (it takes a while).

## The language

Structs, arrays, `enum` + `match` with exhaustiveness checking, generics,
interfaces, closures and function values, error unions (`E!T`, `try`,
`catch`), `defer`/`errdefer`, reference parameters (`x: &T`, `x: inout T`, also
`&self` / `inout self`), `for x in array` and `for i in a..=b`, destructors
(`fn drop(inout self)`, run at the end of the block, with a move checker),
`comptime`, `f32`/`f64`, `str` with f-strings
(`f"x = {x}"`), checked integer arithmetic and bounds checks, `import a.b as c`,
`__include_str("file")` to embed a file at build time, `extern fn` and
`#[export_c]` for the C ABI, inline assembly, `profile kernel` for
freestanding code, and an optional incremental garbage collector
(`gc_init()`, weak references, finalizers).

## Libraries

Everything lives under `lib/` and is written in Firn. For writing an application (what exists, where, what proves it) see [docs/APPLICATION_LIBS.md](docs/APPLICATION_LIBS.md); start a new one with `tools/newapp.sh`.

| area | modules |
|---|---|
| core | `std.io`, `std.str`, `std.text`, `std.mem`, `std.vec`, `std.map`, `std.rc`, `std.math`, `std.num`, `std.bytes`, `std.base64`, `std.json`, `std.hash`, `std.md5`, `std.cpu` |
| system | `std.fs` (create, rename, remove, stat, atomic write), `std.dir`, `std.time` (clocks, `sleep_ms`, calendar, ISO 8601, time zones), threads and mutexes (`thread_start`/`thread_wait` in `lib/gc`, real threads on Linux and Windows), `std.process` (spawn, pipes, wait, kill, process groups; [docs/ROUND-PROCESS.md](docs/ROUND-PROCESS.md)), `std.shell` (open URL/path, known folders, find java), `std.pool` (worker threads), `std.secret` (keyring: Credential Manager / encrypted file; [docs/STD_ARCHIVES.md](docs/STD_ARCHIVES.md)) |
| network | `std.net` (TCP), `net.http` (HTTP/1.1 client: chunked, gzip, redirects, cookies, cache), `http.server` (HTTP/1.1 server: routes, GET/POST, headers, body, keep-alive, WebSocket, Server-Sent Events, optional TLS, pairing token), `ws.ws` (WebSocket client, `ws://` and `wss://`), `net.dns` (resolver: A/AAAA, CNAME, UDP+TCP, cache; [docs/DNS.md](docs/DNS.md)), `net.download` (parallel downloads, resume, ETag, mirrors, SHA check; [docs/DOWNLOAD.md](docs/DOWNLOAD.md)), `async.*` (event loop, non-blocking TCP/TLS, async HTTP, WebSocket client with reconnect; [docs/ASYNC.md](docs/ASYNC.md)), `net.dbus` (D-Bus client/service) |
| security | `tls.tls` (TLS 1.2 and 1.3 client with X.509 validation, [docs/TLS12.md](docs/TLS12.md)), `tls.tls_server` (TLS 1.3 server, P-256 certificates), `std.crypto.*` (SHA-1/256/512, HMAC, HKDF, AES-GCM, ChaCha20-Poly1305, X25519, P-256 ECDSA sign/verify, RSA verify, system randomness), `auth.oauth` / `auth.msa` (OAuth2/OIDC device code + PKCE, Microsoft/Xbox/Minecraft login chain; [docs/OAUTH.md](docs/OAUTH.md)) |
| data and documents | `std.deflate` (DEFLATE, zlib, gzip), `zip.zip` (read/write, ZIP64), `std.tar` / `std.extract` / `std.hashfile` (tar, tar.gz/zst/xz/bz2, one-call archive extraction, streaming hashes; [docs/STD_ARCHIVES.md](docs/STD_ARCHIVES.md)), `compress.*` (zstd, xz/LZMA2, brotli, bzip2, lz4, auto-detect; [docs/COMPRESSION.md](docs/COMPRESSION.md)), `db.*` (embedded SQL database, SQLite 3 file format; [docs/DB.md](docs/DB.md)), `markdown.*` (CommonMark + GFM; [docs/MARKDOWN.md](docs/MARKDOWN.md)), `qr.*` (QR encoder and decoder), `pdf.pdf` (PDF writer with embedded TrueType), `regex.regex` (linear time), `i18n.i18n` (catalogs, plural rules, number/date formats), `print.print` (IPP printing) |
| images and fonts | `jpeg.jpeg` (baseline + progressive), `webp.*` and `gif.*` (incl. animation; [docs/IMAGE_DECODERS.md](docs/IMAGE_DECODERS.md)), `paint.png` (PNG in and out, palette/16-bit/Adam7), `svg.*` (SVG painter), `font.*` (TrueType reader and rasteriser) |
| input | `input.input`: mouse, buttons, wheel, keys, text, volume and media keys; `/dev/uinput` on Linux, `SendInput` on Windows, one interface |
| UI | `window.window` (X11, Win32, Android), `fui.*` (incl. `fui.kit`: toast, sidebar, tiles, modal, search; `markdownview`, `richtext`, animated images, touch; [docs/fui-kit.md](docs/fui-kit.md), [docs/UI_EXTRAS.md](docs/UI_EXTRAS.md)), `plat.*`, `fuishell.*` |
| desktop | `desktop.*` (tray, notifications, file drop, watcher, autostart), `audio.*` (PCM, PulseAudio/waveOut, MP3 player); [docs/DESKTOP.md](docs/DESKTOP.md) |
| app framework | `appkit.*` (signed auto-update against store.fleitec.com, config, log, crash report, single instance, FleiTec-ID login), `tools/newapp.sh` (new app from template), `tools/pack/all.sh` (installers and packages for Windows, Linux, macOS, Android, OrientOS); [docs/APPKIT.md](docs/APPKIT.md), [docs/PACKAGING.md](docs/PACKAGING.md) |
| web page | `web.dom`: the page's DOM from `wasm32-browser` code -- query, create, text, attributes, styles, classes, events, `eval`; through one fixed JS file (`lib/web/dom.js`), no generator |
| web engine | `html`, `css`, `dom`, `layout`, `js`, `paint` -- the parts of a browser engine (HTML tokenizer and tree builder, CSS cascade, layout, painting, a JavaScript interpreter) |

## fUi

fUi (`lib/fui/`) is Firn's UI toolkit: a scene tree with flex layout, style
sheets and themes, widgets (buttons, text fields with IME, lists, sliders,
switches, file chooser), vector shapes with anti-aliasing, SVG and
Lucide icons, animation, and accessibility data. It paints with its own
software rasteriser or on the GPU (OpenGL ES on Android, WebGL in the
browser). The core builds without a memory allocator, so the same widgets
can run inside a kernel.

Platforms: Linux (X11, spoken directly without Xlib), Windows (Win32),
Android (NativeActivity + EGL/GLES), and the browser (`wasm32-browser`,
canvas or WebGL). `lib/plat/fuiwin.fi` connects fUi to a window.

The quickest way in is `fui.app` ([docs/fui-quickstart.md](docs/fui-quickstart.md)):
a window with a label and a button is ten lines, and the same file builds as
an X11 program and as a browser page (`--target=wasm32-browser`). `fui.app`
has no Windows or Android host yet; there, use `lib/plat/fuiwin.fi` directly.

```firn
import fui.app

fn close(a: *mut app.App) {
    app.quit(a)
}

fn main() -> i32 {
    let a: *mut app.App = app.window("Test Window", 400, 250)
    app.label(a, "Hello, this is a test window!", 16)
    app.button(a, "Close", close)
    return app.run(a)
}
```

## Example: phone remote

[examples/phone_remote](examples/phone_remote) turns a phone into a touchpad
and remote for the computer it runs on: an HTTP server with a WebSocket, a
page embedded with `__include_str`, and `lib/input`. The program is 36 lines.

```sh
$FIRNC -o /tmp/remote examples/phone_remote/main.fi && /tmp/remote
# Open on the phone: http://192.168.1.54:8080/?t=<token>
```

Only the browser that opened the printed URL (with its one-time token) is let
in. The same source builds as a Windows `.exe`.

## Example: DOM from WebAssembly

[examples/dom](examples/dom) builds a small list app whose whole logic is Firn:
it creates the elements, handles clicks and reads input fields. No browser
lets WebAssembly touch the DOM directly, so `lib/web/dom.js` passes node
handles and UTF-8 text across; texts are always set as text, never as HTML.

```sh
$FIRNC --target=wasm32-browser -o app.wasm examples/dom/main.fi
cp lib/web/dom.js examples/dom/index.html . && python3 -m http.server
```

## Licence

The whole repository -- compiler, standard library, tools, tests, examples --
is licensed under the **Mozilla Public License 2.0** ([LICENSE](LICENSE),
[NOTICE](NOTICE)). Every source file carries an `SPDX-License-Identifier`
line, which is authoritative for that file. There are no GPL files.

MPL-2.0 is file-based: programs you write in Firn are yours under any licence
and may be closed; changes to Firn's own files must stay open when you
distribute them. MPL-2.0 is compatible with the GPL.

Exceptions, each with its own permissive licence: the icon path data in
`lib/fui/lucide.fi` (ISC; icons from Feather: MIT), and the Unicode data,
fonts and test fonts listed in [THIRD_PARTY.md](THIRD_PARTY.md) with their
licence texts in `LICENSES/`. [LICENSING.md](LICENSING.md) records where
files came from.

## Known limits

* Pre-1.0: the language and the library interfaces may still change.
* Ownership is checked conservatively, not by a full borrow checker: moves of
  values with a `drop` and the "one `inout` per call" rule are enforced; a
  reference can still be copied into a raw pointer, and `drop` does not apply
  to `gc class` / `Rc[T]` yet. The self-hosted compiler `firnc1` does not know
  `drop` (it reports such files as not ported).
* Windows: no threads and no child processes yet (they report `ENOSYS`); no
  debug information in `.exe` files. Creating symbolic links needs Windows
  developer mode or administrator rights.
* WebAssembly: programs that use files, sockets, threads, SIMD or inline
  assembly are refused at compile time, with the reason.
* `net.http` (the HTTP client) does not speak `https://` yet; TLS is
  available directly through `tls.tls`.
* WebP images are decoded only on Android (through the system); Firn has its
  own JPEG and PNG decoders.
* `input.type_text` assumes a US keyboard layout. Linux needs write access to
  `/dev/uinput`; on Windows, `SendInput` cannot reach windows of elevated
  programs.
* No conditional compilation: platform code is chosen by module files
  (`backend.fi` / `backend.windows.fi`, or a `window/backend.fi` next to the
  program).
