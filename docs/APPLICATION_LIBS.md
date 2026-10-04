# Application libraries (OpenPlan LIB-001 … LIB-010)

OpenPlan, the electrical CAD built on Firn, listed the libraries it cannot
do without (`LIBRARY_REQUESTS.md` in the OpenPlan repository). This page
says what exists, where it lives, and what proves it. Every library has a
positive test in `tests/` (run in every build level and on AArch64 by
`test.sh`) and is held against an implementation nobody here wrote by
`tools/libmvp/run.sh` (section 65 of `test.sh`).

| request | module | what it does | held against |
|---|---|---|---|
| LIB-001 | `std.fs` (`lib/std/fs.fi`) | `mkdir_all`, `rename` (atomic), `remove`, `remove_tree` (does not follow links), `symlink`, `stat`/`lstat` (statx, one layout on every machine), `fsync_file`/`fsync_dir`, whole-file read/write/append, `write_atomic` (temp file, fsync, rename, fsync of the directory), path helpers; every failure is an `FsError` | the kernel's own answers (`tests/1910_std_fs.fi`) |
| LIB-002 | `std.time` (`lib/std/time.fi`) | wall and monotonic clock, the proleptic Gregorian calendar (Hinnant's day counts), ISO 8601 out and in, weekday/day of year, UTC offsets from TZif files v1–4 including the POSIX rule in the footer | Python `datetime`/`zoneinfo`: 20,000 instants × 5 zones |
| LIB-003 | `lib/pdf` (`pdf.fi`, `ttfsub.fi`) | PDF 1.7 writer in millimetres from the top left: paths, colours, dashes, clipping, TrueType text as CIDFontType2 with a subset font and ToUnicode, RGBA images with soft mask, internal and URI links, bookmarks, optional content (layers); deterministic output | poppler (`pdfinfo`, `pdffonts`, `pdftotext`, `pdftoppm` pixels), `pypdf` in strict mode, fontTools |
| LIB-004 | `lib/zip` (`zip.fi`) | ZIP reader and writer with store/DEFLATE, CRC-32, ZIP64, UTF-8 names; refuses zip slip, duplicate names, overlapping entries, size bombs (`inflate_limited` in `std.deflate`) | Python `zipfile` and Info-ZIP `unzip`/`zip`, both directions, plus hostile archives |
| LIB-005 | `fuishell.dock`, `fuishell.dockpaint`, `fuishell.filechooser`, `fuishell.chooserview` (`lib/fuishell/`), `window.wait_any`, `fuiwin.fenster_step`/`fenster_wait_many` | **done**: the dock model (areas left/right/bottom/center with tabs, splitter and tab dragging, layout saved/loaded as JSON), `Split`, the file chooser model (listing, sort, glob filter, open/save/directory), and several top-level windows in one program. The dock is also drawn (`fuishell.dockpaint`: tab strips, active tab with accent line, ellipsized titles, splitters, the drop hint while a tab is dragged). The file chooser has its face and hands too (`fuishell.chooserview`: path bar with an up button, the scrolling list with directory/file marks and a size column, name field of a save dialog, Cancel/OK; mouse with double click, wheel, keys, typing). **Done** apart from wiring it into OpenPlan itself | `tests/1918`, `tests/1919`, `tests/1920`, `tests/1921` (pixels read back); two windows on an Xvfb driven by python-xlib |
| LIB-006 | `window.clipboard_*` (`lib/window/x11.fi`), `plat.webclip` | the system clipboard on X11 (CLIPBOARD selection, owner and requestor, TARGETS, any MIME type); `fuiwin.fenster_clip_read/write` for fUi text fields. In the browser: `plat.webclip` (`web_clip_write`, `web_clip_read` answered as `WE_CLIP`) on navigator.clipboard -- text, image/png and a program's own types (Chromium's "web <type>" with the text alongside), the host keeping a refused copy for the page's own paste. Windows follows with the Windows target (LIB-016); other backends answer false and fUi keeps its internal clipboard; large contents (up to 64 MiB) go as INCR transfers in both directions | Tk and python-xlib on an Xvfb |
| LIB-007 | `lib/i18n` (`i18n.fi`) | `.opmsg` catalogs, fallback chain, variant selection, arguments, CLDR plural rules, number and date formatting for en, de, fr, it, es, pl, cs, ru; pseudo-localization (en-XA) | ICU 72 (PyICU): 8,885 cases |
| LIB-008 | `lib/regex` (`regex.fi`) | RE2-style regular expressions in linear time (Pike VM): classes, groups, named groups, lazy quantifiers, anchors, flags, replacement with `$1`/`${name}`; refuses backreferences and lookaround | Python `re` on 20,000 random patterns |
| LIB-009 | `lib/print` (`print.fi`) | IPP client over `net.http`: CUPS-Get-Printers/-Default, Get-Printer-Attributes, Print-Job, Get-Job-Attributes, Cancel-Job | CUPS' `ippeveprinter` (a real IPP Everywhere printer) and a stand-in for the CUPS scheduler |
| LIB-010 | `lib/jpeg` (`jpeg.fi`) | baseline and progressive JPEG to RGBA, any subsampling, restart markers, grey/YCbCr/RGB/CMYK, EXIF orientation; libjpeg's islow IDCT, fancy upsampling and colour tables | Pillow (libjpeg-turbo): the same RGBA octets for 46 files |
| FL-001 | `lib/webp` (`webp.fi`) | WebP to RGBA: lossless (VP8L, all four transforms, colour cache), lossy (VP8 key frames with loop filter, libwebp's fancy upsampling), ALPH alpha plane (raw or compressed, all filters), VP8X canvas, animations (first frame as a still; `webp_decode_anim` composes every frame like WebPAnimDecoder, with delays and loop count); size limit 16 Mpixel by default, hostile input gives `WebpError`, never a crash | Pillow (libwebp): the same RGBA octets for 991 files (generated corpus, 40 hand-muxed animations with every offset/blend/dispose combination, 838 Modrinth icons), lossless AND lossy, no tolerance |
| FL-002 | `lib/gif` (`gif.fi`) | GIF87a/89a to RGBA: LZW (1-8 bit, deferred clear, full table), global/local palettes, transparency, interlace, graphic control (delay, disposal 0-3), NETSCAPE loop count; `gif_decode` = first frame on the screen, `gif_decode_anim` = every frame as a canvas | Pillow: the same RGBA octets for 76 files incl. hand-written LZW streams (no clear, no EOI, 1-octet sub-blocks, KwKwK) and the Modrinth GIFs; known divergences listed in `check_gif.py` |
| FL-003 | `fui.uiimagedec` (`lib/fui/uiimagedec.fi`) + `lib/paint/png.fi` | `image_from_bytes`: PNG, JPEG, WebP or GIF by magic octets (`uiimage.image_format`) to an fUi picture; size read before any pixel memory is asked for. PNG now takes every colour type and bit depth, tRNS and Adam7 (it took 4 of 15 before; 78 % of Modrinth's PNG icons are palette PNGs) | `tests/2082_image_from_bytes.fi`: one picture as PNG/WebP/GIF decodes identically; `tests/2083_png.fi` and `check_png.py` (185 generated PNGs + 307 Modrinth PNGs, identical to Pillow) |

FL-001 .. FL-003 are the image decoders the FleiLauncher (Minecraft launcher
in Firn/fUi) needs for mod icons and screenshots. Details and the hostile-input
rules: `docs/IMAGE_DECODERS.md`.

## Launcher libraries (round PROCESS, `docs/ROUND-PROCESS.md`)

Asked for by a Minecraft launcher in Firn + fUi (FleiLauncher); useful for
any application that starts programs.

| module | what it does | held against |
|---|---|---|
| `std.process` (`lib/std/process.fi`, `process.windows.fi`) | start a program with arguments, working directory, changed environment; stdin/stdout/stderr inherited, piped or discarded; blocking and non-blocking reads; wait (with timeout), try_wait, exit code, kill/terminate, whole process trees (`set_group`, `kill_tree`, `terminate_tree`: a process group on Linux, a job object on Windows; `set_kill_with_launcher` on Windows), detach, `no_window`; `run_capture`/`run_io` collect both outputs while feeding input without deadlock; every failure is a `ProcError` | 3 MiB both ways through a child, 300 KB on stdout+stderr, 300 starts without a leak (`tests/2062`-`2064`, Linux, AArch64, Wine); the tree kill by a grandchild that holds the pipe open (`tests/2100`, Linux and Wine; see [WINDOWS_THREADS.md](WINDOWS_THREADS.md)) |
| `std.shell` (`lib/std/shell.fi`) | `open_url`, `open_path`, `reveal_in_file_manager`, `find_in_path`, home/config/data/cache/temp/Minecraft directories, `find_java`/`java_candidates`/`java_major_version` | a fake `xdg-open` and fake `java` scripts (`tests/2061`) |
| `std.env` (`lib/std/env.fi`) | read the environment, `find_executable` (`which`) | the kernel's `/proc/self/environ`, `sh` on PATH |
| `std.pool` (`lib/std/pool.fi`) | fixed worker threads, bounded job queue, results; Linux and Windows (real `CreateThread` threads since round WIN-THREADS, [WINDOWS_THREADS.md](WINDOWS_THREADS.md)) | `tests/2065`: 5000 jobs once each, parallel speed-up, GC on workers |
| `std.cmdline` (`lib/std/cmdline.fi`) | Windows command line quoting/splitting by the rules of the C runtime, UTF-8 <-> UTF-16 | the published examples + round trip (`tests/2060`) |
| FLEILAUNCHER | `net.dns` (`lib/net/dns.fi`), `net.udp`, `net.dnsconf`, `tls.trust`; `net.http` now fetches `https://host/...` | DNS by name: A/AAAA, CNAME chains, UDP with retry and a doubling timeout, TCP fallback on TC, random transaction ID, answers accepted only from the server asked, TTL cache, hosts file, resolv.conf (Linux) / GetNetworkParams (Windows); `https://` through the TLS client (TLS 1.3 and, since round TLS12, TLS 1.2 -- [TLS12.md](TLS12.md)) with the system's roots (PEM bundle on Linux, the certificate store on Windows) and the certificate checked against the NAME; secp256r1 added next to X25519 because Azure Front Door (`piston-meta.mojang.com`) refuses X25519-only. See [DNS.md](DNS.md) | a fake DNS server in Python (the server's log is counted), real answers of 1.1.1.1 decoded by an independent Python reader, `getaddrinfo`/`dig`/`curl` on the real network, a hermetic Python TLS server with a test CA (`tools/dns/run.sh`, also the Windows build under Wine) |

The compiler learned three more system calls for AArch64 on the way
(`unlinkat`, `symlinkat`, `statx`; `compiler/src/syscalls.rs`), and
`lib/svg/` became MPL-2.0 (LICENSING.md), which OpenPlan (GPL-3.0) needed
to use fUi.

## Libraries for FleiLauncher (std)

A Minecraft launcher in Firn/fUi needs to unpack Java runtimes, verify and
atomically write downloads, and keep tokens. Details, safety rules and the
honest limits: [STD_ARCHIVES.md](STD_ARCHIVES.md).

| module | what it does | held against |
|---|---|---|
| `std.tar` (`lib/std/tar.fi`) | tar reader (ustar, V7, GNU long names, pax), tar writer, `tar_add_tree`, tar.gz and (round COMPRESS) tar.zst / tar.xz / tar.bz2 / tar.lz4 with a size limit on the inflated stream, safe extraction with mode bits | GNU tar 1.34 and Python `tarfile`, both directions; fourteen hostile archives |
| `std.extract` (`lib/std/extract.fi`) | `extract_archive(path, dest)`: .zip, .tar.gz/.tgz, .tar.zst/.xz/.bz2/.lz4, .tar found by content, `strip` for the Adoptium top directory, modes (0755 for `bin/java`) | Info-ZIP, `zipfile`, GNU tar |
| `std.safefs` (`lib/std/safefs.fi`) | the shared rules: names, link targets, nothing written through a link, Windows names | the hostile archives above |
| `std.hashfile` (`lib/std/hashfile.fi`) | streaming md5/sha1/sha256/sha512 of a file, hex, `Download` (temporary file, verify, fsync, atomic rename) | Python `hashlib` |
| `std.secret` (`lib/std/secret.fi`) | keyring `secret_set/get/delete(service, key)`: Credential Manager on Windows, ChaCha20-Poly1305 file on Linux (machine-bound or Argon2id password) | Python `cryptography` + PyNaCl; the real Credential Manager (Wine and a Windows 11 machine) |

## Libraries for FleiLauncher (UI: Markdown, app kit)

The first program to need them is a Minecraft launcher in Firn / fUi; they are
general. Each has a positive test in `tests/` or a check in `tools/fui/run.sh`.

| module | what it does | held against |
|---|---|---|
| `lib/markdown` (`md.fi`, `doc.fi`, `block.fi`, `inline.fi`, `html.fi`; `docs/MARKDOWN.md`) | CommonMark 0.31 plus GFM tables, strikethrough, task items and bare URLs, into an arena tree; raw HTML ignored, escaped or kept (default: ignored, `<br>`/`<img>`/`<a>` kept in sense); HTML writer with a safe mode | the CommonMark spec: **all 652 examples** byte for byte, and 37 GFM cases from markdown-it-py (`tests/2090_markdown.fi`); pathological input bounded |
| `lib/fui/markdownview.fi` | a scrolling Markdown view: wrapped text, headings, lists, quotes, code, tables, task boxes, links with a click callback, images through an async hook (placeholder, ready, failed) | `tools/fui/mdview_main.fi`: pixels in light and dark, wide and narrow, WCAG 2 contrast |
| `lib/fui/kit.fi`, `lib/fui/kitcolor.fi` (`docs/fui-kit.md`) | button, toasts, modal with focus trap, tab bar and sidebar with symbols, tile grid with avatars, search field with clear button, progress bar with a label that reads on fill and track; the launcher accent as readable text on any ground | `tools/fui/kit_main.fi` (pixels, hit functions, focus ring, icon-free variants, every colour pair), `tools/fui/kitlive.py` (the example in a real window on Xvfb) |

## Async IO (round ASYNC)

One thread for many connections: an event loop with timers, non-blocking TCP
and TLS streams, an HTTP client and a WebSocket client that run on it, a way to
hand results from other threads to the UI thread, and a wait that serves a
window and the loop together. Design, API, what was proved against what, and
the honest list of gaps: [ASYNC.md](ASYNC.md).

| module | what it does | held against |
|---|---|---|
| `async.loop` (`lib/async/loop.fi`) | descriptors (epoll on Linux x86-64/AArch64, `poll` everywhere, `select` through the Windows seam), timers in a heap, deferred calls, `wake` from threads, hooks, a descriptor to embed in another wait (`loop_fd`) | the kernel; 4 build levels, AArch64 (qemu), Windows (Wine) (`tests/2120`) |
| `async.stream` | non-blocking TCP and TLS-client streams with one callback, input kept until consumed, output queue and back pressure, one lazy timeout per stream, an acceptor | echo with 120 clients, refused connect, idle timeout, 4 MiB through a closed window (`tests/2121`); **1000 connections open at once against Python asyncio**, both directions (`tools/async/check_conn.py`) |
| `tls.tls` (additions) | the client handshake as a state machine (`tls_handshake_step`, `WouldBlock`), buffered output; the blocking `tls_handshake` runs the same code in a loop | an in-process TLS server that dribbles 1-7 octets per ms, wrong name and unknown CA (`tests/2122`); the blocking suites (`tools/tls/run.sh`) unchanged |
| `async.ahttp` | HTTP/1.1 client on the loop over `net.http`'s request/head/cookie/gzip code: Content-Length, chunked, until-close, redirects, timeouts, https | 100 requests at once, dribbled chunked, redirect loop, gzip, cookies (`tests/2123`) |
| `async.wsc` (+ `ws.ws` additions) | WebSocket client: ws/wss, ping/pong, keep-alive, fragmentation both ways, closing handshake with timeout, reconnect with backoff | an in-process server (and `tls_server.fi`) with every violation the codec knows (`tests/2124`); **real wss:// echo services** (`tools/async/wss_main.fi`) |
| `async.post` | `post` from any thread to the loop thread, a bridge that runs blocking work on `std.pool` and answers on the loop thread | 4 threads x 500 posts in order, GC on workers while the loop sleeps, 25 stress runs (`tests/2125`) |
| `std.net` (additions) | `set_nonblocking`, `read_nb`/`write_nb`/`accept_nb`, `connect_nb` + `connect_finish`, `sock_error` | all of the above |
| `window.wait_any_fd`, `fuiwin.window_wait_many_fd` | wait for window events and a descriptor (the loop's) at once; a window with an event wins | a window + loop on an Xvfb: results arrive with no window event, an idle second costs no CPU (`tools/async/check_ui.py`) |

`bash tools/async/run.sh` runs the outside checks (test.sh section 77);
`bash tools/async/winkit.sh` builds the kit for a real Windows machine.

## The application kit (appkit)

A new Firn program should get updates from the own signed store, settings, a
log with rotation, crash reports, a single-instance lock and translated texts
without writing them again (FleiLauncher is the first user). The map, the
store format it reads and extends, the update flow, the platform layer and its
honest gaps: [APPKIT.md](APPKIT.md).

| module | what it does | held against |
|---|---|---|
| `appkit.update`, `appkit.catalog`, `appkit.fetch`, `appkit.version`, `std.crypto.ed25519` (`lib/appkit/`, `lib/std/crypto/ed25519.fi`) | the update client for the real store (`entry.json` + `index.json`, Ed25519 signatures, freshness and rollback protection, channels per platform, semver + build id): check, streamed download with SHA-256 and progress, atomic replacement of the running program, a confirmation by the new program and **rollback** when it crashes or hangs; a background thread, a worker process or a blocking call; on Android the system's PackageInstaller | `tools/appkit/e2e.sh`: 60 checks against a local store (a changed byte, a wrong signature, a wrong key, an expired or older catalog, redirects, cut-off and slow answers, a real replacement, rollback after a crash and a hang), Linux and Windows (Wine); `tools/appkit/android_check.sh` on the emulator; the RFC 8032 vectors and a real store entry (`tests/2050`); the live store read-only |
| `appkit.platform*` (`platform.fi`, `platform.windows.fi`, `lib/@android/appkit/platform.fi`, `platform_macos.fi`, `platform_osum.fi`) | one interface, one file per platform: directories, lock, spawn, language, replacing the program | `tools/appkit/platforms.py` (same names, same signatures, each type-checks); macOS and OrientOS are **untested stubs** |
| `appkit.config`, `appkit.log`, `appkit.crash`, `appkit.single_instance`, `appkit.texts`, `appkit.appinfo` | settings in one JSON file (atomic), a log with rotation, crash reports made by the next start, one copy per user, `.opmsg` texts (English and German built in), who the program is | `tests/2051`-`2056` in every build mode and under Wine |
| `templates/app` + `tools/newapp.sh` | `newapp.sh <Name> <app-id>` writes a runnable fUi program (sidebar, Settings, Updates, About, update banner, dark with a green accent) with build scripts for Linux, Windows and Android and a `release.sh` that publishes into the store | `tools/appkit/newapp_test.sh` (generate, build, start under Xvfb with a self-test, dry-run release) |

The store side (`store add-app`: `exe`, `bin`, `appimage`, `macos-app`
packages, per-platform channel pointers, `mindestFassung`) is in the
orientstore repository, `docs/KATALOG-FORMAT.md`.

## Packaging (lib/pack, tools/pack)

What turns a built program into the files people install it with, on every platform, without the platform's
own packaging tools: [PACKAGING.md](PACKAGING.md). `bash package.sh` in a project from `tools/newapp.sh`
(= `tools/pack/all.sh APP-DIR VERSION`) writes `dist/<version>/`: `.deb`, `.rpm`, `.tar.gz`, a real AppImage and a self-extracting
`.run`, the Windows `setup.exe` (installer + uninstaller + Start Menu/Desktop shortcuts + "Apps & features" entry), the
program with its icon, a portable zip and an NSIS script, `.app`/`.zip`/`.dmg` (structure only), an APK, an OrientOS `.opk`,
all icon sizes, and `manifest.json` (SHA-256 + Ed25519 per file) with the `store add-app` commands.

| module | what it does | held against |
|---|---|---|
| `pack.lnk` (`lib/pack/lnk.fi`) | Windows shortcuts (.lnk) written and read as plain octets (no COM): ID list, LinkInfo, Unicode strings | `tests/2200`; an independent reader (`tools/pack/test/lnkread.py`); Wine's shell starts the program through them |
| `pack.icons` (`lib/pack/icons.fi`, `tools/pack/icons.fi`) | PNG encoder, resampler, `.ico` and `.icns` writers; one SVG/PNG -> every size, Android mipmaps, hicolor theme | `tests/2201`; read back by `lib/paint/png.fi`, PIL, `unsquashfs`-free parsers in `checks.py` |
| `pack.payload`, `pack.install`, `pack.installed`, `pack.winreg` (+ `winreg.windows.fi`) | a zip appended to a program behind a hashed trailer; the installer state machine (extract, list, upgrade, shortcuts, `HKCU\...\Uninstall`, uninstaller that deletes itself); the registry writers (new `advapi32` imports in `compiler/src/win.rs`) | `tests/2202`, `tests/2203` (without Windows); `tools/pack/test/windows.sh` under Wine, window included |
| `tools/pack/pack.py` (`packlib/`) | zip, deb, rpm, tar, SquashFS + AppImage, plist/.app/.dmg, OPKG, PE icon resource, NSIS script, manifest + signatures; Python standard library only | `tools/pack/test/checks.py`: ~170 checks against dpkg-deb, `dpkg -i` / `rpm -i` in a container, unsquashfs, the AppImage runtime, OrientOS's `opk.py`, makensis, xorriso |
| `tools/pack/stub/selfx.fi` | the self-extracting Linux program (unpack once to the cache, `execve`, `$APPIMAGE` set so appkit updates the file itself) | `checks.py`: run, cached start, damaged file refused |

## Round WIN-THREADS / TLS 1.2 (threads, process trees, TLS 1.2)

| module | what it does | held against |
|---|---|---|
| Windows threads (`lib/gc/gc.fi` thread runtime on `CreateThread`, `compiler/src/thread.rs`, `win.rs`, `win_seam.rs`) | `thread_start/wait`, mutex, channel, `std.pool` on real threads on Windows; the seam is thread safe | `tests/834`, `860`-`862`, `1600`, `2065`, `2066`, `2091`, `2101` give the same output on Linux and under Wine; `tools/windows/threadkit.sh` writes a kit for a real Windows PC. See [WINDOWS_THREADS.md](WINDOWS_THREADS.md) |
| process trees (`std.process`: `set_group`, `kill_tree`, `terminate_tree`, `set_kill_with_launcher`) | stop a child and everything it started: process group (Linux), job object (Windows) | `tests/2100`: a grandchild that holds the child's stdout pipe; end of file proves it died (Linux, Wine) |
| `tls.tls` TLS 1.2 (`lib/tls/tls.fi`, `prf12.fi`; AES-256 in `aes.fi`/`gcm.fi`) | ECDHE-ECDSA/RSA with AES-128/256-GCM and ChaCha20-Poly1305, X25519 + secp256r1, extended master secret, downgrade protection, the same certificate and host name check; negotiated in the one ClientHello (no fallback retry) | `tests/2102`, `2103`; `tools/tls/tls12_check.py`: openssl s_server (suites x groups x signature schemes), Python `ssl` (512 KiB), man in the middle, fuzz, real hosts (`login.live.com`, `tls-v1-2.badssl.com`). See [TLS12.md](TLS12.md) |

## UI extras (docs/UI_EXTRAS.md)

The second wave of the launcher's UI: animated pictures, text selection and copy, the kit in the
accessibility tree, touch gestures and right to left, rich text and syntax highlighting, QR
codes, and localized times and sizes. Each has a test; the ones with a second implementation to
compare with are held against it.

| module | what it does | held against |
|---|---|---|
| `lib/qr` (`qr.fi`, `qrdec.fi`), `lib/fui/qrview.fi` | QR encoder (versions 1-40, L/M/Q/H, numeric/alphanumeric/byte, all masks, auto mask, level boost), decoder from a matrix or a picture (rotated, perspective, blurred, damaged), the widget | Nayuki's `qrcodegen`: module for module for every version/level/mask; python-qrcode (forced version and mask); ZXing-C++ reads every code and is the yardstick for the decoder (282/285, 349/355, 340/345 against its 284, 353, 340 on pictures that get worse) |
| `lib/i18n/human.fi` | "5 minutes ago", "12,3 MB", date and time styles, percent, lists, wall clock of a TZif zone, in en de fr it es pl cs ru | ICU 72.1 (PyICU): 8,016 random cases, 0 differences; zones against Python `zoneinfo` |
| `lib/highlight` (`highlight.fi`), `lib/fui/syntaxcolor.fi` | tokenizer interface and regex rules for JSON, TOML, INI, Firn, shell, Markdown, YAML, C-like; colours with 4.5:1 on the code ground | `tests/2242_highlight.fi` (every rule on sample text, the spans tile the text exactly on random input, linear time); the Markdown view's pixels |
| `lib/regex` `Matcher` | the machine of a regex kept between finds (10x faster tokenizing) | tests/1914 and 6,000 random patterns against Python `re` unchanged |
| `lib/fui/richtext.fi` | styled spans, wrapping, links, selection (drag, word, paragraph, Ctrl+A), copy, RTL paragraphs | `tools/fui/richtext_main.fi` (pixels) |
| `lib/fui/uianim.fi` | GIF / animated WebP playback: delays, loops, pause, clock jumps | Pillow's frames (CRC) and delays for four files; `tools/fui/anim_main.fi` |
| `lib/fui/kita11y.fi` | every kit part describes itself into the accessibility tree | `tools/fui/kita11y_main.fi`: the audit green, the dump compared |
| `lib/fui/kittouch.fi` | tap, double tap, long press, pan with fling, pinch on `lib/window/pointers.fi`'s records | `tools/fui/touch_main.fi` (synthetic streams, the grid and the Markdown view) |
| `kit.kit_set_rtl` | every kit part mirrored | `tools/fui/kitrtl_main.fi` (hit functions, decorations, keys) |


## Libraries for FleiLauncher (downloads and sign-in)

A launcher fetches thousands of files and signs the user in with a Microsoft
account. Details, tests and the honest limits: [DOWNLOAD.md](DOWNLOAD.md),
[OAUTH.md](OAUTH.md).

| module | what it does | held against |
|---|---|---|
| `net.download` (`lib/net/download.fi`) | a download manager: N workers on `std.pool`, a keep-alive connection per worker and host, resume with `Range`/`If-Range`, ETag/Last-Modified (`If-None-Match`, 304), retry with exponential backoff and jitter, `Retry-After`, mirrors, a global rate limit, MD5/SHA-1/SHA-256/SHA-512 while the data arrives, atomic rename, per-file and total progress (polled or callback), cancel, skip of files that are already right | a Python server that cuts connections, answers 503, ignores `Range`, sends a wrong `Content-Range`, corrupts bytes, stalls (99 checks, three build stages, Wine); 3000 files at ~350 files/s; the real Mojang CDN (40 assets by SHA-1, `client.jar` resumed from 5 MB) |
| `net.http` (additions) | extra request headers, user agent, a streaming body sink (no 32 MiB limit), `client_close`, errors as numbers | the download checks above |
| `auth.jose` (`lib/auth/jose.fi`) | JWT / JWKS: RS256/384/512, ES256/384, HS256; `kid` lookup; exp/nbf/iat/iss/aud/nonce; alg none and RS256-to-HS256 refused | 54 tokens signed by Python `cryptography` (`tests/2150`) |
| `auth.oauth` (`lib/auth/oauth.fi`) | OAuth 2.0 / OIDC client: discovery, device code flow, authorization code + PKCE with a loopback redirect server and the system browser, refresh, id_token check with the JWKS, userinfo, revocation, tokens in `std.secret` | a Python provider that checks PKCE, redirect URI, single-use codes, rotating refresh tokens and the polling interval (97 checks, three build stages, Wine); RFC 7636 appendix B |
| `auth.msa` (`lib/auth/msa.fi`) | Microsoft account -> Xbox Live -> XSTS -> Minecraft services -> ownership -> profile, XSTS error texts, session in the keyring, `msa_ensure` | a Python stand-in that checks every header and body (64 checks); the real hosts with bogus credentials (TLS 1.3 path proven); **no real login** (no client id here) |
| `appkit.fleitec_login` (`lib/appkit/fleitec_login.fi`) | "Sign in with Fleitec-ID": the five login answers and three `me` answers of docs/FLEITEC-ID.md, token in the keyring | `tests/2153` against an in-process ID server; not run against the real server |

## Compression formats (round COMPRESS, [COMPRESSION.md](COMPRESSION.md))

zstd, xz/LZMA2, Brotli, bzip2, LZ4 and a streaming gzip/zlib/DEFLATE inflater,
with one door (`compress.auto`: magic-octet detection, whole buffer and
streaming), and the two places that needed them: `.tar.zst`/`.tar.xz`/
`.tar.bz2`/`.tar.lz4` in `std.tar`/`std.extract` (Adoptium, Modrinth packs and
Linux packages) and `Content-Encoding: br`/`zstd` in `net.http`.

| module | what it does | held against |
|---|---|---|
| `compress.zstd`, `zstd_enc` | RFC 8878 in full incl. dictionaries and streaming; encoder levels 1-19 with Huffman literals, FSE tables, repeat offsets, dictionaries | libzstd (`zstd` command): corpus x levels 1-22/--long/small windows/dictionaries, both directions, every cut and damaged copy, a 64 MiB bomb under limits |
| `compress.lzma`, `lzma_enc` | LZMA2, `.xz` (CRC-32/64, SHA-256, filters delta/x86/PPC/IA-64/ARM/Thumb/SPARC/ARM64, several streams), `.lzma`; encoder presets 0-9 | liblzma (`xz`, Python `lzma`), both directions |
| `compress.brotli` | RFC 7932 in full incl. the static dictionary and transforms | libbrotli (Python `brotli`): qualities 0-11, windows 10-24 |
| `compress.bz2` | bzip2 (several streams, randomised blocks) | libbz2 (Python `bz2`) |
| `compress.lz4` | LZ4 blocks and frames (linked, checksums, dictionaries, skippable, legacy) and an encoder | liblz4 (Python `lz4`), both directions |
| `compress.gzip` | streaming gzip (several members)/zlib/DEFLATE | zlib/gzip |
| `compress.auto` | detection by content and one decompress/compress API | all of the above |

Every decoder takes a size limit and refuses with `TooLarge` before writing
past it; every format has a streaming reader whose memory is "window + one
unit". Linux, AArch64 (qemu) and Windows (Wine), all four build levels.
