#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import struct
import sys


def build(objects, root=1):
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n" % (len(objects) + 1)
    out += b"0000000000 65535 f \n"
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objects) + 1, root, xref)
    return bytes(out)


def stream(dict_text, data):
    return b"<< " + dict_text.encode() + b" /Length %d >>\nstream\n" % len(data) + data + b"\nendstream"


def page_pdf(content, resources, extra=()):
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << " + resources.encode() + b" >> >>",
        stream("", content.encode()),
    ]
    objects.extend(extra)
    return build(objects)


def exp_fn(c0, c1, n=1):
    return "<< /FunctionType 2 /Domain [0 1] /C0 [%s] /C1 [%s] /N %s >>" % (c0, c1, n)


def stitch_fn():
    return ("<< /FunctionType 3 /Domain [0 1] /Bounds [0.3 0.7] /Encode [0 1 0 1 0 1] /Functions ["
            + exp_fn("1 0 0", "1 1 0") + exp_fn("1 1 0", "0 0.6 0") + exp_fn("0 0.6 0", "0 0 1") + "] >>")


def sampled_obj():
    samples = bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 255])
    return stream("/FunctionType 0 /Domain [0 1] /Range [0 1 0 1 0 1] /Size [5] /BitsPerSample 8", samples)


def make(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    cases = {}
    cases["axial"] = page_pdf(
        "/P1 cs /Pt scn 20 20 160 160 re f",
        "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 20 180 180] /Function "
        + exp_fn("1 0 0", "0 0 1") + " /Extend [true true] >> >> >> /ColorSpace << /P1 /Pattern >>")
    cases["axial_noextend"] = page_pdf(
        "/P1 cs /Pt scn 10 10 180 180 re f",
        "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [60 60 140 100] /Function "
        + exp_fn("1 0.5 0", "0 0.5 1") + " /Background [0.9 0.9 0.9] >> >> >> /ColorSpace << /P1 /Pattern >>")
    cases["radial"] = page_pdf(
        "/P1 cs /Pt scn 20 20 160 160 re f",
        "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [100 100 0 100 100 80] /Function "
        + exp_fn("1 1 0", "0 0 0.8") + " /Extend [false false] >> >> >> /ColorSpace << /P1 /Pattern >>")
    cases["radial_offset"] = page_pdf(
        "/P1 cs /Pt scn 10 10 180 180 re f",
        "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [70 120 10 120 80 50] /Function "
        + stitch_fn() + " /Extend [true true] >> >> >> /ColorSpace << /P1 /Pattern >>")
    cases["stitch_sh"] = page_pdf(
        "q 20 20 160 160 re W n /Sh sh Q",
        "/Shading << /Sh << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 0 180 0] /Function " + stitch_fn() + " >> >>")
    cases["sampled"] = page_pdf(
        "q 20 60 160 80 re W n /Sh sh Q",
        "/Shading << /Sh << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 0 180 0] /Function 5 0 R /Extend [true true] >> >>",
        extra=[sampled_obj()])
    cases["gray_cmyk"] = page_pdf(
        "q 10 110 180 80 re W n /G sh Q q 10 10 180 80 re W n /C sh Q",
        "/Shading << /G << /ShadingType 2 /ColorSpace /DeviceGray /Coords [10 0 190 0] /Function " + exp_fn("0", "1")
        + " >> /C << /ShadingType 2 /ColorSpace /DeviceCMYK /Coords [10 0 190 0] /Function " + exp_fn("0 0 0 0", "1 0.5 0 0.2") + " >> >>")
    cases["stroke_pattern"] = page_pdf(
        "/P1 CS /Pt SCN 12 w 30 30 140 140 re S",
        "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [30 30 170 170] /Function "
        + exp_fn("1 0 0", "0 0 1") + " /Extend [true true] >> >> >> /ColorSpace << /P1 /Pattern >>")
    tile = "0.9 0.2 0.2 rg 0 0 10 10 re f 0.2 0.2 0.9 rg 10 10 10 10 re f"
    cases["tiling"] = page_pdf(
        "/Pattern cs /Pt scn 20 20 160 160 re f",
        "/Pattern << /Pt 5 0 R >>",
        extra=[stream("/PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20] /XStep 20 /YStep 20 /Resources << >>", tile.encode())])
    cases["tiling_matrix"] = page_pdf(
        "/Pattern cs /Pt scn 20 20 160 160 re f",
        "/Pattern << /Pt 5 0 R >>",
        extra=[stream("/PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20] /XStep 25 /YStep 25 /Matrix [0.866 0.5 -0.5 0.866 5 5] /Resources << >>", tile.encode())])
    cases["tiling_uncoloured"] = page_pdf(
        "/CsP cs 0.1 0.6 0.2 /Pt scn 20 20 160 160 re f 0.8 0.3 0.0 /Pt scn 60 60 80 80 re f",
        "/Pattern << /Pt 5 0 R >> /ColorSpace << /CsP [/Pattern /DeviceRGB] >>",
        extra=[stream("/PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8 /Resources << >>", b"1 1 6 6 re f")])
    cases["tiling_bbox_small"] = page_pdf(
        "/Pattern cs /Pt scn 0 0 200 200 re f",
        "/Pattern << /Pt 5 0 R >>",
        extra=[stream("/PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 16 /YStep 16 /Resources << >>", b"0 0.5 0.5 rg -5 -5 30 30 re f")])
    font_res = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >> >> "
    cases["text_axial"] = page_pdf(
        "/P1 cs /Pt scn BT /F1 64 Tf 10 70 Td (Firn) Tj ET",
        font_res + "/Pattern << /Pt << /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [10 0 190 0] /Function "
        + exp_fn("1 0 0", "0 0 1") + " /Extend [true true] >> >> >> /ColorSpace << /P1 /Pattern >>")
    cases["text_tiling"] = page_pdf(
        "/Pattern cs /Pt scn BT /F1 64 Tf 10 70 Td [(Fi) -50 (rn)] TJ ET",
        font_res + "/Pattern << /Pt 5 0 R >>",
        extra=[stream("/PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20] /XStep 20 /YStep 20 /Resources << >>", tile.encode())])
    for name, data in cases.items():
        with open(os.path.join(out_dir, name + ".pdf"), "wb") as handle:
            handle.write(data)
    return list(cases)


if __name__ == "__main__":
    print(" ".join(make(sys.argv[1])))
