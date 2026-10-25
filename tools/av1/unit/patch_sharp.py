#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/patch_sharp.py <in.avif> <out.avif> <sharpness 0..7> -- rewrite loop_filter_sharpness (3 bits) in the frame header.
libaom always writes sharpness 0 for intra frames, so the sharpness path of the loop filter needs patched fixtures.
The frame header is plain (non entropy coded) bits, so the tile data stays valid; ffmpeg's decoder is the reference."""
import re, subprocess, sys
src, dst, val = sys.argv[1], sys.argv[2], int(sys.argv[3])
data = bytearray(open(src, "rb").read())
log = subprocess.run(["ffmpeg", "-v", "debug", "-i", src, "-c", "copy", "-bsf:v", "trace_headers", "-f", "null", "-"],
                     capture_output=True, text=True).stderr
pos = None
size = None
for line in log.splitlines():
    m = re.search(r"\] (\d+)\s+loop_filter_sharpness\s+([01]{3}) = (\d)", line)
    if m and pos is None:
        pos = int(m.group(1))
    m = re.search(r"\] 8\s+obu_size\s+= (\d+)", line)
    if m:
        size = int(m.group(1))  # last obu_size seen = the frame OBU
assert pos is not None and size is not None, "trace failed"
leb = bytearray()
v = size
while True:
    b = v & 0x7F
    v >>= 7
    leb.append(b | (0x80 if v else 0))
    if not v:
        break
needle = bytes([0x32]) + bytes(leb)
at = data.find(needle)
assert at >= 0 and data.find(needle, at + 1) < 0, "frame OBU not found uniquely"
for i in range(3):
    bit = pos + i
    byte = at + bit // 8
    mask = 0x80 >> (bit % 8)
    if (val >> (2 - i)) & 1:
        data[byte] |= mask
    else:
        data[byte] &= ~mask & 0xFF
open(dst, "wb").write(data)
