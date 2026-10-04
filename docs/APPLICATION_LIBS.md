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
| `std.process` (`lib/std/process.fi`, `process.windows.fi`) | start a program with arguments, working directory, changed environment; stdin/stdout/stderr inherited, piped or discarded; blocking and non-blocking reads; wait (with timeout), try_wait, exit code, kill/terminate, detach, `no_window`; `run_capture`/`run_io` collect both outputs while feeding input without deadlock; every failure is a `ProcError` | 3 MiB both ways through a child, 300 KB on stdout+stderr, 300 starts without a leak (`tests/2062`-`2064`, Linux, AArch64, Wine) |
| `std.shell` (`lib/std/shell.fi`) | `open_url`, `open_path`, `reveal_in_file_manager`, `find_in_path`, home/config/data/cache/temp/Minecraft directories, `find_java`/`java_candidates`/`java_major_version` | a fake `xdg-open` and fake `java` scripts (`tests/2061`) |
| `std.env` (`lib/std/env.fi`) | read the environment, `find_executable` (`which`) | the kernel's `/proc/self/environ`, `sh` on PATH |
| `std.pool` (`lib/std/pool.fi`) | fixed worker threads, bounded job queue, results; Linux only (Windows runs jobs inline) | `tests/2065`: 5000 jobs once each, parallel speed-up, GC on workers |
| `std.cmdline` (`lib/std/cmdline.fi`) | Windows command line quoting/splitting by the rules of the C runtime, UTF-8 <-> UTF-16 | the published examples + round trip (`tests/2060`) |
| FLEILAUNCHER | `net.dns` (`lib/net/dns.fi`), `net.udp`, `net.dnsconf`, `tls.trust`; `net.http` now fetches `https://host/...` | DNS by name: A/AAAA, CNAME chains, UDP with retry and a doubling timeout, TCP fallback on TC, random transaction ID, answers accepted only from the server asked, TTL cache, hosts file, resolv.conf (Linux) / GetNetworkParams (Windows); `https://` through the TLS 1.3 client with the system's roots (PEM bundle on Linux, the certificate store on Windows) and the certificate checked against the NAME; secp256r1 added next to X25519 because Azure Front Door (`piston-meta.mojang.com`) refuses X25519-only. See [DNS.md](DNS.md) | a fake DNS server in Python (the server's log is counted), real answers of 1.1.1.1 decoded by an independent Python reader, `getaddrinfo`/`dig`/`curl` on the real network, a hermetic Python TLS server with a test CA (`tools/dns/run.sh`, also the Windows build under Wine) |

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
| `std.tar` (`lib/std/tar.fi`) | tar reader (ustar, V7, GNU long names, pax), tar writer, `tar_add_tree`, tar.gz with a size limit on the inflated stream, safe extraction with mode bits | GNU tar 1.34 and Python `tarfile`, both directions; fourteen hostile archives |
| `std.extract` (`lib/std/extract.fi`) | `extract_archive(path, dest)`: .zip, .tar.gz/.tgz, .tar found by content, `strip` for the Adoptium top directory, modes (0755 for `bin/java`) | Info-ZIP, `zipfile`, GNU tar |
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

