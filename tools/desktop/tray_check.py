#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/tray_check.py -- the StatusNotifierItem tray (lib/desktop/tray.fi)
# against an implementation nobody here wrote: libdbus through dbus-python plays
# the desktop (a StatusNotifierWatcher, a property reader and a dbusmenu client)
# on a private bus.
#
#   usage: tray_check.py <tray_main binary>
import os, subprocess, sys, threading, time, signal
import dbus, dbus.service, dbus.bus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

binary = sys.argv[1]
DBusGMainLoop(set_as_default=True)

daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"],
                          stdout=subprocess.PIPE, text=True)
address = daemon.stdout.readline().strip()
os.environ["DBUS_SESSION_BUS_ADDRESS"] = address

registered = []
signals = []

class Watcher(dbus.service.Object):
    def __init__(self, bus):
        bus.request_name("org.kde.StatusNotifierWatcher")
        dbus.service.Object.__init__(self, bus, "/StatusNotifierWatcher")

    @dbus.service.method("org.kde.StatusNotifierWatcher", in_signature="s", out_signature="")
    def RegisterStatusNotifierItem(self, service):
        registered.append(str(service))

try:
    host = dbus.bus.BusConnection(address)  # runs on the GLib default context
    loop = GLib.MainLoop()
    t = threading.Thread(target=loop.run, daemon=True)
    # Without a watcher: the program must say SHOWN no and keep serving.
    out_lines = []
    proc = subprocess.Popen([binary], stdout=subprocess.PIPE, text=True, bufsize=1)
    first = proc.stdout.readline().strip()
    check("without a watcher the tray reports SHOWN no", first == "SHOWN no", first)
    # Now a watcher appears: the program registers by itself (NameOwnerChanged).
    t.start()
    w = Watcher(host)
    host.add_signal_receiver(lambda *a: signals.append(("LayoutUpdated",) + tuple(a)),
                             signal_name="LayoutUpdated", dbus_interface="com.canonical.dbusmenu")
    host.add_signal_receiver(lambda *a: signals.append(("NewIcon",)),
                             signal_name="NewIcon", dbus_interface="org.kde.StatusNotifierItem")
    deadline = time.time() + 6
    while not registered and time.time() < deadline:
        time.sleep(0.05)
    check("the item registers when a watcher appears later", len(registered) == 1, registered)
    svc = registered[0] if registered else ""
    check("service name is org.kde.StatusNotifierItem-<pid>-1",
          svc == "org.kde.StatusNotifierItem-%d-1" % proc.pid, svc)

    client = dbus.bus.BusConnection(address)  # blocking calls only
    obj = client.get_object(svc, "/StatusNotifierItem")
    props = dbus.Interface(obj, "org.freedesktop.DBus.Properties")
    IF = "org.kde.StatusNotifierItem"
    allp = props.GetAll(IF)
    check("Id", str(allp["Id"]) == "org.firn.traytest")
    check("Title", str(allp["Title"]) == "Firn tray test")
    check("Category", str(allp["Category"]) == "ApplicationStatus")
    check("Status", str(allp["Status"]) == "Active")
    check("IconName", str(allp["IconName"]) == "mail-unread")
    check("Menu path", str(allp["Menu"]) == "/MenuBar")
    check("ItemIsMenu false", bool(allp["ItemIsMenu"]) is False)
    pm = allp["IconPixmap"]
    check("IconPixmap has one picture", len(pm) == 1, len(pm))
    if len(pm) == 1:
        w_, h_, data = int(pm[0][0]), int(pm[0][1]), bytes(bytearray(pm[0][2]))
        check("IconPixmap 4x2", (w_, h_) == (4, 2), (w_, h_))
        # ARGB32, network order: A, R, G, B
        check("pixel 0 is opaque red (A R G B)", data[0:4] == bytes([255, 255, 0, 0]), list(data[0:4]))
        check("pixel 2 is opaque blue", data[8:12] == bytes([255, 0, 0, 255]), list(data[8:12]))
        check("pixel 4 is half-transparent red", data[16:20] == bytes([128, 255, 0, 0]), list(data[16:20]))
        check("pixel data is w*h*4 octets", len(data) == 32, len(data))
    tip = allp["ToolTip"]
    check("ToolTip title and text", str(tip[2]) == "Firn" and str(tip[3]) == "two new messages", tuple(tip))
    check("Get a single property", str(props.Get(IF, "Id")) == "org.firn.traytest")
    try:
        props.Get(IF, "Nope")
        check("unknown property is an error", False)
    except dbus.DBusException as e:
        check("unknown property is an error", "UnknownProperty" in e.get_dbus_name(), e.get_dbus_name())
    intro = dbus.Interface(obj, "org.freedesktop.DBus.Introspectable").Introspect()
    check("Introspect lists the interface", "org.kde.StatusNotifierItem" in str(intro))

    # --- the menu, as the desktop reads it
    menu = dbus.Interface(client.get_object(svc, "/MenuBar"), "com.canonical.dbusmenu")
    mprops = dbus.Interface(client.get_object(svc, "/MenuBar"), "org.freedesktop.DBus.Properties").GetAll("com.canonical.dbusmenu")
    check("dbusmenu Version 3", int(mprops["Version"]) == 3)
    rev, layout = menu.GetLayout(0, -1, [])
    check("layout root id 0", int(layout[0]) == 0)
    check("root has children-display submenu", str(layout[1].get("children-display")) == "submenu")
    kids = [c for c in layout[2]]
    check("five children", len(kids) == 5, len(kids))
    ids = [int(k[0]) for k in kids]
    labels = [str(k[1].get("label", "")) for k in kids]
    check("child ids in order (separator id from the top range)", ids[0] == 1 and ids[2] == 3 and ids[3] == 4 and ids[4] == 2 and ids[1] > 2000000000, ids)
    check("labels", labels == ["Open", "", "Mute", "Gray", "Quit"], labels)
    check("separator has type separator", str(kids[1][1].get("type")) == "separator")
    check("Gray is disabled", kids[3][1].get("enabled") is not None and bool(kids[3][1]["enabled"]) is False)
    check("Mute has a check mark, unchecked", str(kids[2][1].get("toggle-type")) == "checkmark" and int(kids[2][1]["toggle-state"]) == 0)
    check("Open carries no enabled=false", kids[0][1].get("enabled") is None)
    gp = menu.GetGroupProperties([3], [])
    check("GetGroupProperties for one id", len(gp) == 1 and int(gp[0][0]) == 3 and str(gp[0][1]["label"]) == "Mute", gp)
    r2, sub = menu.GetLayout(1, 0, [])
    check("GetLayout of one item", int(sub[0]) == 1 and str(sub[1]["label"]) == "Open" and len(sub[2]) == 0)
    try:
        menu.GetLayout(99, 0, [])
        check("GetLayout of an unknown id is an error", False)
    except dbus.DBusException:
        check("GetLayout of an unknown id is an error", True)
    check("AboutToShow answers false", bool(menu.AboutToShow(0)) is False)

    # --- clicks the way a desktop sends them; the program prints one EV line each
    obj_i = dbus.Interface(obj, IF)
    obj_i.Activate(11, 22)
    obj_i.SecondaryActivate(3, 4)
    obj_i.ContextMenu(5, 6)
    obj_i.Scroll(-120, "vertical")
    menu.Event(1, "clicked", dbus.String("", variant_level=1), dbus.UInt32(0))
    menu.Event(1, "hovered", dbus.String("", variant_level=1), dbus.UInt32(0))   # not a click: no event
    menu.Event(3, "clicked", dbus.String("", variant_level=1), dbus.UInt32(0))   # Mute: the program toggles it
    time.sleep(0.6)
    rev2, layout2 = menu.GetLayout(0, -1, [])
    k2 = [c for c in layout2[2]]
    check("after the click: revision grew", int(rev2) > int(rev), (int(rev), int(rev2)))
    check("Mute is now checked", int(k2[2][1]["toggle-state"]) == 1)
    check("label of item 1 was changed", str(k2[0][1]["label"]) == "Open now", str(k2[0][1]["label"]))
    check("LayoutUpdated signals arrived", any(s[0] == "LayoutUpdated" for s in signals), signals)
    # the Peer interface
    peer = dbus.Interface(obj, "org.freedesktop.DBus.Peer")
    peer.Ping()
    check("Peer.Ping", True)
    menu.Event(2, "clicked", dbus.String("", variant_level=1), dbus.UInt32(0))   # Quit
    try:
        rest = proc.communicate(timeout=10)[0]
    except subprocess.TimeoutExpired:
        proc.kill()
        rest = proc.communicate()[0]
    evs = [l.split() for l in rest.splitlines() if l.startswith("EV ")]
    got = [tuple(int(x) for x in e[1:]) for e in evs]
    check("event Activate (1, 0, 11, 22)", (1, 0, 11, 22) in got, got)
    check("event SecondaryActivate (2, 0, 3, 4)", (2, 0, 3, 4) in got, got)
    check("event ContextMenu (4, 0, 5, 6)", (4, 0, 5, 6) in got, got)
    check("event Scroll (5, 0, -120, 0)", (5, 0, -120, 0) in got, got)
    check("menu click item 1 (3, 1)", (3, 1, 0, 0) in got, got)
    check("hovered is not an event", got.count((3, 1, 0, 0)) == 1, got)
    check("menu click item 3 (Mute) and 2 (Quit)", (3, 3, 0, 0) in got and (3, 2, 0, 0) in got, got)
    check("events arrive in order", got.index((1, 0, 11, 22)) < got.index((2, 0, 3, 4)) < got.index((4, 0, 5, 6)) < got.index((5, 0, -120, 0)) < got.index((3, 1, 0, 0)), got)
    check("program says VISIBLE yes at the end", "VISIBLE yes" in rest, rest)
    check("program exited 0", proc.returncode == 0, proc.returncode)
finally:
    try:
        proc.kill()
    except Exception:
        pass
    daemon.terminate()

print("tray: %d failed" % len(FAILED) if FAILED else "tray: all checks passed")
sys.exit(1 if FAILED else 0)
