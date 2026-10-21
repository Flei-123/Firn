#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/lucide_icons.py -- WRITES lib/fui/lucide.fi OUT OF LUCIDE.
#
#   python3 tools/fui/lucide_icons.py <icon-nodes.json> > lib/fui/lucide.fi
#
# <icon-nodes.json> is the file of that name in the npm package
# lucide-static (https://lucide.dev, ISC licence, see
# LICENSES/Lucide-ISC.txt). Only the icons named in WANTED are taken --
# a program pays for every string in its data segment, and 2000 icons
# nobody draws are 700 KB of WebAssembly.
#
# Every Lucide icon is drawn on a 24 x 24 grid with a 2 unit stroke, round
# caps and round joins, no fill. Its elements (path, circle, ellipse,
# rect, line, polyline, polygon) are turned into ONE SVG path string here,
# so the drawing side (lib/fui/icons.fi) needs exactly one parser,
# svg.path.split_d, and no XML at all:
#
#   circle  cx cy r        M cx-r cy  a r r 0 1 0 2r 0  a r r 0 1 0 -2r 0 z
#   ellipse cx cy rx ry    the same with rx/ry
#   rect    x y w h rx ry  four lines, four quarter arcs when rx > 0
#                          (ry defaults to rx, and both are clamped to
#                          half the side, as SVG 1.1 9.2 says)
#   line    x1 y1 x2 y2    M x1 y1 L x2 y2
#   polyline / polygon     M p0 L p1 ... (z for the polygon)
#
# To add an icon: put its Lucide name into WANTED, run this again, and
# commit both files. The constant is the name in upper case with '_' for
# '-' (send-horizontal -> SEND_HORIZONTAL).
import json
import re
import sys

VERSION = "lucide-static 1.48.0"

WANTED = [
    # the head of a messenger
    "users", "user", "user-round", "user-plus", "user-check", "user-x",
    "plus", "message-square-plus", "square-pen", "sun", "moon", "log-out",
    "log-in", "settings", "search", "x", "menu", "ellipsis-vertical",
    # navigation
    "arrow-left", "arrow-right", "arrow-down", "arrow-up", "chevron-left",
    "chevron-right", "chevron-down", "chevron-up",
    # the conversation
    "send", "send-horizontal", "paperclip", "face-slightly-smiling", "mic", "image",
    "message-circle", "message-square", "messages-square", "hash",
    "at-sign", "lock", "lock-open", "shield-check", "key-round", "check",
    "check-check", "clock", "bell", "bell-off", "pin",
    # state and actions
    "wifi", "wifi-off", "refresh-cw", "circle-alert", "info",
    "circle-check", "ban", "trash", "pencil", "copy", "share-2",
    "eye", "eye-off", "smartphone", "monitor", "globe", "mail", "phone",
    "star", "heart", "house", "folder", "download", "upload",
    "external-link", "link", "qr-code",
    # a launcher and its kit (lib/fui/kit.fi, examples/fui/launcher_kit.fi)
    "play", "package", "puzzle", "layout-grid", "library", "triangle-alert",
    "loader-circle", "folder-open", "gamepad-2", "server", "hard-drive",
    "circle-x", "file-text", "rocket", "sparkles", "layers", "list",
    "terminal", "box",
    # context menus (lib/fui/ctxstd.fi): cut / paste / undo / select, window
    # and process entries, sort
    "scissors", "clipboard-paste", "undo-2", "redo-2", "text-select", "minus",
    "square", "minimize-2", "move", "move-diagonal-2", "arrow-up-down",
    "pin-off", "list-checks", "app-window", "power",
    # a camera app (examples/firncam)
    "zap", "zap-off", "timer", "timer-off", "grid-3x3", "switch-camera", "camera", "sliders-horizontal", "aperture", "video", "scan", "circle", "flashlight",
    "volume-2", "volume-x",
]


def num(v):
    f = float(v)
    if f == int(f):
        return str(int(f))
    s = ("%.4f" % f).rstrip("0").rstrip(".")
    return s


def attrs_float(a, k, default=0.0):
    return float(a[k]) if k in a else default


NUMBER = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


def absolute_start(d):
    # The elements of an icon are separate <path>s, and the first command
    # of a path is absolute even when it is written `m` (SVG 1.1, 8.3.2).
    # Joined into ONE string, a leading `m` would be taken relative to the
    # end of the element before -- the rays of `sun` landed in the middle.
    # So: `m x y [more pairs]` -> `M x y l [more pairs]` (the pairs after
    # the first one of an `m` are relative line-tos, and stay that).
    d = d.strip()
    if not d.startswith("m"):
        return d
    rest = d[1:]
    nums = []
    pos = 0
    while len(nums) < 2:
        m = NUMBER.search(rest, pos)
        nums.append(m.group(0))
        pos = m.end()
    tail = rest[pos:].lstrip(" ,")
    out = "M%s %s" % (nums[0], nums[1])
    if tail and not tail[0].isalpha():
        out += "l" + tail
    else:
        out += tail
    return out


def to_d(tag, a):
    if tag == "path":
        return absolute_start(a["d"])
    if tag in ("circle", "ellipse"):
        cx = attrs_float(a, "cx")
        cy = attrs_float(a, "cy")
        if tag == "circle":
            rx = ry = attrs_float(a, "r")
        else:
            rx = attrs_float(a, "rx")
            ry = attrs_float(a, "ry")
        return ("M%s %sa%s %s 0 1 0 %s 0a%s %s 0 1 0 %s 0z"
                % (num(cx - rx), num(cy), num(rx), num(ry), num(2 * rx),
                   num(rx), num(ry), num(-2 * rx)))
    if tag == "rect":
        x = attrs_float(a, "x")
        y = attrs_float(a, "y")
        w = attrs_float(a, "width")
        h = attrs_float(a, "height")
        rx = a.get("rx")
        ry = a.get("ry")
        if rx is None and ry is None:
            rx = ry = 0.0
        elif rx is None:
            rx = ry = float(ry)
        elif ry is None:
            rx = ry = float(rx)
        rx = min(float(rx), w / 2)
        ry = min(float(ry), h / 2)
        if rx <= 0 or ry <= 0:
            return "M%s %sh%sv%sh%sz" % (num(x), num(y), num(w), num(h),
                                         num(-w))
        return ("M%s %sh%sa%s %s 0 0 1 %s %sv%sa%s %s 0 0 1 %s %s"
                "h%sa%s %s 0 0 1 %s %sv%sa%s %s 0 0 1 %s %sz"
                % (num(x + rx), num(y), num(w - 2 * rx),
                   num(rx), num(ry), num(rx), num(ry), num(h - 2 * ry),
                   num(rx), num(ry), num(-rx), num(ry),
                   num(-(w - 2 * rx)),
                   num(rx), num(ry), num(-rx), num(-ry), num(-(h - 2 * ry)),
                   num(rx), num(ry), num(rx), num(-ry)))
    if tag == "line":
        return "M%s %sL%s %s" % (num(attrs_float(a, "x1")),
                                 num(attrs_float(a, "y1")),
                                 num(attrs_float(a, "x2")),
                                 num(attrs_float(a, "y2")))
    if tag in ("polyline", "polygon"):
        pts = a["points"].replace(",", " ").split()
        pairs = [(pts[i], pts[i + 1]) for i in range(0, len(pts) - 1, 2)]
        s = "M%s %s" % (num(pairs[0][0]), num(pairs[0][1]))
        for px, py in pairs[1:]:
            s += "L%s %s" % (num(px), num(py))
        if tag == "polygon":
            s += "z"
        return s
    raise SystemExit("lucide_icons: element <%s> is not handled" % tag)


# NOT Lucide: icons copied from the JARVIS web app so the Android app matches it (appended LAST: the ids of the others stay).
EXTRA = {
    "wrench": [["path", {"d": "M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18v3h3l6.3-6.3a4 4 0 0 0 5.4-5.4l-2.6 2.6-2-2 2.6-2.6z"}]],
    "lightbulb": [["path", {"d": "M11 3a5 5 0 0 0-5 5c0 1.6.8 2.6 1.5 3.4.5.6 1 1.1 1 2.1v.5h5v-.5c0-1 .5-1.5 1-2.1C16.2 10.6 17 9.6 17 8a5 5 0 0 0-5-5z"}],
                  ["path", {"d": "M10 20h4M11 22h2"}]],
    "loader-fabric": [["path", {"d": "M21.3 9.98L13.88 2.38C13.48 1.98 12.98 1.55 11.63 2.47C9.91 3.65 10.91 4.37 11.18 4.67C11.89 5.45 18.48 12.39 19.61 13.69C19.83 13.93 17.85 11.74 19.05 10.25C19.52 9.64 20.61 9.21 21.3 9.99C21.87 10.62 22.23 11.55 21.01 12.56C18.87 14.33 12.16 19.71 10 21.49C8.91 22.39 7.52 21.69 6.89 20.97C6.24 20.2 4.24 17.97 2.44 15.93C1.97 15.38 2.09 14.23 2.64 13.76C5.68 11.13 10.61 7.36 12.35 5.89"}]],
    "loader-forge": [["path", {"d": "M2 7.5L10 7.5L10 5.5L22 5.5L22 7.5C22 7.5 15 10.9 15 13.5C15 16.1 18.1 16.6 18.1 16.6L19 20.5L5 20.5L6 16.4C6 16.4 9.8 16.5 10 13.5C10.2 10.8 3.5 12.8 2 7.5z"}]],
    "loader-quilt": [["path", {"d": "M10.31 3.92C10.31 3.73 10.15 3.57 9.96 3.57L3.94 3.57C3.75 3.57 3.59 3.73 3.59 3.92L3.59 9.93C3.59 10.12 3.75 10.28 3.94 10.28L9.96 10.28C10.15 10.28 10.31 10.12 10.31 9.92L10.31 3.92zM10.31 14.12C10.31 13.93 10.15 13.77 9.96 13.77L3.94 13.77C3.75 13.77 3.59 13.93 3.59 14.12L3.59 20.13C3.59 20.32 3.75 20.48 3.94 20.48L9.96 20.48C10.15 20.48 10.31 20.32 10.31 20.12L10.31 14.12zM20.41 3.92C20.41 3.73 20.25 3.57 20.06 3.57L14.04 3.57C13.85 3.57 13.69 3.73 13.69 3.92L13.69 9.93C13.69 10.12 13.85 10.28 14.04 10.28L20.06 10.28C20.25 10.28 20.41 10.12 20.41 9.92L20.41 3.92zM22.18 18.16C22.32 18.02 22.32 17.79 22.18 17.66L18.26 13.73C18.12 13.59 17.9 13.59 17.76 13.73L13.84 17.64C13.71 17.78 13.71 18.01 13.84 18.14L17.77 22.07C17.91 22.21 18.13 22.21 18.27 22.07L22.18 18.16z"}]],
    "loader-vanilla": [["path", {"d": "M12 3L4 7.2L4 16.8L12 21L20 16.8L20 7.2L12 3zM4 7.2L12 11L20 7.2M12 11L12 21"}]],
}


def main():
    nodes = json.load(open(sys.argv[1], encoding="utf-8"))
    nodes.update(EXTRA)
    out = []
    w = out.append
    w("// SPDX-License-Identifier: ISC")
    w("// lib/fui/lucide.fi -- THE LUCIDE ICONS fUi DRAWS, AS PATH DATA.")
    w("//")
    w("// GENERATED by tools/fui/lucide_icons.py out of %s" % VERSION)
    w("// (icon-nodes.json). Do not edit by hand: add the name to WANTED")
    w("// there and run it again.")
    w("//")
    w("// Lucide, https://lucide.dev -- ISC License, Copyright (c) 2026")
    w("// Lucide Icons and Contributors; the icons Lucide took over from")
    w("// Feather are MIT, Copyright (c) 2013-present Cole Bemis. The full")
    w("// texts stand in LICENSES/Lucide-ISC.txt. This file is the only one")
    w("// in lib/ under that licence; lib/fui/icons.fi, which draws it, is")
    w("// MPL-2.0 like the rest.")
    w("//")
    w("// Every icon is a 24 x 24 grid, meant to be STROKED 2 units wide")
    w("// with round caps and joins (lib/fui/icons.fi does that). One string")
    w("// per icon: all its elements, turned into path commands.")
    w("")
    names = []
    for n in WANTED + list(EXTRA):
        if n not in nodes:
            raise SystemExit("lucide_icons: no icon '%s' in %s" % (n, sys.argv[1]))
        names.append(n)
    consts = [n.upper().replace("-", "_") for n in names]
    w("export {")
    w("    LUCIDE_N, lucide_d, lucide_name, lucide_find,")
    line = "   "
    for c in consts:
        piece = " " + c + ","
        if len(line) + len(piece) > 76:
            w(line)
            line = "   "
        line += piece
    w(line)
    w("}")
    w("")
    w("const LUCIDE_N: i64 = %d" % len(names))
    w("")
    for i, c in enumerate(consts):
        w("const %s: i64 = %d" % (c, i))
    w("")
    w("// The path string of an icon; an empty string for an unknown id.")
    w("fn lucide_d(id: i64) -> str {")
    for i, n in enumerate(names):
        d = "".join(to_d(t, a) for t, a in nodes[n])
        assert '"' not in d and "\\" not in d
        w("    if id == %d {" % i)
        w("        return \"%s\"" % d)
        w("    }")
    w("    return \"\"")
    w("}")
    w("")
    w("// The Lucide name of an icon (\"send-horizontal\").")
    w("fn lucide_name(id: i64) -> str {")
    for i, n in enumerate(names):
        w("    if id == %d {" % i)
        w("        return \"%s\"" % n)
        w("    }")
    w("    return \"\"")
    w("}")
    w("")
    w("// The id of a Lucide name, -1 when this set does not carry it.")
    w("fn lucide_find(p: u64, n: usize) -> i64 {")
    w("    var i: i64 = 0")
    w("    while i < LUCIDE_N {")
    w("        let s: str = lucide_name(i)")
    w("        if s.n == n {")
    w("            var k: usize = 0")
    w("            while k < n && *((s.p as u64 + k as u64) as *mut u8) == *((p + k as u64) as *mut u8) {")
    w("                k = k + 1")
    w("            }")
    w("            if k == n {")
    w("                return i")
    w("            }")
    w("        }")
    w("        i = i + 1")
    w("    }")
    w("    return 0 - 1")
    w("}")
    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
