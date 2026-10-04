#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""lnkcheck.py FILE.lnk --target T --workdir D [--icon-suffix S] [--desc N]
Exit 0 when the shortcut says exactly that (no shell quoting involved: plain arguments)."""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lnkread   # noqa: E402

a = argparse.ArgumentParser()
a.add_argument("file"); a.add_argument("--target"); a.add_argument("--workdir")
a.add_argument("--icon-suffix"); a.add_argument("--desc"); a.add_argument("--idlist", action="store_true")
a = a.parse_args()
try:
    d = lnkread.parse(open(a.file, "rb").read())
except Exception as e:
    print("does not parse: %s" % e)
    sys.exit(1)
bad = []
if a.target is not None and d.get("target") != a.target:
    bad.append("target %r != %r" % (d.get("target"), a.target))
if a.workdir is not None and d.get("workdir") != a.workdir:
    bad.append("workdir %r != %r" % (d.get("workdir"), a.workdir))
if a.icon_suffix and not d.get("icon", "").endswith(a.icon_suffix):
    bad.append("icon %r" % d.get("icon"))
if a.desc is not None and d.get("desc") != a.desc:
    bad.append("desc %r" % d.get("desc"))
if a.idlist:
    ids = d.get("idlist") or []
    if len(ids) < 3 or ids[0][2:3] != b"\x1f" and ids[0][0:1] != b"\x1f":
        bad.append("the ID list does not start at My Computer")
    last = ids[-1] if ids else b""
    if b"hello.exe" not in last:
        bad.append("the last ID list item is not the file: %r" % last)
if bad:
    print("; ".join(bad))
    sys.exit(1)
