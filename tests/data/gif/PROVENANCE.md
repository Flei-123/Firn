# tests/data/gif -- the test pictures of lib/gif

Made on 4 October 2026 with Pillow 12.3 by `tools/libmvp/mk_imgdata.py` from
synthetic pictures (random ellipses and a pixel pattern, seed 7). No
third-party content.

| file | what it exercises |
|---|---|
| pal2.gif, pal4.gif, pal16.gif, pal256.gif | colour tables of 2 .. 256 entries (LZW minimum code size 2 .. 8) |
| inter.gif | interlaced; decodes to the same pixels as pal16.gif |
| transp.gif | a transparent index |
| s1x1.gif | one pixel |
| noise.gif | incompressible: the LZW code table fills up to 4096 |
| anim.gif | 5 frames, delays 50/100/70/20/0 ms, endless loop |
| disp.gif | transparent RGBA frames with disposal 1, 2, 3, loop count 2 |

`tests/2081_gif.fi` compares the CRC-32 of the decoded RGBA with the CRC-32 of
Pillow's RGBA for the same file. `tools/libmvp/check_gif.py` compares every
octet on a larger corpus including hand-written LZW streams.
