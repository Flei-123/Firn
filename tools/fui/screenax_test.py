#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import dbus
from atspi_test import ACC, COMP, ACT, FRAME, ROLE, STATE, Client, bits

addr, jpath, title = sys.argv[1], sys.argv[2], sys.argv[3]
with open(jpath) as f:
    tree = json.load(f)
c = Client(addr)
root = "/org/a11y/atspi/accessible/root"
frame = str(c.call(root, ACC, "GetChildren")[0][1])
seen = 0
good = 0
bad = 0


def walk(jn, path, is_frame):
    global seen, good, bad
    seen += 1
    want_role = FRAME if is_frame else ROLE[jn["role"]]
    want_name = jn.get("name", "") or (title if is_frame else "")
    ok = c.call(path, ACC, "GetRole") == want_role and str(c.prop(path, ACC, "Name")) == want_name
    st = bits(c.call(path, ACC, "GetState"))
    for k, num in STATE.items():
        if k == "focused":
            continue
        if (k in jn.get("states", [])) != (num in st):
            ok = False
    ex = [int(v) for v in c.call(path, COMP, "GetExtents", dbus.UInt32(1))]
    if not is_frame and ex[2] <= 0:
        ok = False
    if ok:
        good += 1
    else:
        bad += 1
        print("  mismatch %s %r role=%s" % (path, want_name, jn["role"]))
    ach = c.call(path, ACC, "GetChildren")
    jch = jn.get("children", [])
    if len(ach) != len(jch):
        bad += 1
        print("  child count %s: atspi %d json %d" % (path, len(ach), len(jch)))
        return
    for jc, ac in zip(jch, ach):
        walk(jc, str(ac[1]), False)


walk(tree, frame, True)
print("METRIC atspi_visible=%d expected=%d" % (good, seen))
print("%s AT-SPI shows every registry node (%d of %d)" % ("  ok   " if bad == 0 and seen > 10 else "  WRONG", good, seen))
sys.exit(0 if bad == 0 and seen > 10 else 1)
