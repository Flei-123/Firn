#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/dialog/toast_check.py -- lib/std/toast.fi (on lib/desktop/notify.fi) against a notification
# server written with libdbus (dbus-python) on a private bus.
#
#   usage: toast_check.py <toast_main binary>
import os, subprocess, sys, threading, time
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

calls = []
NEXT = [1]

class Server(dbus.service.Object):
    def __init__(self, bus):
        self.bus = bus
        bus.request_name("org.freedesktop.Notifications")
        dbus.service.Object.__init__(self, bus, "/org/freedesktop/Notifications")

    @dbus.service.method("org.freedesktop.Notifications", in_signature="susssasa{sv}i", out_signature="u")
    def Notify(self, app, replaces, icon, summary, body, actions, hints, timeout):
        calls.append(dict(app=str(app), replaces=int(replaces), icon=str(icon), summary=str(summary),
                          body=str(body), actions=[str(a) for a in actions],
                          hints={str(k): v for k, v in hints.items()}, timeout=int(timeout)))
        if int(replaces):
            return dbus.UInt32(int(replaces))
        i = NEXT[0]; NEXT[0] += 1
        return dbus.UInt32(i)

    @dbus.service.method("org.freedesktop.Notifications", in_signature="u", out_signature="")
    def CloseNotification(self, i):
        calls.append(dict(close=int(i)))
        self.NotificationClosed(i, 3)

    @dbus.service.method("org.freedesktop.Notifications", in_signature="", out_signature="as")
    def GetCapabilities(self):
        return ["actions", "body", "persistence"]

    @dbus.service.method("org.freedesktop.Notifications", in_signature="", out_signature="ssss")
    def GetServerInformation(self):
        return ("FirnTestServer", "Firn", "1.2", "1.2")

    @dbus.service.signal("org.freedesktop.Notifications", signature="us")
    def ActionInvoked(self, i, key):
        pass

    @dbus.service.signal("org.freedesktop.Notifications", signature="uu")
    def NotificationClosed(self, i, why):
        pass

# --- 1. no server at all
r = subprocess.run([binary], capture_output=True, text=True, timeout=20)
check("without a server: READY no and ID 0", r.stdout.split() == ["READY", "no", "ID", "0"], r.stdout)
check("without a server: exit 0, poll and close answer false", r.returncode == 0, r.returncode)

# --- 2. with the server
host = dbus.bus.BusConnection(address)
srv = Server(host)
loop = GLib.MainLoop()
threading.Thread(target=loop.run, daemon=True).start()
time.sleep(0.2)

proc = subprocess.Popen([binary], stdout=subprocess.PIPE, text=True, bufsize=1)
lines = []
def read_until(tag, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        l = proc.stdout.readline()
        if not l:
            break
        lines.append(l.rstrip("\n"))
        if l.startswith(tag):
            return True
    return False

check("program reaches GO", read_until("GO"), lines)
check("READY yes", "READY yes" in lines, lines)
check("server name", "SERVER FirnTestServer" in lines, lines)
check("three capabilities", "CAPS 3" in lines, lines)
check("first ID is 1, replacing keeps 1", "ID 1" in lines and "ID2 1" in lines, lines)
check("close worked", "CLOSE ok" in lines, lines)
c = [x for x in calls if "summary" in x]
check("two Notify calls", len(c) == 2, calls)
if len(c) == 2:
    a, b = c
    check("app name from the options", a["app"] == "Firn toast test", a["app"])
    check("summary and UTF-8 body", a["summary"] == "Download finished" and a["body"] == "mod.jar, 4.2 MiB \u00e4\u00f6", repr(a["body"]))
    check("icon", a["icon"] == "emblem-downloads")
    check("actions as key/label pairs", a["actions"] == ["default", "Open", "open", "Show file"], a["actions"])
    check("urgency critical (byte 2)", int(a["hints"]["urgency"]) == 2 and isinstance(a["hints"]["urgency"], dbus.Byte), a["hints"])
    check("category", str(a["hints"]["category"]) == "transfer.complete")
    check("timeout 7000, replaces 0", a["timeout"] == 7000 and a["replaces"] == 0)
    check("second call replaces 1, timeout 0, no hints", b["replaces"] == 1 and b["timeout"] == 0 and len(b["hints"]) == 0, b)
check("CloseNotification arrived with id 1", {"close": 1} in calls, calls)

srv.ActionInvoked(1, "open")
srv.NotificationClosed(1, 2)
read_until("DONE")
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.kill()
lines += [l.rstrip("\n") for l in proc.stdout.read().splitlines()]
check("the callback saw the click, with its key and its user data", "ACTION 1 open ud=42" in lines, lines)
check("the dismissal came through poll (kind 2, id 1, reason 2)", "EVENT 2 1 2" in lines, lines)
check("the close by call did not reach the callback", not any(l.startswith("ACTION") and "ud=42" not in l for l in lines), lines)
check("DONE and exit 0", "DONE" in lines and proc.returncode == 0, (lines, proc.returncode))
daemon.terminate()
print("toast: %d failed" % len(FAILED) if FAILED else "toast: all checks passed")
sys.exit(1 if FAILED else 0)
