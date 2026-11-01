#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdf_pattern_fixtures import page_pdf, stream


def calc_obj(code):
    return stream("/FunctionType 4 /Domain [0 1] /Range [0 1 0 1 0 1]", code.encode())


def make(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    cases = {}
    programs = {
        "calc_ifelse": "{ dup 1 exch sub exch dup 0.5 gt { 1 } { 0 } ifelse }",
        "calc_roll": "{ dup 0.25 mul exch dup 0.5 exch sub abs 2 mul 3 1 roll }",
        "calc_trig": "{ 360 mul dup sin 0.5 mul 0.5 add exch dup cos 0.5 mul 0.5 add exch 2 div }",
        "calc_if": "{ dup 0.5 lt { 0.5 add } if dup dup 0.3 mul exch 0.8 mul }",
        "calc_idx": "{ dup dup 2 index exch pop 3 copy pop pop pop 0.5 mul exch 1 exch sub }",
    }
    for name, code in programs.items():
        cases[name] = page_pdf(
            "q 20 40 160 120 re W n /Sh sh Q",
            "/Shading << /Sh << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 0 180 0] /Function 5 0 R >> >>",
            extra=[calc_obj(code)])
    for name, data in cases.items():
        with open(os.path.join(out_dir, name + ".pdf"), "wb") as handle:
            handle.write(data)
    return list(cases)


if __name__ == "__main__":
    print(" ".join(make(sys.argv[1])))
