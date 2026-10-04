# Image decoders: WebP, GIF, and one call for all four formats

Written 4 October 2026 for the FleiLauncher (Minecraft launcher in Firn/fUi):
Modrinth serves mod icons as WebP (most), PNG and GIF; screenshots are PNG.

| module | formats | API |
|---|---|---|
| `lib/jpeg/jpeg.fi` | JPEG | `jpeg_decode(p, n, &im)` |
| `lib/paint/png.fi` | PNG (grey/RGB/grey+alpha/RGBA, 8 bit, no interlace) | `decode_png` |
| `lib/webp/webp.fi` | WebP lossy, lossless, alpha, animated | `webp_decode`, `webp_decode_anim`, `webp_info` |
| `lib/gif/gif.fi` | GIF87a, GIF89a | `gif_decode`, `gif_decode_anim`, `gif_info` |
| `lib/fui/uiimagedec.fi` | all four | `image_from_bytes(p, n, &decoded, &view)` |

All of them give STRAIGHT RGBA octets, row 0 on top, `w * h * 4` in an
`rt.Buf`; fUi's `uiimage.image_wrap` takes that as it is.

```
import fui.uiimage
import fui.uiimagedec

var d: uiimagedec.Decoded = uiimagedec.decoded_new()
var view: uiimage.Image = uiimage.image_new()
if uiimagedec.image_from_bytes(bytes, n, &d, &view) {
    uiimage.image_draw(&view, ...)      // while `d` lives
}
uiimagedec.decoded_free(&d)
```

`uiimage.image_format(p, n)` is the pure magic-octet check (PNG, JPEG, WebP,
GIF, unknown) and works in the kernel-clean half of fUi.

## Animations

`webp_decode` / `gif_decode` return the first frame (for WebP: on the canvas,
as libwebp's WebPAnimDecoder shows it). `webp_decode_anim` / `gif_decode_anim`
return every frame as a full canvas (`an.px`, `an.count` canvases of
`an.w * an.h * 4` octets), the delay of frame `i` in milliseconds
(`webp_anim_delay(&an, i)`, `gif_anim_delay`) and the loop count (`an.loops`,
0 = forever). A GIF delay is reported as the file says (hundredths of a second
times ten); players show anything under 20 ms as 100 ms, the decoder does not.

## Hostile input

* The picture size is read from the header and checked against the limit
  (default 16,777,216 pixels; `*_decode_limited` takes another) BEFORE any
  pixel memory is requested. An animation is limited to 256 MiB of canvases.
* Cut or damaged files give `WebpError::Corrupt` / `GifError::Corrupt` and an
  empty picture. There is no partial picture, except a GIF animation, whose
  complete frames before the damage are kept.
* Held by: every prefix of test files, thousands of damaged copies (bit flips,
  0x00/0xFF octets) in `tests/2080..2082` and `tools/libmvp/fuzz_img.py`, the
  latter with `release-safe` builds (an arithmetic overflow is a trap there).

## Held against Pillow

`tools/libmvp/run.sh` (section 65 of `test.sh`): `check_webp.py` and
`check_gif.py` compare every octet with Pillow (libwebp / libgif). WebP: 991
files, lossless and lossy identical, no tolerance, 40 animations muxed by hand
with random offsets and blend/dispose flags compared frame by frame. The
Modrinth icons are downloaded for the run (`fetch_modrinth.py`), never
committed; offline they are skipped. GIF: where Pillow and a browser differ
(disposal 2 of a frame without transparency, the colour of transparent
pixels after the first frame) the decoder does what a browser does; the list
is at the top of `check_gif.py`.

## Not done / honest

* `lib/paint/png.fi` takes neither palette PNGs, 16-bit samples nor Adam7
  interlacing; many Modrinth icons are palette PNGs. A full PNG decoder is on
  the roadmap.
* The colour profile (ICC) of a WebP is ignored; GIF plain-text and
  application extensions other than NETSCAPE2.0 are skipped.
* WebP lossy decoding is libwebp's default ("fancy" upsampling, no dithering).
* The first call of the WebP decoder builds two global lookup tables;
  `webp_init()` does it up front (relevant only if several threads would
  race on the very first picture; the writes are idempotent).
