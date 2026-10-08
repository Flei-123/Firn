#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/gtk_peer.py -- GTK 3's clipboard as "the other program" for clipboard_check.py.
#   set-text <file>       owns the CLIPBOARD with that file's text (UTF-8) for 25 s
#   set-png <file>        owns it with that picture (GTK serves it as image/png) for 25 s
#   get-text <outfile>    writes the text on the clipboard to <outfile> (UTF-8) and prints "TEXT <n>" or "NOTEXT"
#   get-png <outfile>     writes the picture on the clipboard re-encoded as PNG and prints "PNG <w> <h> <sha256 of RGBA>"
#                         or "NOPNG"
import hashlib, sys, time
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gtk, Gdk, GLib, GdkPixbuf

cb = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
mode = sys.argv[1]

def pump_wait(fn, seconds=8):
    end = time.time() + seconds
    r = None
    while time.time() < end and r is None:
        while Gtk.events_pending():
            Gtk.main_iteration()
        r = fn()
        if r is None:
            time.sleep(0.2)
    return r

def rgba(pb):
    if not pb.get_has_alpha():
        pb = pb.add_alpha(False, 0, 0, 0)
    px = bytes(pb.get_pixels())
    rs, w, h = pb.get_rowstride(), pb.get_width(), pb.get_height()
    data = b"".join(px[y * rs:y * rs + w * 4] for y in range(h))
    return w, h, hashlib.sha256(data).hexdigest()

if mode == "set-text":
    cb.set_text(open(sys.argv[2], encoding="utf-8").read(), -1)
    cb.store()
    print("READY", flush=True)
    GLib.timeout_add(25000, Gtk.main_quit)
    Gtk.main()
elif mode == "set-png":
    cb.set_image(GdkPixbuf.Pixbuf.new_from_file(sys.argv[2]))
    cb.store()
    print("READY", flush=True)
    GLib.timeout_add(25000, Gtk.main_quit)
    Gtk.main()
elif mode == "get-text":
    txt = pump_wait(cb.wait_for_text)
    if txt is None:
        print("NOTEXT")
    else:
        open(sys.argv[2], "w", encoding="utf-8", newline="").write(txt)
        print("TEXT", len(txt.encode("utf-8")))
elif mode == "get-png":
    pb = pump_wait(cb.wait_for_image)
    if pb is None:
        print("NOPNG")
    else:
        w, h, d = rgba(pb)
        print("PNG", w, h, d)
