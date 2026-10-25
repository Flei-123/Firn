# JPEG XL decoder (`lib/jxl`): plan and format facts

Status 25 October 2026: NOT built yet. This file is the verified groundwork for the first build.

## Reference tooling (host)

- `djxl` / `cjxl` are not installed. Reference encoder / decoder: Python venv `/root/jarvis/data/staff_wt/firn/_jxl_ref/v` with `pillow` + `pillow-jxl-plugin` (PyPI wheel, libjxl inside): `Image.save(buf, "JXL", lossless=True, effort=N)`, then `Image.open`. A bare codestream starts `ff 0a`.
- `ffmpeg` has `libjxl` 0.7 (`-c:v libjxl -distance 0`), `libjxl.so.0.7` is in `/usr/lib/x86_64-linux-gnu`.
- libjxl 0.8.2 source for reading the format (BSD-3, read-only reference, do not copy code): `/root/jarvis/data/staff_wt/firn/_jxl_ref/src/libjxl-0.8.2/lib/jxl` (`image_metadata.cc`, `frame_header.cc`, `dec_ans.cc`, `dec_huffman.cc`, `modular/encoding/*`, `modular/transform/*`).

## Build order

1. `jxl_bits` (LSB-first bit reader, U32 / U64 / F16 / Enum), `jxl_container` (bare `ff 0a` or ISOBMFF `JXL ` + `jxlc` / `jxlp`), `jxl_header` (SizeHeader, ImageMetadata, CustomTransformData, ICC skip).
2. `jxl_entropy`: ANS (alias table, 12-bit table, state init 32 bits, renormalise below 2^16, final state `0x130000`), prefix codes (canonical, Brotli style), hybrid uint (split_exponent, msb / lsb in token), LZ77 (window 2^20, 120 special distances), context map (simple / entropy coded, optional inverse MTF).
3. `jxl_frame`: frame header, TOC (non-permuted first), LfGlobal, group sections.
4. `jxl_modular`: MA tree, 14 predictors incl. weighted predictor, properties, transforms RCT / palette / squeeze. Lossless RGB, grey, alpha first; checked against the original pixels.
5. VarDCT: LF, HF, quantiser, DCT variants, XYB, EPF, gaborish, noise, patches, splines. Bit-exactness against libjxl needs the same float order; decide the tolerance (djxl output is 8 / 16 bit, so compare after rounding).
6. Later: encoder (modular lossless first), JPEG recompression (`jbrd` box).

## Verified header facts (from libjxl 0.8.2)

- SizeHeader: `small` bool; small: `ysize_div8_minus_1` 5 bits; else U32(BitsOffset(9,1), (13,1), (18,1), (30,1)); `ratio` 3 bits; ratio 0: xsize the same way; ratios 1..7 = 1:1, 12:10, 4:3, 3:2, 16:9, 5:4, 2:1.
- ImageMetadata: `all_default`; `extra_fields` (orientation-1 3 bits, intrinsic size, preview, animation header); BitDepth (float flag; U32(8,10,12,BitsOffset(6,1)) or float U32(32,16,24,BitsOffset(6,1)) + 4-bit exponent-1); `modular_16bit` bool (default 1); `num_extra_channels` U32(0,1,BitsOffset(4,2),BitsOffset(12,1)) + ExtraChannelInfo each; `xyb_encoded` bool (default 1); ColorEncoding; ToneMapping only if `extra_fields`; extensions.
- After ImageMetadata, ALWAYS CustomTransformData (`all_default` bool first), then ICC stream if `want_icc`, then zero-pad to a byte, then the frame header.
- Customxy: two U32(Bits(19), BitsOffset(19,524288), BitsOffset(20,1048576), BitsOffset(21,2097152)), UnpackSigned. ColorEncoding: `want_icc`, color_space Enum(U32(0,1,BitsOffset(4,2),BitsOffset(6,18))), white point (not for XYB; custom = Customxy), primaries (not for grey / XYB; custom = 3 Customxy), gamma (bool + 24 bits) or transfer-function Enum, rendering intent Enum.
- FrameHeader order: all_default; frame_type U32(0,1,2,3); encoding bool (1 = modular); flags U64; `do_ycbcr` bool if not xyb; chroma subsampling if ycbcr and no LF frame; upsampling U32(1,2,4,8) + per extra channel (not with LF frame); group_size_shift 2 bits (modular, group dim = 128 << shift); x_qm_scale 3 bits (default 3), b_qm_scale 3 bits (default 2) for VarDCT + XYB; Passes (not for reference-only frames); lf level U32(1,2,3,4) for LF frames; custom size / origin (crop sizes U32(Bits(8), BitsOffset(11,256), BitsOffset(14,2304), BitsOffset(30,18688)), origin signed); BlendingInfo + one per extra channel; animation duration / timecode; `is_last`; `save_as_reference` 2 bits if not last; `save_before_color_transform` under the CanBeReferenced rule; name; LoopFilter (gab, epf_iters 2 bits, optional F16 sets, `epf_sigma_for_modular` for modular); extensions.
- MA tree contexts: split value 0, property 1, predictor 2, offset 3, multiplier log 4, multiplier bits 5. Nodes are stored breadth first: after reading a split node, its children are `tree.size() + to_decode + 1` and `+ 2`. Leaf: predictor, offset (signed), multiplier = (mul_bits + 1) << mul_log.
- Properties: 0 channel, 1 group id, 2 y, 3 x, 4 |N|, 5 |W|, 6 N, 7 W, 8 W minus the property-9 value of the previous pixel in the row (0 at the row start), 9 local gradient (W + N - NW), 10 W - NW, 11 NW - N, 12 N - NE, 13 N - NN, 14 W - WW, 15 weighted predictor max error; then 4 per earlier channel of equal size (|v|, v, |v - grad|, v - grad).
- Weighted predictor: 4 sub-predictors, 3 extra bits, error weight `4 + ((maxweight * divlookup[x >> shift]) >> shift)`, `divlookup[i] = 2^24 / (i + 1)`, defaults per header `all_default` (p1C 16, p2C 10, p3Ca..c 7, p3Cd 0, p3Ce 0, w = 0xd 0xc 0xc 0xc).
- ANS histogram: simple code (1 or 2 symbols, VarLenUint8), flat code, or logcount code with a 128-entry Huffman lookup table (7 peeked bits -> bits consumed, logcount) and run-length of repeated counts. The table is in `dec_ans.cc` (`huff[128][2]`).
