# Image and video formats: what exists, what is measured

Survey of 25 October 2026. `-` = no measured or documented value. Firn libs live in `lib/`, Certus libs in the Certus
worktrees (`/root/certus-*`, branch names in the last column). Tempo only where a script in the repo produced the number.

## Images

| Codec | Supported | Decode | Encode | Tempo | Evidence | Where |
|---|---|---|---|---|---|---|
| PNG | all colour types, 1..16 bit, tRNS, Adam7 | yes | yes (`write_png`, `png_encode`) | - | 185 generated + 307 Modrinth PNGs identical to Pillow (`docs/IMAGE_DECODERS.md`) | Firn `lib/paint/png.fi`, `lib/pack/icons.fi` |
| JPEG | baseline + progressive | yes | yes (`lib/photo/jpegenc.fi`, baseline) | decode: -, bench main exists in Certus (`jpeg_bench_main.fi`) | `tools/libmvp` checks | Firn `lib/jpeg/jpeg.fi`; Certus `lib/paint/jpeg.fi` |
| WebP | lossy VP8, lossless VP8L, ALPH, animation | yes | no | - | 991 files octet-identical to libwebp via Pillow, 40 muxed animations | Firn `lib/webp/webp.fi`; Certus `lib/paint/webp.fi` |
| GIF | 87a / 89a, animation | yes | no | - | compared with Pillow, browser rules where they differ | Firn `lib/gif/gif.fi` |
| AVIF | HEIF container, AV1 intra frame: 4:2:0 / mono, alpha, sb128, tiles, delta_q / delta_lf, qmatrix, palette, intrabc, irot / imir, deblock, CDEF, loop restoration | yes (still pictures, intra only) | no | - | bit-exact fixtures (`avif_420`, `sb128`, ...); `image/avif` Accept is default off (`--avif=1`) | Certus `lib/paint/avif*.fi`, `av1*.fi` (`w-r1720avif`) |
| JPEG XL | header only (jxl_bits, jxl_container, jxl_header, jxl.jxl_info) | planned: Modular lossless, VarDCT | no | - | - | Firn `lib/jxl` (entropy modules on branch staff/firn-mvo7ma20, untested) |
| HEIC / HEIF (HEVC) | none | no | no | - | - | - |
| BMP, ICO, TGA, TIFF | none (ICO / ICNS are only written by `pack.icons`) | no | partly | - | - | - |

Picture limit in the Firn decoders: 16,777,216 pixels by default, checked before memory is requested.

## Video and audio containers

| Codec | Supported | Decode | Encode | Tempo | Evidence | Where |
|---|---|---|---|---|---|---|
| H.264 / AVC | baseline, main, high; CAVLC + CABAC; I / P / B; 8x8 transform; weighted prediction; multi-slice; progressive 4:2:0 8 bit. Not: interlaced, FMO, 4:2:2 / 4:4:4, > 8 bit | yes | no | 720p 4 Mbit/s: 33.6 (weightp) / 32.2 (plain) frames/s, one thread, shared host (`docs/H264-W6-0510.md`) | 44 checks against ffmpeg frame by frame (`tools/media/h264_test.sh`) | Certus `lib/media/h264.fi` |
| MP4 / fMP4 | sample tables, fragments | demux | no | - | `tools/media/mp4_test.sh` | Certus `lib/media/mp4.fi` |
| WebM / Matroska | EBML, tracks (VP8 / VP9 / AV1 / Opus / Vorbis ids), clusters | demux only | no | - | tests in branch | Certus `lib/media/webm.fi`, `ebml.fi` (`w-r135webm`) |
| VP8 / VP9 | none (plan only: `docs/VP9-OPUS-PLAN-1610.md`) | no | no | - | - | - |
| AV1 (video) | only the intra frame path used by AVIF | still pictures only | no | - | see AVIF | Certus `lib/paint/av1*.fi`; another worker builds a Firn AV1 lib |
| HEVC / H.265 | none | no | no | - | licence note below | - |
| Opus | Ogg Opus, CELT (bit-exact against libopus), SILK, hybrid, redundancy | yes | no | - | `tools/media/opus_test.sh`, celt / silk bit-exact checks | Certus `lib/media/opus*.fi`, `celt*.fi`, `silk*.fi` (`w-r146silk`) |
| AAC-LC | no SBR / PS / LTP | yes | no | - | `tools/media/aac_test.sh` | Certus `lib/media/aac.fi` |
| MP3 | - | yes | no | - | `tools/media/mp3_test.sh` | Certus `lib/media/mp3.fi`; Firn `lib/ton/mp3.fi` |
| Vorbis, FLAC | - | yes | no | - | `vorbis_main.fi`, `flac_check.py` | Certus `lib/media/vorbis.fi`, `flac.fi` |

## HEVC: licence / patent short view

- HEVC is covered by several patent pools (Access Advance, Via LA) and some single licensors (Velos, others); no single royalty-free grant exists.
- Decoder-only software is not exempt: licensors ask for royalties on end products that contain a decoder, also for software.
- AV1, VP9, Opus, JPEG XL are royalty-free by their own grants; H.264 is already in Certus under the same caveat (MPEG LA pool).
- Judgement: not defensible for a free-licensed Firn lib. Do not build HEVC / HEIC. Ship AVIF (AV1) and JPEG XL as the modern image path.
- This is an engineering assessment, not legal advice; a lawyer has to confirm before any shipping decision.

## Reference decoders on this host

- `ffmpeg` with `libjxl` 0.7 (encoder + decoder), libaom, dav1d, libwebp, libx264, libx265.
- `djxl` / `cjxl` are not installed (`apt-cache policy libjxl-tools`: candidate 0.7.0-10+deb12u1, not installed). `pillow-jxl-plugin` is on PyPI.
