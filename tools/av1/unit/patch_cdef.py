#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/patch_cdef.py <in.avif> <out.avif> <damping 3..6> <idx:ypri:ysec:uvpri:uvsec>... -- rewrite the CDEF strengths in the frame header.
ypri/uvpri 0..15, ysec/uvsec raw 0..3 (3 means 4).  Only the values change (bit widths stay), cdef_bits is untouched, so the tile data
stays valid; ffmpeg's decoder is the reference.  libaom always picks strengths of its own choosing, this reaches the rest of the table."""
import re, subprocess, sys
src, dst, damp = sys.argv[1], sys.argv[2], int(sys.argv[3])
specs = [tuple(int(v) for v in a.split(":")) for a in sys.argv[4:]]
data = bytearray(open(src, "rb").read())
log = subprocess.run(["ffmpeg", "-v", "debug", "-i", src, "-c", "copy", "-bsf:v", "trace_headers", "-f", "null", "-"],
                     capture_output=True, text=True).stderr
pos = {}
size = None
for line in log.splitlines():
    m = re.search(r"\] (\d+)\s+(cdef_\w+(?:\[\d+\])?)\s+([01]+) = (\d+)", line)
    if m and m.group(2) not in pos:
        pos[m.group(2)] = (int(m.group(1)), len(m.group(3)))
    m = re.search(r"\] 8\s+obu_size\s+= (\d+)", line)
    if m:
        size = int(m.group(1))
assert size is not None and "cdef_damping_minus_3" in pos, "trace failed"
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
def put(name, val):
    p, n = pos[name]
    assert 0 <= val < (1 << n), (name, val)
    for i in range(n):
        bit = p + i
        byte = at + bit // 8
        mask = 0x80 >> (bit % 8)
        if (val >> (n - 1 - i)) & 1:
            data[byte] |= mask
        else:
            data[byte] &= ~mask & 0xFF
put("cdef_damping_minus_3", damp - 3)
for idx, yp, ys, up, us in specs:
    if "cdef_y_pri_strength[%d]" % idx not in pos:
        continue  # idx >= 1 << cdef_bits: not present in this stream
    put("cdef_y_pri_strength[%d]" % idx, yp)
    put("cdef_y_sec_strength[%d]" % idx, ys)
    if "cdef_uv_pri_strength[%d]" % idx in pos:
        put("cdef_uv_pri_strength[%d]" % idx, up)
        put("cdef_uv_sec_strength[%d]" % idx, us)
open(dst, "wb").write(data)
