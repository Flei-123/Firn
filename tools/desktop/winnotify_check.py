#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/winnotify_check.py -- window.notify on X11: a Firn window under Xvfb posts a notification to a
# notification server (dbus-python) on a private bus. usage: winnotify_check.py <winnotify_main binary>
import os, subprocess, sys, threading, time
from tscale import T, scratch_dir
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
daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], stdout=subprocess.PIPE, text=True)
address = daemon.stdout.readline().strip()
xvfb = subprocess.Popen(["Xvfb", ":97", "-screen", "0", "400x300x24"], stderr=subprocess.DEVNULL)
time.sleep(1.0)
env = dict(os.environ, DISPLAY=":97", DBUS_SESSION_BUS_ADDRESS=address)
calls = []

class Server(dbus.service.Object):
    def __init__(self, bus):
        bus.request_name("org.freedesktop.Notifications")
        dbus.service.Object.__init__(self, bus, "/org/freedesktop/Notifications")
    @dbus.service.method("org.freedesktop.Notifications", in_signature="susssasa{sv}i", out_signature="u")
    def Notify(self, app, replaces, icon, summary, body, actions, hints, timeout):
        calls.append(dict(app=str(app), summary=str(summary), body=str(body)))
        return dbus.UInt32(1)
    @dbus.service.method("org.freedesktop.Notifications", in_signature="", out_signature="as")
    def GetCapabilities(self):
        return ["body"]
    @dbus.service.method("org.freedesktop.Notifications", in_signature="", out_signature="ssss")
    def GetServerInformation(self):
        return ("FirnTestServer", "Firn", "1.2", "1.2")

try:
    r = subprocess.run([binary], capture_output=True, text=True, timeout=T(30), env=env)
    check("without a server: NOTIFY no", "NOTIFY no" in r.stdout, r.stdout + r.stderr)
    check("stay_alive honestly false", "ALIVE no" in r.stdout, r.stdout)
    host = dbus.bus.BusConnection(address)
    srv = Server(host)
    loop = GLib.MainLoop()
    threading.Thread(target=loop.run, daemon=True).start()
    time.sleep(0.2)
    r = subprocess.run([binary], capture_output=True, text=True, timeout=T(30), env=env)
    check("with a server: NOTIFY yes", "NOTIFY yes" in r.stdout, r.stdout + r.stderr)
    check("one Notify call", len(calls) == 1, calls)
    if calls:
        check("summary", calls[0]["summary"] == "Hello from a window", calls[0])
        check("body (UTF-8)", calls[0]["body"] == "body äö", repr(calls[0]["body"]))
finally:
    xvfb.terminate(); daemon.terminate()
print("RESULT winnotify: " + ("FAIL" if FAILED else "ok"))
sys.exit(1 if FAILED else 0)
