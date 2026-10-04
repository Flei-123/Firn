#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/gtk_clip.py -- GTK 3's clipboard as the other program: `set <text-file>` owns the CLIPBOARD
# with that text for 25 s; `get` prints what is on it (text, the uri list, the target names).
import sys, time
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, GLib
cb = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if sys.argv[1] == "set":
    text = open(sys.argv[2], encoding="utf-8").read()
    cb.set_text(text, -1)
    cb.store()
    print("READY", flush=True)
    GLib.timeout_add(25000, Gtk.main_quit)
    Gtk.main()
else:
    end = time.time() + 8
    txt = None
    while time.time() < end and txt is None:
        while Gtk.events_pending():
            Gtk.main_iteration()
        txt = cb.wait_for_text()
        time.sleep(0.2)
    uris = cb.wait_for_uris()
    ok, targets = cb.wait_for_targets()
    print("TEXT", repr(txt))
    print("URIS", list(uris) if uris else None)
    print("TARGETS", sorted(t.name() for t in targets) if ok else None)
