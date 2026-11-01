#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import struct
import sys
import zlib


def main():
    data = sys.stdin.buffer.read()
    nl = data.index(b"\n")
    w, h = map(int, data[:nl].split())
    px = data[nl + 1:nl + 1 + w * h * 4]
    raw = b"".join(b"\x00" + px[y * w * 4:(y + 1) * w * 4] for y in range(h))

    def chunk(tag, body):
        c = struct.pack(">I", len(body)) + tag + body
        return c + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)

    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
    open(sys.argv[1], "wb").write(png)


if __name__ == "__main__":
    main()
