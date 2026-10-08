#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/png_tool.py -- helpers of clipboard_check.py that need no clipboard:
#   make <file>              writes a 3x2 RGB test PNG (hand made with zlib, no imaging library)
#   digest <file>            prints "PNG <w> <h> <sha256 of RGBA>" of a PNG file as GdkPixbuf decodes it
import hashlib, struct, sys, zlib

def chunk(t, d):
    c = struct.pack(">I", len(d)) + t + d
    return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

def make(path):
    rows = [bytes([255, 0, 0, 0, 255, 0, 0, 0, 255]), bytes([10, 20, 30, 200, 210, 220, 255, 255, 0])]
    raw = b"".join(b"\x00" + r for r in rows)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 3, 2, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)

def digest(path):
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    pb = GdkPixbuf.Pixbuf.new_from_file(path)
    if not pb.get_has_alpha():
        pb = pb.add_alpha(False, 0, 0, 0)
    px = bytes(pb.get_pixels())
    rs = pb.get_rowstride()
    w, h = pb.get_width(), pb.get_height()
    data = b"".join(px[y * rs:y * rs + w * 4] for y in range(h))
    print("PNG", w, h, hashlib.sha256(data).hexdigest())

if sys.argv[1] == "make":
    make(sys.argv[2])
else:
    digest(sys.argv[2])
