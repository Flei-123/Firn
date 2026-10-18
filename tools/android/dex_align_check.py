#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/dex_align_check.py -- ART refuses a dex whose map_list names a code_item / type_list / map_list at an offset
# that is not a multiple of 4 ("Offset(782) should be aligned by 4 for maplist item of type 8193", seen on Android 16).
# usage: python3 tools/android/dex_align_check.py classes.dex   (exit 1 on a misaligned map entry or item)
import struct
import sys

d = open(sys.argv[1], "rb").read()
map_off, = struct.unpack_from("<I", d, 0x34)
n, = struct.unpack_from("<I", d, map_off)
bad = 0
for i in range(n):
    typ, _, cnt, off = struct.unpack_from("<HHII", d, map_off + 4 + 12 * i)
    if typ in (0x1001, 0x2001, 0x1000) and off % 4:
        print("misaligned map entry type 0x%x offset %d" % (typ, off))
        bad = 1
    if typ == 0x2001:
        first = off
print("dex align check: %s" % ("FAIL" if bad else "ok"))
sys.exit(bad)
