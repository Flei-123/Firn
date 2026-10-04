# tests/data/img -- one picture in four containers

`same.png` (colour type 2), `same.webp` (lossless) and `same.gif` are the same
opaque 61 x 47 picture (pal16.gif of `tests/data/gif`, 16 colours); decoded they
are identical, which `tests/2082_image_from_bytes.fi` checks. `rgba.png` is a
24 x 16 RGBA picture, `palette.png` a palette PNG (Modrinth's icons are mostly these). Made by `tools/libmvp/mk_imgdata.py`,
synthetic, no third-party content.
