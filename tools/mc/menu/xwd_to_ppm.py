#!/usr/bin/env python3
import struct
import sys

from PIL import Image


def main():
    src, dst = sys.argv[1], sys.argv[2]
    data = open(src, "rb").read()
    header = struct.unpack(">25I", data[:100])
    hdr_size, width, height, bpl, ncolors = header[0], header[4], header[5], header[12], header[19]
    off = hdr_size + ncolors * 12
    img = Image.frombuffer("RGBA", (width, height), data[off:off + bpl * height], "raw", "BGRA", bpl, 1).convert("RGB")
    with open(dst, "wb") as f:
        f.write(b"P6\n%d %d\n255\n" % (width, height))
        f.write(img.tobytes())


main()
