# Firn platform map (read this FIRST)

RULE: Search here, then in `docs/LIBS.md` (generated, all 2340 modules) or `grep -r` in `lib/`, then the `search` tool (typ lib). Build a new lib ONLY if nothing fits. No duplicates: extend the existing module. After any `lib/` change run `bash tools/gen_libs_index.sh`.

Format: `area | entry file | purpose | call`. Import path = file path without `lib/` and `.fi` (`std/fs.fi` -> `import std.fs`). Exact signatures: header comment of the entry file or `docs/LIBS.md`.

## std (lib/std, 100 modules, topic docs: SKRIPT-LIBS.md, APPLICATION_LIBS.md)
- `std/io.fi` | file read/write, text | `io.read_file_text(path)`
- `std/fs.fi`, `std/fsx.fi`, `std/dir.fi`, `std/glob.fi`, `std/safefs.fi` | files, dirs, atomic writes, globbing
- `std/str.fi`, `std/text.fi`, `std/textfmt.fi`, `std/pystr.fi` | strings, formatting
- `std/vec.fi`, `std/map.fi`, `std/deque.fi`, `std/heap.fi`, `std/dict.fi`, `std/pycoll.fi`, `std/itertools.fi` | collections
- `std/json.fi`, `std/csv.fi`, `std/xml.fi`, `std/nbt.fi`, `std/base64.fi` | parse/emit formats
- `std/time.fi`, `std/log.fi`, `std/args.fi`, `std/cmdline.fi`, `std/env.fi`, `std/process.fi`, `std/shell.fi` | clocks, logging, CLI, spawn
- `std/hash.fi`, `std/md5.fi`, `std/tar.fi`, `std/extract.fi`, `std/stats.fi`, `std/mathx.fi`, `std/rng.fi` | hashes, archives, math
- `std/pool.fi`, `std/scope.fi`, `std/rc.fi`, `std/mem.fi` | worker pool, scopes, refcount, memory
- `std/dialog.fi`, `std/clipboard.fi`, `std/toast.fi` | native dialogs, clipboard, notifications (use via `services` in apps)
- `std/testkit.fi` | assertions for `#[test]`

## Crypto (lib/std/crypto)
- `sha256.fi sha512.fi sha1.fi blake2b.fi hmac.fi hkdf.fi argon2.fi aes.fi gcm.fi chacha.fi x25519.fi ed25519.fi p256.fi ecdsa.fi rsa.fi random.fi keyring.fi` | primitives | `sha256.sha256(ptr, len, out_ptr)`; secrets: `std/secret.fi`

## Net (lib/net, lib/tls, lib/ws, lib/http, lib/async)
- `net/http.fi` | blocking HTTP(S) client, redirects, gzip/br/zstd bodies
- `http/server.fi` | HTTP/1.1 server + WebSocket + SSE + TLS 1.3 | `http.app(port)`, `http.reply(r, 200, "text/plain", "hi")`
- `net/h2.fi`, `net/hpack.fi`, `net/h2frame.fi` | HTTP/2 client (multiplexing, flow control, ALPN h2; docs/HTTP2.md) | `h2.h2_request(&c, "GET", "https", host, "/", "", 0, 0)`
- `ws/ws.fi` | WebSocket codec + client (ws://, wss://)
- `tls/tls.fi` (client 1.3/1.2), `tls/tls_server.fi`, `tls/x509.fi`, `tls/trust.fi` | TLS, certs, system roots
- `net/dns.fi`, `net/dnsd.fi`, `net/addrinfo.fi`, `net/tcp.fi`, `net/udp.fi`, `net/idn.fi`, `net/ip6.fi` | DNS, sockets
- `async/loop.fi` | event loop (fds, timers) | `async/ahttp.fi` async HTTP, `async/adns.fi`, `async/wsc.fi`
- `mdns/`, `ssdp/`, `webdav/`, `bluetooth/`, `cast/` | discovery, WebDAV, BT, casting

## Data (lib/db, lib/parquet)
- `db/sqlite.fi` | embedded SQL DB, real SQLite 3 file format (SQL subset, docs/DB.md) | `sqlite.db_open("x.db", sqlite.OPEN_CREATE)`, `db_exec`, `db_prepare`, `stmt_step`
- `parquet/` | Parquet reader

## Compression (lib/compress, lib/zip) — docs/COMPRESSION.md
- `compress/auto.fi` | detect + decode any format, whole or streaming | `auto.decompress_limited(p, n, max, &out)`
- `compress/gzip.fi zstd.fi zstd_enc.fi lzma.fi lzma_enc.fi bz2.fi brotli.fi lz4.fi snappy.fi`, `zip/zip.fi`, `std/deflate.fi`

## Media
- `image/picture.fi` | Pillow-like facade (PNG/JPEG/WebP/GIF) | `picture.open("in.jpg")`; codecs `jpeg/`, `webp/`, `gif/`, `jxl/`, `svg/`
- `qr/qr.fi`, `qr/qrdec.fi`, `barcode/` | QR/barcode encode+decode
- `photo/` | camera pipeline: `pipeline.fi`, `merge.fi`, `night.fi`, `portrait.fi`, `focus.fi`, `sr.fi`, `camjob.fi`, `campreset.fi`
- `audio/player.fi` | playback | `player.play_mp3("App", ptr, len, 80)`; `audio/mixer.fi`, `audio/synth.fi`, `audio/hub.fi`, `ton/`
- `media/audio/` (wav flac mp3 ogg_vorbis opus aac), `media/video/vidsource.fi` (h264 vp8 vp9 av1), `media/mp4.fi`, `media/webm.fi`, `media/demux.fi`, `av1/` | decoders (docs/FORMATS.md, VIEWERS.md)
- `scene3d/` (docs/SCENE3D.md), `gpu/` (GL/Vulkan), `paint/`, `font/` | 3D, GPU, 2D paint, fonts

## Documents
- `pdf/pdf.fi` | PDF writer | `pdf.pdf_new(&out, pdf.pdf_options_default())`, `pdf_page_begin`
- `docview/` | viewers PDF/DOCX/ODT/XLSX/PPTX (docs/VIEWERS.md); `print/`, `markdown/`, `html/`, `css/`, `dom/`, `highlight/`

## UI (lib/fui, 280 modules; docs/fui-quickstart.md, fui-kit.md, fui-gap-analysis.md)
- `fui/app.fi` | window + app loop | `app.window("Title", 400, 250)`, `app.quit(a)`
- `fui/widget.fi`, `fui/listrow.fi`, `fui/editor.fi`, `fui/anim.fi`, `fui/a11y.fi`, `fui/bidi.fi`; `window/` (windows, shell thread), `input/`, `layout/`, `term/`

## App platform
- `appkit/` | config, log, crash, single_instance, update, catalog, texts, appinfo (docs/APPKIT.md) | `appkit/update.fi` signed self-update
- `services/` | ONE API for file open/save, dir pick, clipboard, dialog, store (docs/services-contract.md) | `services/svcfile.fi`, `svcdir.fi`, `svcclip.fi`, `svcdialog.fi`, `svcstore.fi`
- `plat/`, `@linux @windows @android @web`, `android/`, `desktop/` | platform layers; `NAME.<os>.fi` twins pick the OS
- `i18n/i18n.fi` | catalogs, plurals, date/number formats; `i18n/human.fi`; apps use `appkit/texts.fi` (`tr`, `trn`)
- `mail/mail.fi` | SMTP/IMAP | `mail.message(from, to, subj, body)`, `mail.send(host, port, &m)`, `mail.send_login`
- `auth/`, `fleitec_id/`, `fleitec_login/` | OAuth, Fleitec-ID login

## Concurrency
- `jobs/jobs.fi` | CPU thread pool | `jobs.parallel_for(j, begin, end, chunk, body, ctx)`; `jobs/pmap.fi` parallel map over files
- `std/pool.fi` | IO worker pool; `chan/` channels; `async/` event loop

## Toolchain and quality
- `tools/firn/firn` (new/add/fetch/update/build; docs/PACKAGES.md), `lib/toolchain/` (installer), `tools/pin/` (PIN-POLICY.md)
- Test: `firnc --test`, `lib/test/runner.fi`, `tools/testrunner/`; gate `bash test.sh --stage N` (tools/teststages/README.md)
- Coverage `tools/coverage/firncov` (docs/COVERAGE.md); lint `tools/lint/run.sh`; format `tools/fmt/premerge.sh`; LSP `firnc --lsp` (tools/lsp); debugger docs/DEBUGGER.md
- Language: `docs/FIR.md`, interfaces `docs/ROUND46.md`, tuples `docs/TUPLES.md`, aliases `docs/TYPE_ALIASES.md`, errors `docs/ERROR_UNIONS.md`, closures `docs/CLOSURES.md`

## Domain libs
- `mc/` (610 Minecraft modules; docs/MC-LIBS.md), `brigadier/`, `rvemu/`, `hf/`, `bpe/`, `cuda/`, `rc/`, `compile: firnc1/`

## Before you build
- `grep -ril "<keyword>" lib docs/LIBS.md | head`
- `search(query, typ:"lib", projekt:"firn")`
- Comparison table with gaps vs .NET: JARVIS archive "Firn-Plattform vs .NET/Rust/Go".
