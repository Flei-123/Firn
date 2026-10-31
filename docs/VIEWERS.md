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
