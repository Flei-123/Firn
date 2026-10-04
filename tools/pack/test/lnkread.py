#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""A Windows shortcut (.lnk) reader written from [MS-SHLLINK], independent of lib/pack/lnk.fi,
used by the tests to check what the Firn writer made.   lnkread.py FILE.lnk  -> JSON"""
import json
import struct
import sys

CLSID = bytes([0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46])


def parse(data):
    if len(data) < 76 or struct.unpack_from("<I", data, 0)[0] != 0x4C or data[4:20] != CLSID:
        raise ValueError("not a shell link")
    flags, attrs = struct.unpack_from("<II", data, 20)
    showcmd, = struct.unpack_from("<I", data, 60)
    icon_index, = struct.unpack_from("<i", data, 56)
    at = 76
    out = {"flags": flags, "show": showcmd, "icon_index": icon_index, "idlist": None}
    if flags & 1:                                   # HasLinkTargetIDList
        size, = struct.unpack_from("<H", data, at)
        ids, p = [], at + 2
        while True:
            n, = struct.unpack_from("<H", data, p)
            if n == 0:
                break
            ids.append(data[p + 2:p + n])
            p += n
        if p != at + 2 + size - 2:
            raise ValueError("the ID list size does not match its items")
        out["idlist"] = ids
        at += 2 + size
    if flags & 2:                                   # HasLinkInfo
        li = at
        size, hsize, lflags, vol, base, net, suffix = struct.unpack_from("<IIIIIII", data, li)
        if lflags & 1:
            out["target_ansi"] = data[li + base:data.index(b"\0", li + base)].decode("latin-1")
            vsize, dtype, serial, loff = struct.unpack_from("<IIII", data, li + vol)
            out["drive_type"] = dtype
            if hsize >= 36:
                ubase, usuf = struct.unpack_from("<II", data, li + 28)
                p = li + ubase
                e = p
                while data[e:e + 2] != b"\0\0" or (e - p) % 2:
                    e += 1
                out["target"] = data[p:e].decode("utf-16-le")
            else:
                out["target"] = out["target_ansi"]
        if li + size > len(data):
            raise ValueError("LinkInfo runs past the end")
        at += size
    names = [(4, "desc"), (16, "workdir"), (32, "args"), (64, "icon")]
    if not flags & 0x80:
        raise ValueError("only Unicode string data is read here")
    # NAME_STRING, RELATIVE_PATH(8), WORKING_DIR, COMMAND_LINE_ARGUMENTS, ICON_LOCATION in this order
    for bit, key in ((4, "desc"), (8, "relpath"), (16, "workdir"), (32, "args"), (64, "icon")):
        if flags & bit:
            n, = struct.unpack_from("<H", data, at)
            out[key] = data[at + 2:at + 2 + 2 * n].decode("utf-16-le")
            at += 2 + 2 * n
    if struct.unpack_from("<I", data, at)[0] != 0:
        raise ValueError("no terminal block")
    if at + 4 != len(data):
        raise ValueError("bytes after the terminal block")
    return out


if __name__ == "__main__":
    print(json.dumps(parse(open(sys.argv[1], "rb").read()), indent=1, default=lambda b: b.hex()))
