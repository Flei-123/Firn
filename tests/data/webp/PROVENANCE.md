# tests/data/webp -- the test pictures of lib/webp

Made on 4 October 2026 with Pillow 12.3 (libwebp) by `tools/libmvp/mk_imgdata.py`
from synthetic pictures (random ellipses and a pixel pattern, seed 7). No
third-party content, except the two logos of FleiLauncher, Justin's own launcher
(`logo-blue.webp`, `logo-green.webp`, copied unchanged from its `public/`).

| file | what it exercises |
|---|---|
| ll_rgb.webp, ll_rgba.webp | lossless, with and without alpha |
| ll_pal.webp | lossless with 13 colours (colour-indexing transform) |
| lossy_q75.webp, lossy_q5.webp | VP8 at quality 75 and 5 (loop filter, dequantisation extremes) |
| lossy_alpha.webp, lossy_alpha_exact.webp | VP8 + ALPH (compressed alpha; `exact` keeps the colour of transparent pixels) |
| lossy_1x1.webp, lossy_33x2.webp | smallest and thin pictures (macroblock padding, upsampling edges) |
| logo-blue.webp, logo-green.webp | real files: FleiLauncher's logos, 128 x 128 |
| anim_ll.webp, anim_lossy.webp | animations (6 frames, delays, loop count, sub-rectangle frames) |

`tests/2080_webp.fi` compares the CRC-32 of the decoded RGBA with the CRC-32 of
Pillow's RGBA for the same file. `tools/libmvp/check_webp.py` compares every
octet on a much larger corpus and on the Modrinth icons (downloaded, never
committed).
