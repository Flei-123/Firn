#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdf_pattern_fixtures import page_pdf, stream

DECODE = "/Decode [0 200 0 200 0 1 0 1 0 1]"
HEAD = "/ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 "


def vertex(flag, x, y, rgb, flag_bits=True):
    out = bytearray()
    if flag is not None:
        out.append(flag)
    out += struct.pack(">HH", int(x / 200 * 65535), int(y / 200 * 65535))
    out += bytes(rgb)
    return bytes(out)


def point(x, y):
    return struct.pack(">HH", int(x / 200 * 65535), int(y / 200 * 65535))


def shading_page(dict_text, data, extra=None):
    body = "q 0 0 200 200 re W n /Sh sh Q"
    objs = [stream(dict_text, data)]
    return page_pdf(body, "/Shading << /Sh 5 0 R >>", extra=objs)


def coons_points(x0, y0, x1, y1, bend):
    return [(x0, y0), (x0 - bend, y0 + (y1 - y0) / 3), (x0 + bend, y0 + 2 * (y1 - y0) / 3), (x0, y1),
            (x0 + (x1 - x0) / 3, y1 + bend), (x0 + 2 * (x1 - x0) / 3, y1 - bend), (x1, y1),
            (x1 + bend, y1 - (y1 - y0) / 3), (x1 - bend, y1 - 2 * (y1 - y0) / 3), (x1, y0),
            (x0 + 2 * (x1 - x0) / 3, y0 - bend), (x0 + (x1 - x0) / 3, y0 + bend)]


def make(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    cases = {}
    tri = vertex(0, 20, 20, (255, 0, 0)) + vertex(0, 180, 30, (0, 255, 0)) + vertex(0, 90, 180, (0, 0, 255))
    cases["mesh4"] = shading_page("/ShadingType 4 " + HEAD + "/BitsPerFlag 8 " + DECODE, tri)
    strip = (vertex(0, 20, 20, (255, 0, 0)) + vertex(0, 100, 20, (0, 255, 0)) + vertex(0, 60, 100, (0, 0, 255)) +
             vertex(1, 140, 100, (255, 255, 0)) + vertex(2, 100, 180, (255, 0, 255)))
    cases["mesh4_strip"] = shading_page("/ShadingType 4 " + HEAD + "/BitsPerFlag 8 " + DECODE, strip)
    lattice = b""
    cols = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0), (0, 255, 255), (255, 0, 255), (255, 255, 255), (0, 0, 0), (128, 128, 0)]
    for r in range(3):
        for c in range(3):
            lattice += vertex(None, 20 + c * 80 + (r % 2) * 10, 20 + r * 80, cols[r * 3 + c])
    cases["mesh5"] = shading_page("/ShadingType 5 " + HEAD + "/VerticesPerRow 3 " + DECODE, lattice)
    pts = coons_points(30, 30, 170, 170, 12)
    coons = bytes([0]) + b"".join(point(*p) for p in pts) + bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0])
    cases["mesh6"] = shading_page("/ShadingType 6 " + HEAD + "/BitsPerFlag 8 " + DECODE, coons)
    coons2 = coons + bytes([2]) + b"".join(point(*p) for p in coons_points(30, 30, 170, 170, 12)[4:12]) + bytes([0, 255, 255, 255, 0, 255])
    cases["mesh6_edge"] = shading_page("/ShadingType 6 " + HEAD + "/BitsPerFlag 8 " + DECODE,
                                       bytes([0]) + b"".join(point(*p) for p in coons_points(20, 20, 100, 100, 6)) +
                                       bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]) +
                                       bytes([2]) + b"".join(point(*p) for p in [(110, 150), (130, 170), (150, 150), (170, 170), (170, 130), (150, 110), (170, 90), (130, 20)][:8]) + bytes([255, 0, 255, 0, 255, 255]))
    tensor_pts = coons_points(30, 30, 170, 170, 12) + [(75, 75), (75, 125), (125, 125), (125, 75)]
    tensor = bytes([0]) + b"".join(point(*p) for p in tensor_pts) + bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0])
    cases["mesh7"] = shading_page("/ShadingType 7 " + HEAD + "/BitsPerFlag 8 " + DECODE, tensor)
    fn = "<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >>"
    fv = lambda f, x, y, t: bytes([f]) + struct.pack(">HH", int(x / 200 * 65535), int(y / 200 * 65535)) + bytes([t])
    ftri = fv(0, 20, 20, 0) + fv(0, 180, 30, 128) + fv(0, 90, 180, 255)
    cases["mesh4_fn"] = shading_page("/ShadingType 4 /ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 200 0 200 0 1] /Function " + fn, ftri)
    cases["mesh4_gray"] = shading_page("/ShadingType 4 /ColorSpace /DeviceGray /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [0 200 0 200 0 1]",
                                       fv(0, 20, 20, 0) + fv(0, 180, 30, 255) + fv(0, 90, 180, 128))
    for name, data in cases.items():
        with open(os.path.join(out_dir, name + ".pdf"), "wb") as handle:
            handle.write(data)
    return list(cases)


if __name__ == "__main__":
    print(" ".join(make(sys.argv[1])))
