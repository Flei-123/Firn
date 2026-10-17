#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/shadow.py -- the source tree of a Firn program as the Android
# build sees it: a mirror of symlinks in which every PLATFORM WINDOW LAYER
# (a directory `window` holding a `backend.fi`) is left out, and the entry's
# directory gets window/backend.fi -> lib/window/android.fi.
#
# A project may also VENDOR copies of Firn library modules (vendor/std/...).
# The compiler searches project sources before $FIRNLIB, so a copy of a module
# that the library also reaches by itself (e.g. std.crypto.chacha via
# tls.tlsmini) fails with "name conflict: module comes from two files". Every
# file below a `vendor/std` directory that also exists in $FIRNLIB/std is left
# out: the library version is the one that is used on Android.
#
# Why a whole tree and not only the entry directory: a program with a
# firn.package finds its modules through the package's source directories,
# and a project may VENDOR its own window layer there (FIRNCHAT carries
# vendor/window/{window,backend}.fi, an X11 copy). Rule 1 of the module
# search ("next to the importing file") would pick that copy's backend.
# Without the directory, `import window.window` falls through to
# $FIRNLIB/window/window.fi, whose `window.backend` is found next to the
# root file -- the Android one.
#
#   shadow.py <entry.fi> <out dir> <android backend .fi>   -> prints the root
import os, sys
entry, out, backend = (os.path.abspath(a) for a in sys.argv[1:4])
edir = os.path.dirname(entry)
root = edir
d = edir
while True:
    if os.path.exists(os.path.join(d, "firn.package")):
        root = d
        break
    up = os.path.dirname(d)
    if up == d:
        break
    d = up
SKIP = {".git", "build", "bin", ".work"}
FIRNLIB = os.environ.get("FIRNLIB", "")

def vendored_std(p):
    # p lies below <something>/vendor/std
    parts = p.split(os.sep)
    return "vendor" in parts and "std" in parts[parts.index("vendor") + 1:]

def lib_has(p):
    parts = p.split(os.sep)
    i = len(parts) - 1 - parts[::-1].index("std")
    return bool(FIRNLIB) and os.path.exists(os.path.join(FIRNLIB, *parts[i:]))

def platform_window(p):
    return os.path.basename(p) == "window" and os.path.exists(os.path.join(p, "backend.fi"))

def needs_descent(p):
    # the entry directory lies below, or a platform window layer does
    if edir == p or edir.startswith(p + os.sep):
        return True
    for top, dirs, _ in os.walk(p):
        dirs[:] = [x for x in dirs if x not in SKIP]
        for x in dirs:
            if platform_window(os.path.join(top, x)):
                return True
    return False

def mirror(src, dst):
    os.makedirs(dst, exist_ok=True)
    for name in sorted(os.listdir(src)):
        s = os.path.join(src, name)
        t = os.path.join(dst, name)
        if name in SKIP and src == root:
            continue
        if os.path.isdir(s) and not os.path.islink(s):
            if platform_window(s):
                print("  replaced platform window layer:", os.path.relpath(s, root), file=sys.stderr)
                continue
            if needs_descent(s) or (name == "vendor" and os.path.isdir(os.path.join(s, "std"))) or vendored_std(s):
                mirror(s, t)
                continue
        if vendored_std(s) and not os.path.isdir(s) and lib_has(s):
            print("  replaced vendored library copy:", os.path.relpath(s, root), file=sys.stderr)
            continue
        os.symlink(s, t)

mirror(root, out)
rel = os.path.relpath(entry, root)
wdir = os.path.join(out, os.path.dirname(rel), "window")
os.makedirs(wdir, exist_ok=True)
for name in (os.listdir(os.path.join(edir, "window")) if os.path.isdir(os.path.join(edir, "window")) else []):
    if name != "backend.fi":
        os.symlink(os.path.join(edir, "window", name), os.path.join(wdir, name))
os.symlink(backend, os.path.join(wdir, "backend.fi"))
print(os.path.join(out, rel))
