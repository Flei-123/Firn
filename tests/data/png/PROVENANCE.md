# tests/data/png -- the test pictures of lib/paint/png.fi (PNG in)

Written on 4 October 2026 by `tools/libmvp/mk_imgdata.py` with the PNG writer of
`tools/libmvp/pngmake.py` (not by a PNG library): every colour type with the bit
depths listed, interlaced (Adam7) or not, random row filters, PLTE/tRNS. The
expected pixels (CRC-32 in `tests/2083_png.fi`) are computed from the raw samples
by `pngmake.expect`, so a decoder bug cannot hide in its own reference. Synthetic,
no third-party content.

| file | colour type / depth | extras |
|---|---|---|
| pal8_trns, pal4_i, pal2, pal1 | palette 8, 4, 2, 1 bit | tRNS alpha (partial), Adam7 for pal4_i |
| grey2, grey4_i, grey8_key, grey16 | grey 2, 4, 8, 16 bit | colour key (grey8_key), Adam7 for grey4_i |
| rgb8_key, rgb16 | RGB 8, 16 bit | colour key |
| la8, la16_i | grey + alpha 8, 16 bit | Adam7 for la16_i |
| rgba8_i, rgba16 | RGBA 8, 16 bit | Adam7 for rgba8_i |
