# VIEWERS -- in-app file viewing and playback (no download)

Goal (Boss, 2026-10-31): Orbit / FirnChat (Firn + fUi, WASM in the browser and native) shows and plays files in place:
PDF, DOCX (later XLSX/PPTX/ODT), images, audio (MP3/Opus/AAC/FLAC/WAV/Vorbis), video, text/code with highlighting, Markdown.
Everything is built as reusable Firn libs first; Orbit integration comes after.

## Layers

| Layer | Dir | Rule |
|---|---|---|
| Sniff + registry | `lib/docview/kind.fi`, `lib/docview/registry.fi` | magic/extension -> `Kind`; one registry of openers, no special cases in callers |
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

Checks: `tools/media_video/run_h264.sh` (10 x264 clips vs `ffmpeg -f rawvideo`, bit exact), `run_demux.sh` (packets/pts/flags vs ffprobe), `fuzz.py` (mutated/truncated MP4/WebM/H.264), tests `6410_media_video_demux`, `6411_media_video_source`.
Numbers: 720p high-profile decode 79.8 M instructions/frame (callgrind, release-fast); wasm32-browser VideoSource+demux+h264: 221 KB release-fast, 795 KB release-safe.
