#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""AT-SPI client for tools/fui/atspiset_main.fi: Value.CurrentValue set and EditableText on a screen-mode tree.
usage: atspiset_test.py <bus-address>"""
import sys
import time

import dbus
import dbus.bus

ACC = "org.a11y.atspi.Accessible"
ACT = "org.a11y.atspi.Action"
VAL = "org.a11y.atspi.Value"
TEXT = "org.a11y.atspi.Text"
ETEXT = "org.a11y.atspi.EditableText"
PROPS = "org.freedesktop.DBus.Properties"

bad = 0


def check(ok, what):
    global bad
    if not ok:
        bad += 1
    print(("  ok    " if ok else "  WRONG ") + what)
    sys.stdout.flush()


class Client:
    def __init__(self, addr):
        self.bus = dbus.bus.BusConnection(addr)
        me = self.bus.get_unique_name()
        cand = [str(n) for n in self.bus.list_names() if str(n).startswith(":") and str(n) != me]
        if not cand:
            raise SystemExit("no application on the bus")
        self.app = cand[0]

    def obj(self, path):
        return self.bus.get_object(self.app, path, introspect=False)

    def call(self, path, iface, member, *args):
        return self.obj(path).get_dbus_method(member, iface)(*args)

    def get(self, path, iface, name):
        return self.obj(path).Get(iface, name, dbus_interface=PROPS)

    def set(self, path, iface, name, value):
        return self.obj(path).Set(iface, name, value, dbus_interface=PROPS, signature="ssv")

    def ifaces(self, path):
        return [str(i) for i in self.call(path, ACC, "GetInterfaces")]

    def find(self, name):
        frame = str(self.call("/org/a11y/atspi/accessible/root", ACC, "GetChildren")[0][1])
        for k in self.call(frame, ACC, "GetChildren"):
            p = str(k[1])
            if str(self.get(p, ACC, "Name")) == name:
                return p
        raise SystemExit("node not found: " + name)


def until(fn, secs=3.0):
    end = time.time() + secs
    while time.time() < end:
        if fn():
            return True
        time.sleep(0.05)
    return fn()


def refused(fn):
    try:
        return not bool(fn())
    except dbus.exceptions.DBusException:
        return True


def main():
    c = Client(sys.argv[1])
    size = c.find("Size")
    name = c.find("Name")
    note = c.find("Note text")
    prev = c.find("Preview")
    plain = c.find("Plain")
    quit_ = c.find("Quit")

    check(VAL in c.ifaces(size), "slider offers Value")
    check(float(c.get(size, VAL, "CurrentValue")) == 200.0, "slider reads per mille")
    c.set(size, VAL, "CurrentValue", dbus.Double(500.0))
    check(until(lambda: float(c.get(size, VAL, "CurrentValue")) == 500.0), "Set CurrentValue 500 lands")
    c.set(size, VAL, "CurrentValue", dbus.Double(9000.0))
    check(until(lambda: float(c.get(size, VAL, "CurrentValue")) == 1000.0), "Set CurrentValue is clamped")
    check(refused(lambda: c.set(size, VAL, "MaximumValue", dbus.Double(5.0))), "other Value properties stay read only")
    check(refused(lambda: c.set(quit_, VAL, "CurrentValue", dbus.Double(5.0))), "Set on a button is refused")

    for n, label in ((name, "field"), (note, "editor"), (prev, "preview"), (plain, "plain field")):
        check(TEXT in c.ifaces(n) and ETEXT in c.ifaces(n), label + " offers Text and EditableText")
    check(str(c.call(name, TEXT, "GetText", 0, -1)) == "abc", "field text is readable")
    check(str(c.call(note, TEXT, "GetText", 0, -1)) == "# Hi", "editor text is readable")
    check(int(c.get(note, TEXT, "CharacterCount")) == 4, "editor character count")
    check(bool(c.call(name, ETEXT, "SetTextContents", "renamed")), "field SetTextContents accepted")
    check(until(lambda: str(c.call(name, TEXT, "GetText", 0, -1)) == "renamed"), "field text changed")
    check(bool(c.call(name, ETEXT, "InsertText", 7, "!", 1)), "field InsertText accepted")
    check(until(lambda: str(c.call(name, TEXT, "GetText", 0, -1)) == "renamed!"), "field text has the insert")
    check(bool(c.call(name, ETEXT, "DeleteText", 7, 8)), "field DeleteText accepted")
    check(until(lambda: str(c.call(name, TEXT, "GetText", 0, -1)) == "renamed"), "field text lost the char")
    check(bool(c.call(note, ETEXT, "SetTextContents", "set by reader")), "editor SetTextContents accepted")
    check(until(lambda: str(c.call(note, TEXT, "GetText", 0, -1)) == "set by reader"), "editor text changed")
    check(str(c.call(prev, TEXT, "GetText", 0, -1)) == "set by reader", "preview follows the editor text")
    check(refused(lambda: c.call(prev, ETEXT, "SetTextContents", "x")), "preview is read only")
    check(refused(lambda: c.call(plain, ETEXT, "SetTextContents", "x")), "unbound field is read only")

    got = []
    from dbus.mainloop.glib import DBusGMainLoop
    from gi.repository import GLib
    ebus = dbus.bus.BusConnection(sys.argv[1], mainloop=DBusGMainLoop(set_as_default=False))

    def on_sig(*args, **kw):
        got.append((str(kw["path"]), str(kw["member"]), args))

    ebus.add_signal_receiver(on_sig, dbus_interface="org.a11y.atspi.Event.Object", path_keyword="path", member_keyword="member")

    def pump(sec):
        end = time.time() + sec
        ctx = GLib.MainContext.default()
        while time.time() < end:
            while ctx.iteration(False):
                pass
            time.sleep(0.02)

    def seen(path, member, d1=None):
        return any(p == path and m == member and (d1 is None or int(a[1]) == d1) for p, m, a in got)

    pump(0.5)
    check(int(c.call(note, TEXT, "GetCaretOffset")) == 0, "editor caret starts at 0 after a text set")
    check(int(c.call(note, TEXT, "GetNSelections")) == 0, "editor has no selection")
    got[:] = []
    check(bool(c.call(note, TEXT, "SetCaretOffset", 4)), "editor SetCaretOffset accepted")
    check(until(lambda: int(c.call(note, TEXT, "GetCaretOffset")) == 4), "editor caret moved to 4")
    pump(0.8)
    check(seen(note, "TextCaretMoved", 4), "event: TextCaretMoved to 4")
    check(not bool(c.call(note, TEXT, "SetCaretOffset", 99)), "caret beyond the text is refused")
    got[:] = []
    check(bool(c.call(note, TEXT, "SetSelection", 0, 2, 7)), "editor SetSelection accepted")
    check(until(lambda: int(c.call(note, TEXT, "GetNSelections")) == 1), "editor reports one selection")
    sel = c.call(note, TEXT, "GetSelection", 0)
    check((int(sel[0]), int(sel[1])) == (2, 7), "selection range is 2..7")
    check(int(c.call(note, TEXT, "GetCaretOffset")) == 7, "caret sits at the selection end")
    pump(0.8)
    check(seen(note, "TextSelectionChanged"), "event: TextSelectionChanged")
    check(bool(c.call(note, TEXT, "RemoveSelection", 0)), "RemoveSelection accepted")
    check(until(lambda: int(c.call(note, TEXT, "GetNSelections")) == 0), "selection is gone")
    check(int(c.call(note, TEXT, "GetCaretOffset")) == 7, "caret stays after RemoveSelection")
    check(refused(lambda: c.call(prev, TEXT, "SetCaretOffset", 2)), "preview has no caret to set")
    check(int(c.call(prev, TEXT, "GetCaretOffset")) == 0, "preview caret reads 0")
    check(refused(lambda: c.call(plain, TEXT, "SetCaretOffset", 0)), "unbound field has no caret to set")
    check(refused(lambda: c.call(name, TEXT, "SetCaretOffset", 0)), "plain kit field has no caret to set")
    check(bool(c.call(note, ETEXT, "SetTextContents", "\u00e4\u00f6 x")), "editor takes a non-ASCII text")
    check(until(lambda: str(c.call(note, TEXT, "GetText", 0, -1)) == "\u00e4\u00f6 x"), "non-ASCII text is read back")
    check(bool(c.call(note, TEXT, "SetCaretOffset", 2)), "caret offset counts codepoints")
    check(until(lambda: int(c.call(note, TEXT, "GetCaretOffset")) == 2), "caret at codepoint 2")
    c.call(quit_, ACT, "DoAction", 0)
    print("atspiset client: %s" % ("all ok" if bad == 0 else "%d wrong" % bad))
    return 1 if bad else 0


sys.exit(main())
