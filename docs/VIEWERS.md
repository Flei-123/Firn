# VIEWERS -- in-app file viewing and playback (no download)

Goal (Boss, 2026-10-31): Orbit / FirnChat (Firn + fUi, WASM in the browser and native) shows and plays files in place:
PDF, DOCX (later XLSX/PPTX/ODT), images, audio (MP3/Opus/AAC/FLAC/WAV/Vorbis), video, text/code with highlighting, Markdown.
Everything is built as reusable Firn libs first; Orbit integration comes after.

## Layers

| Layer | Dir | Rule |
|---|---|---|
| Sniff + registry | `lib/docview/kind.fi` (+ `kinds`, `magic`, `container`, `dvbytes`), `lib/docview/registry.fi` | magic/extension -> `Kind`; one registry of openers, no special cases in callers |
| Document model | `lib/docview/<format>/` | parse only, no UI; `Doc` handle: `doc_open(bytes) -> Doc`, `doc_pages`, `doc_page_size`, `doc_render(page, scale, out_rgba)`, `doc_text(page)`, `doc_free` |
| Media model | `lib/media/` | `AudioSource`: `probe`, `read_f32(frames)`, `seek`, `duration`; `VideoSource`: `probe`, `next_frame -> yuv/rgba + pts`, `seek`; A/V clock in `lib/media/clock.fi` |
| Widgets | `lib/fui/kitdoc*.fi`, `lib/fui/kitplayer*.fi`, `lib/fui/kitcode*.fi` | fUi widgets over the models only (page strip, zoom, search, seek bar, volume) |
| Lazy loading | `lib/docview/lazy.fi` | codecs/parsers registered by name, pulled in on first use (WASM size) |

All libs: no native-only calls in model layers, must build for `wasm32-browser`, one small module per concern, English only, no comments.

## Inventory (2026-10-31)

| Need | Where it exists | State |
|---|---|---|
| PDF read/render | OrientOS `lib/pdfread/*` (5.0k lines, own SVG painter via lib/svg, checked against poppler: serif.pdf 0.87 gray distance); Certus `lib/pdf/*` (13.5k lines + 15k font data: ppage/pfont/prast/pfill/pform/pannot/filter/obj/doc) | not in Firn. Firn `lib/pdf` is a WRITER only. OrientOS: no embedded fonts, no shading/pattern, no JBIG2/JPX/CCITT, no links. |
| PDF text/search | OrientOS `pdftext.fi` | not in Firn |
| DOCX/ODT/XLSX/PPTX | nothing | `lib/zip` + `std.xml` exist; no OPC/WordprocessingML model, no flow layout for rich text |
| Markdown | Firn `lib/markdown` + `fui/kitmd*` | done |
| Code highlight | Firn `lib/highlight` (16 langs) | done; more langs possible |
| Images | Firn `image.picture` (PNG/JPEG/WebP/GIF), `jxl`, `av1/avif`, `svg` | done; BMP/ICO/TIFF missing |
| MP3 | Firn `ton/mp3.fi` (gapless, bit exact tests) AND Certus `media/mp3.fi` | duplicate; keep Firn |
| Vorbis/Ogg | Firn `ton/vorbis*.fi`, `ton/ogg.fi` AND Certus `media/vorbis.fi`, `ogg.fi` | duplicate; keep Firn |
| FLAC, AAC, Opus (SILK+CELT), WAV | Certus `media/flac.fi`, `aac.fi`, `opus*.fi`/`celt*`/`silk*`, `player.fi` (WAV) | not in Firn; Firn `audio.engine` has `engine_load_wav` only |
| MP4 / WebM / EBML demux | Certus `media/mp4.fi`, `webm.fi`, `ebml.fi`, `webm_ogg.fi` | not in Firn |
| H.264 | Certus `media/h264.fi` (9.2k lines, CAVLC+CABAC, I/P/B, no interlace); Firn `cast/h264dec.fi` (244 lines) | Certus one is the real decoder |
| VP8/VP9 | none (demux detects the codec only) | GAP |
| AV1 video | Firn `av1` = still image (AVIF) only | GAP (inter frames exist in tables: av1interm; check) |
| A/V sync + audio out | Certus `media/player.fi` (audio engine), `audio_*.fi` backends; Firn `audio.sink`, `audio.player` | unify on `audio.sink` |
| WASM audio/video output | Firn `plat/web` | needs WebAudio sink + canvas blit check |

## Gaps list (to fill)

1. G1 `lib/docview` core: sniff, registry, `Doc` API, lazy table.
2. G2 PDF: lift best of OrientOS pdfread / Certus pdf into `lib/docview/pdf`; embedded fonts, links, text extraction, search.
3. G3 Office: OPC container, DOCX model + flow layout (headings, runs, lists, tables, images); XLSX/PPTX/ODT later.
4. G4 Audio: FLAC, AAC (m4a), Opus, WAV, unified `AudioSource` over Firn mp3/vorbis.
5. G5 Video: MP4/WebM demux, H.264 lifted from Certus, YUV->RGBA; VP8/VP9/AV1 as gaps.
6. G6 Widgets: kitdoc (paged view), kitplayer, kitcode (highlighted text view), sniffed dispatcher widget.
7. G7 WASM: size budget per lib, lazy loading, WebAudio sink.
8. G8 Orbit integration (separate project "Fleitec All-in-One"): chat preview + fullscreen viewer.

## Reference checks

* PDF: `pdftoppm -r 72` PSNR/mean abs diff per page, `pdftotext` text equality.
* DOCX: no LibreOffice on the server; text extraction vs python zipfile/xml, structure counts; render check by eye only.
* Audio: `ffmpeg -f f32le` PCM vs ours (max abs diff / SNR), duration vs `ffprobe`.
* Video: `ffmpeg -f rawvideo` frames vs ours (bit exact for H.264).
* Fuzz: mutated/truncated files must never crash or hang (error code only).

## G5 Video (2026-10-31)

| Module | Role |
|---|---|
| `lib/media/mp4.fi`, `webm.fi`, `ebml.fi` | container parsers (classic + fragmented MP4, WebM/Matroska) lifted from Certus, MPL-2.0 |
| `lib/media/demux.fi` | one track API over both: `demux_open`, `demux_find_track`, `demux_track_*`, `demux_nsamples/off/size/pts_us/dts_us/sync`, `demux_seek_sync`, `demux_track_skip_us`; pts in microseconds, ffmpeg-style (edit list / Opus delay applied, audio may start negative); sample ranges outside the file read as size 0 |
| `lib/media/video/source.fi` | `VideoSource`: `vs_open`, `vs_width/height/fps_milli/duration_us/codec`, `vs_next_frame(out_rgba, &pts)`, `vs_seek` (keyframe), `vs_close`; unsupported codecs open with `VS_ERR_UNSUPPORTED` but still probe |
| `lib/media/video/convert.fi` | scalar YUV420 -> RGBA (BT.601 / BT.709 by stream matrix) |
| `lib/media/video/sync.fi` | pure model `sync_decide(frame_pts, clock_us, early, late)` -> SHOW / WAIT / DROP, driven by the audio clock (`lib/media/clock.fi`) |
| `lib/media/video/h264/*.fi` | H.264 decoder (CAVLC+CABAC, I/P/B, 8x8, weighted, direct, multi-slice, cropping; no interlace) split into 28 modules; public API in `params` (`H264`, `h264_new`), `nal` (`h264_config_avcc/decode/flush/output/reset`), `pics` (`h264_free`), `types` (`Frame`) |

Division with `lib/cast/h264dec.fi` (244 lines): the cast decoder is the tiny baseline-only decoder paired with `cast/h264enc` (RTP/RTSP round trip tests). It stays; real files (any profile, B-frames) go through `media.video.source`. Pointing cast at the lifted decoder is a roadmap item.

Lifted from Certus `lib/media/h264.fi` (9.2k lines): the helper threads (deblock/recon, futex), the breath hook and `thr.*` were removed (single thread, no platform dependency), so it builds for `wasm32-browser`; comments stripped; code tables in `h264/tables.fi`.

Checks: `tools/media_video/run_h264.sh` (10 x264 clips vs `ffmpeg -f rawvideo`, bit exact), `run_demux.sh` (packets/pts/flags vs ffprobe), `fuzz.py` (mutated/truncated MP4/WebM/H.264), tests `6410_media_video_demux`, `6411_media_video_source`. Fuzz: 30000 tool runs (seeds 21,42,77,901-903 x 2500), 0 panics, 0 hangs; use `+%`/`-%`/`*%` and clamped `ebml_u32`/`ebml_i64` for untrusted sizes.
Numbers: 720p high-profile decode 79.8 M instructions/frame (callgrind, release-fast); wasm32-browser VideoSource+demux+h264: 221 KB release-fast, 795 KB release-safe.
## G2 PDF viewer (lib/docview/pdf)
- Base: Certus lib/pdf renderer (chosen by measurement vs OrientOS pdfread, 36 comparable corpus files, pdftoppm -r 72 page 1: median MAD 5.36 vs 6.66, PSNR 19.9 vs 18.4). Fixed horizontal-edge bug in prast: mean MAD 12.89 -> 12.48, mean PSNR 20.25 -> 21.31 (OrientOS 16.77 / 19.42).
- Modules (flat unique names, module name = last path segment): pobj, pdoc (parser/model), filter, prast, pfont, ppage (render+text), pannot, pform, textfold, pdf (facade), opener (docview registry).
- API (pdf.fi): pdf_new, pdf_open, pdf_free, pdf_error, pdf_pages, pdf_page_size, pdf_render (+pdf_pixels/width/height, RGBA opaque), pdf_text (UTF-8), pdf_find (count + u32 codepoint offsets), pdf_links (rects in page space, origin top-left, URI).
- Registry: `opener.pdf_install_lazy()` registers kind KIND_PDF as "pdf.native" (lazy). Not part of install_builtin on purpose: static font data (364 KB) is only linked by apps that call it.
- Checks: tests/docview_pdf_basic.fi, tests/docview_pdf_registry.fi; tools/docview/pdf_compare.py (pdftoppm), pdf_textcmp.py (pdftotext word match page 1: 97.56 % over 27 files, 8291 words), pdf_fuzz.py (2400 mutated/truncated runs, 0 crashes, 0 hangs).
- Size: wasm32-browser probe 1 096 010 B, of which 364 KB built-in font data (11 substitute faces). Lazy font loading needs a font hook (open).
- Gaps: embedded fonts (FontFile2/CIDToGIDMap; cr-artikel text match 22 %), encryption (RC4/AES, empty user password), SMask in ExtGState, CCITTFax, glyph boxes for search highlight, /Rotate in links, named destinations.
## Status G1 + G6 (2026-10-31)

Module names are unique on purpose: Firn resolves a module by its LAST path segment, so `docview.dvdoc` (not `doc`, clashes with `markdown.doc`), `dvbytes` (`std.bytes`), `dvinstall` (`pack.install`), `dvstrip` (`fui.layout`).
`image.picture` and `fui.*` cannot be imported into one program (both pull a module named `icons`); the image opener therefore uses `fui.uiimagedec` and `docview.resample`.

### Core (`lib/docview`)

| Module | Role |
|---|---|
| `kinds` | `KIND_*` constants (pdf, docx, xlsx, pptx, odt, zip, png, jpeg, webp, gif, jxl, avif, svg, mp3, ogg, opus, flac, wav, m4a, mp4, webm, markdown, text, code), `kind_name`, `kind_family` |
| `kind` | `sniff(p, n, name) -> Kind` (magic first, zip entries for Office/ODT, svg, then text by extension: markdown / code via `highlight.hl_language` / text), `code_language(name)` |
| `registry` | `register(name, kind, DocOps) -> bool` (same name replaces), `unregister`, `find_by_kind` (last registration wins), `find_by_name`, `has_kind`, `registry_reset`; `DocOps` = `open(p, n) -> handle`, `pages`, `page_size`, `render`, `text`, `free` (start from `ops_empty()`) |
| `lazy` | `lazy_register(name, kind, init)`, `lazy_ensure_kind`, `lazy_ensure_name`; the init function calls `registry.register` on first use only |
| `dvdoc` | `Doc`, `doc_open(p, n)`, `doc_open_named(p, n, name)`, `doc_open_kind`, `doc_pages`, `doc_page_size(page, &w, &h)`, `doc_render(page, scale, &Buf rgba, &w, &h)`, `doc_text(page, &Buf)`, `doc_free`, `doc_is_flow` |
| `dvinstall` | `install_builtin()` registers lazy openers: text, code, markdown (flow docs, 1 page, text = source) and png/jpeg/webp/gif (1 page, bilinear scaled RGBA) |
| `dvstrip`, `search`, `pagecache` | page strip layout (zoom, fit width, visible range), text search over `doc_text`, LRU render cache |

Opener contract for other workers: `registry.register("pdf.main", kinds.KIND_PDF, ops)` from an init function and `lazy.lazy_register("pdf.main", kinds.KIND_PDF, init)`; `doc_render` writes straight RGBA (`uiimage` pixel layout) into the Buf; failures return `false`/0, never crash.

### Widgets (`lib/fui`)

* `kitdoc`: `DocView` page strip (wheel, zoom in/out/set, fit width, next/prev/goto page, search with hit marker bar, status text, lazy visible-page render through the cache).
* `kitcode`: `CodeView` highlighted text with line numbers (visible lines only, mono face from font slot 1).
* `kitview`: `View` dispatcher bytes + name -> pages (kitdoc), code/text (kitcode), markdown (`fui.markdownview`), media placeholder, unsupported placeholder.

Tests: `tests/6400`-`6403_docview_*.fi`; `wasm32-browser` builds of `kitview` and the core pass.

## G4 audio status (2026-10-31)

API: `lib/media/audio/source.fi` (`AudioSource`: `source_open`, `source_info`, `source_read_f32`, `source_seek`, `source_seek_ms`, `source_close`), `sniff.fi` (magic-byte sniffer), `open.fi` (`open_bytes`), `all.fi` (`register_all`; apps import single codecs to keep WASM small), clock `lib/media/clock.fi`.
Codecs done: WAV (u8/s16/s24/s32/f32/f64/A-law/mu-law), MP3 (`mpeg.fi`, gapless Xing/LAME, seek), Ogg Vorbis (`ogg_vorbis.fi`), FLAC (`flac.fi` over `flac_dec.fi`, lifted from Certus). Module names avoid the basenames `mp3`/`vorbis` (an import of `ton.mp3` resolves wrongly next to a module of the same basename).
Cross-check: `python3 tools/media/audio_cross.py` (ffmpeg f32le: WAV/FLAC bit-exact, MP3 SNR ~79-83 dB with identical length, Vorbis maxdiff 1e-7, seek, fuzz 2000 runs without crash or hang).
WASM (wasm32-browser, probe `tools/media/audio_wasm_probe.fi`, whole module): wav 25.7 KB, flac 45.7 KB, ogg_vorbis 122.9 KB. MP3 builds for wasm32-browser: the SIMD kernels live in `ton/mp3_vec.fi`; the web overlay `lib/@web/ton/mp3_vec.fi` is GENERATED (`python3 tools/media/gen_mp3_soft.py`) from it onto the software lanes `ton/v4soft.fi`. Output is bit-identical to native (mp3/aac CRC under node = native, `tools/media/audio_crc.fi`). Size mpeg: 90.3 KB release-fast / 148.7 KB release-safe. Regenerate the overlay after every change to `ton/mp3_vec.fi`.
G4b (Opus + AAC): Opus (SILK+CELT, lifted from Certus, 29 modules in `lib/media/audio/opus/`) decodes Ogg-Opus and WebM-Opus through `audio.opus_src` (`audio.ogg_index` packet index, `audio.pktplay` generic packet player with preroll seek, webm DiscardPadding trims the end). Against ffmpeg `-c:a libopus`: SNR 66-71 dB (i16 fixed-point vs float), same length, seek ok (200 ms preroll), fuzz 2400 runs 0 crashes. wasm32-browser: opus 130 KB release-fast / 354 KB release-safe, aac 73 KB / 186 KB. `mp4a` channels/rate now come from the ASC.
G4c: AAC-LC (`lib/media/audio/aac/`: aac_state, aac_bits, aac_huff, aac_win, aac_init, aac_sect, aac_spec, aac_dequant, aac_stereo, aac_tns, aac_fb, aac_frame, aac_tables; `audio.aac_src` M4A via demux and ADTS) is registered in `register_all`. Fixes: PNS noise uses ffmpeg's LCG (state 0x1f2e3d4c, `*1664525+1013904223`, clip -100..155), M4A priming lead is rounded. Against ffmpeg `aac`: SNR 79-83 dB (decoder output is i16), same length for M4A and ADTS, seek exact without PNS, fuzz 2400 runs 0 crashes. wasm32-browser output equals native. `audio.sinkplay` (`sp_open/sp_start|sp_attach/sp_pump/sp_pause/sp_seek_ms/sp_position_ms`) pumps an AudioSource into `audio.sink` (S16, 1-2 channels) and drives `media.clock` (latency from the sink's free space); test 6412 (file sink). Real PulseAudio playback not tested on the server (no sound daemon).
Open: AAC first-frame bug + M4A length, audio.sink-Play with `media.clock`, MP3 scalar path for wasm, kitplayer widget.
