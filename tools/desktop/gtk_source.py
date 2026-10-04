#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/gtk_source.py -- a REAL toolkit as the drag source: a GTK 3 window at (0,300)
# that offers the given files as text/uri-list when dragged with the left mouse button.
# Used by drop_check.py (the drag is driven with xdotool). Prints READY, "GET <target>" per request.
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, GLib

uris = sys.argv[1:]
win = Gtk.Window(type=Gtk.WindowType.POPUP)
win.set_default_size(200, 100)
win.move(0, 300)
box = Gtk.EventBox()
box.add(Gtk.Label(label="drag me"))
win.add(box)
box.drag_source_set(Gdk.ModifierType.BUTTON1_MASK,
                    [Gtk.TargetEntry.new("text/uri-list", 0, 0)], Gdk.DragAction.COPY)

def on_get(widget, ctx, data, info, time):
    print("GET", data.get_target().name(), flush=True)
    data.set_uris(uris)

def on_end(widget, ctx):
    print("END", flush=True)

box.connect("drag-data-get", on_get)
box.connect("drag-end", on_end)
win.connect("destroy", Gtk.main_quit)
win.show_all()
GLib.timeout_add(500, lambda: print("READY", flush=True) and False)
GLib.timeout_add(60000, Gtk.main_quit)
Gtk.main()
