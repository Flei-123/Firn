#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/portald/portald_check.py -- orient-portald (the Level 2 spike, docs/DIALOG.md) against clients nobody here wrote.

    python3 tools/portald/portald_check.py <portald binary>

A private session bus (dbus-daemon). portald owns org.freedesktop.Notifications and org.freedesktop.portal.Desktop on it. The
clients are dbus-python (libdbus), `gdbus` (GLib) and `dbus-send`: Notify with every kind of argument, replacing, the
NotificationClosed signal, GetCapabilities / GetServerInformation, FileChooser.OpenFile / SaveFile with options (multiple,
directory, current_folder, current_name, filters) and the Request.Response signal, Settings.Read / ReadOne / ReadAll.
The dialog behind the file chooser is std.dialog's with a STAND-IN dialog service (a script that records the request line and
answers from a file), so what the portal passes on is checked exactly; the real dialogs are tools/dialog/dialog_check.py's.
"""
import os, shutil, subprocess, sys, tempfile, threading, time

if len(sys.argv) < 2:
    print("usage: portald_check.py <portald>"); sys.exit(2)
PORTALD = os.path.abspath(sys.argv[1])
try:
    import dbus, dbus.bus
    from dbus.mainloop.glib import DBusGMainLoop
    from gi.repository import GLib
except ImportError as e:
    print("  SKIP  %s: portald was not checked here" % e); sys.exit(0)
if not shutil.which("dbus-daemon"):
    print("  SKIP  dbus-daemon missing"); sys.exit(0)

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

td = tempfile.mkdtemp(prefix="portald-")
toast = os.path.join(td, "toasts.txt")
req = os.path.join(td, "req.txt"); ans = os.path.join(td, "ans.txt")
stub = os.path.join(td, "dialogd")
open(stub, "w").write("#!/bin/sh\n[ \"$1\" = \"--stdio\" ] || exit 2\nIFS= read -r line\nprintf '%s\\n' \"$line\" > \"" + req + "\"\ncat \"" + ans + "\"\n")
os.chmod(stub, 0o755)
def answer(text):
    open(ans, "w").write(text)
def last_request():
    return open(req).read()

DBusGMainLoop(set_as_default=True)
daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], stdout=subprocess.PIPE, text=True)
address = daemon.stdout.readline().strip()
env = dict(os.environ, DBUS_SESSION_BUS_ADDRESS=address, DISPLAY=":77", FIRN_DIALOG="service", FIRN_DIALOG_SERVICE=stub)
env.pop("WAYLAND_DISPLAY", None)

def start_portald(*args):
    p = subprocess.Popen([PORTALD, "--toast-file", toast] + list(args), env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    line = p.stdout.readline().strip()
    return p, line

pd, line = start_portald()
check("portald starts and says READY", line == "READY", (line, pd.poll()))

bus = dbus.bus.BusConnection(address)
loop = GLib.MainLoop()
threading.Thread(target=loop.run, daemon=True).start()
closed, responses = [], []
bus.add_signal_receiver(lambda i, why: closed.append((int(i), int(why))), signal_name="NotificationClosed", dbus_interface="org.freedesktop.Notifications")
bus.add_signal_receiver(lambda code, res, path=None: responses.append((int(code), res, path)), signal_name="Response",
                        dbus_interface="org.freedesktop.portal.Request", path_keyword="path")
time.sleep(0.3)

# ---- org.freedesktop.Notifications
nobj = bus.get_object("org.freedesktop.Notifications", "/org/freedesktop/Notifications")
ni = dbus.Interface(nobj, "org.freedesktop.Notifications")
check("GetCapabilities", [str(c) for c in ni.GetCapabilities()] == ["body", "actions", "persistence"], ni.GetCapabilities())
check("GetServerInformation", [str(x) for x in ni.GetServerInformation()] == ["OrientOS toast", "Fleitec", "1", "1.2"], ni.GetServerInformation())
i1 = int(ni.Notify("Firefox", 0, "icon", "Summary äö 日本", "Body\nline two\ttab", ["default", "Open"],  # english: ok
                   {"urgency": dbus.Byte(2), "category": "x", "n": dbus.Int32(5), "arr": dbus.Array([1, 2], signature="i")}, 5000,
                   signature="susssasa{sv}i"))
i2 = int(ni.Notify("Other", 0, "", "Second", "", [], {}, -1, signature="susssasa{sv}i"))
i3 = int(ni.Notify("Firefox", i1, "", "Replaced", "again", [], {}, 0, signature="susssasa{sv}i"))
check("ids count up, replacing keeps the id", (i1, i2, i3) == (1, 2, 1), (i1, i2, i3))
lines = open(toast).read().split("\n")[:-1]
check("three toasts in the sink", len(lines) == 3, lines)
if len(lines) == 3:
    f = lines[0].split("\t")
    check("first toast: id, urgency 2 (a byte hint), app, summary and body with TAB / line feed escaped, UTF-8",
          f == ["TOAST", "1", "2", "Firefox", "Summary äö 日本", "Body\\nline two\\ttab"], f)  # english: ok
    check("second toast: no urgency hint is 1", lines[1].split("\t")[:3] == ["TOAST", "2", "1"], lines[1])
    check("third toast replaced id 1", lines[2].split("\t")[:2] == ["TOAST", "1"] and lines[2].split("\t")[4] == "Replaced", lines[2])
ni.CloseNotification(dbus.UInt32(2), signature="u")
time.sleep(0.4)
check("CloseNotification answers with the signal NotificationClosed(2, 3)", (2, 3) in closed, closed)

# a foreign client: gdbus (GLib's own D-Bus) and dbus-send
if shutil.which("gdbus"):
    r = subprocess.run(["gdbus", "call", "--session", "--dest", "org.freedesktop.Notifications", "--object-path", "/org/freedesktop/Notifications",
                        "--method", "org.freedesktop.Notifications.Notify", "gdbus-app", "0", "", "From gdbus", "hello", "[]", "{}", "5000"],
                       env=env, capture_output=True, text=True, timeout=20)
    check("gdbus: Notify works and returns an id", r.returncode == 0 and r.stdout.strip() == "(uint32 3,)", (r.stdout, r.stderr))
    check("gdbus: the toast reached the sink", "From gdbus" in open(toast).read())
else:
    print("  SKIP  gdbus not installed")
r = subprocess.run(["dbus-send", "--session", "--print-reply", "--dest=org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                    "org.freedesktop.Notifications.GetServerInformation"], env=env, capture_output=True, text=True, timeout=20)
check("dbus-send: GetServerInformation", r.returncode == 0 and "OrientOS toast" in r.stdout, (r.stdout, r.stderr))
r = subprocess.run(["dbus-send", "--session", "--print-reply", "--dest=org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                    "org.freedesktop.Notifications.NoSuchMethod"], env=env, capture_output=True, text=True, timeout=20)
check("an unknown method is an error, not a hang", r.returncode != 0 and "UnknownMethod" in r.stderr, (r.stdout, r.stderr))

# ---- org.freedesktop.portal.FileChooser
pobj = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
fc = dbus.Interface(pobj, "org.freedesktop.portal.FileChooser")
def wait_response(n, timeout=8):
    end = time.time() + timeout
    while time.time() < end and len(responses) < n:
        time.sleep(0.05)
    return responses[n - 1] if len(responses) >= n else None

answer("ok\t/tmp/a b.png\\n/tmp/ü.png\n")  # english: ok
h = fc.OpenFile("x11:123", "Pick pictures", {"handle_token": "tok1", "multiple": True, "current_folder": dbus.ByteArray(b"/tmp/x\0"),
                "filters": dbus.Array([("Pics", dbus.Array([(dbus.UInt32(0), "*.png"), (dbus.UInt32(0), "*.jpg"), (dbus.UInt32(1), "image/gif")], signature="(us)")),
                                       ("All", dbus.Array([(dbus.UInt32(0), "*")], signature="(us)"))], signature="(sa(us))")}, signature="ssa{sv}")
unique = bus.get_unique_name().replace(":", "").replace(".", "_")
check("OpenFile returns the Request path /request/<sender>/<token>", str(h) == "/org/freedesktop/portal/desktop/request/%s/tok1" % unique, str(h))
rsp = wait_response(1)
check("Response arrives with code 0", rsp is not None and rsp[0] == 0, rsp)
if rsp:
    check("results: uris as file:// URIs, percent-encoded, two files", [str(u) for u in rsp[1]["uris"]] == ["file:///tmp/a%20b.png", "file:///tmp/%C3%BC.png"], rsp[1])
    check("the Response came on the Request object", str(rsp[2]) == str(h), rsp[2])
check("the dialog request: open_files, title, folder, first filter's globs only",
      last_request() == "open_files\tPick pictures\t/tmp/x\tPics|*.png;*.jpg\n", last_request())

answer("cancel\n")
fc.OpenFile("", "Pick one", {"handle_token": "tok2"}, signature="ssa{sv}")
rsp = wait_response(2)
check("a cancelled dialog is Response code 1 with no results", rsp is not None and rsp[0] == 1 and len(rsp[1]) == 0, rsp)
check("a single file: open_file with empty folder and no filters", last_request() == "open_file\tPick one\t\t\n", last_request())

answer("ok\t/home/x/new.txt\n")
fc.SaveFile("", "Save as", {"handle_token": "tok3", "current_name": "draft.txt", "current_folder": dbus.ByteArray(b"/home/x\0")}, signature="ssa{sv}")
rsp = wait_response(3)
check("SaveFile: uris of the chosen name", rsp is not None and rsp[0] == 0 and [str(u) for u in rsp[1]["uris"]] == ["file:///home/x/new.txt"], rsp)
check("SaveFile: save_file with folder and the suggested name", last_request() == "save_file\tSave as\t/home/x\tdraft.txt\t\n", last_request())

answer("ok\t/srv/data\n")
fc.OpenFile("", "Folder", {"handle_token": "tok4", "directory": True}, signature="ssa{sv}")
rsp = wait_response(4)
check("directory: pick_folder", rsp is not None and rsp[0] == 0 and last_request() == "pick_folder\tFolder\t\n", (rsp, last_request()))

answer("failed\n")
fc.OpenFile("", "Broken", {"handle_token": "tok5"}, signature="ssa{sv}")
rsp = wait_response(5)
check("a dialog that failed is Response code 2", rsp is not None and rsp[0] == 2, rsp)

# ---- org.freedesktop.portal.Settings
st = dbus.Interface(pobj, "org.freedesktop.portal.Settings")
check("ReadOne color-scheme: light = 2", int(st.ReadOne("org.freedesktop.appearance", "color-scheme", signature="ss")) == 2)
check("Read color-scheme (the old, double wrapped form): the same value", int(st.Read("org.freedesktop.appearance", "color-scheme", signature="ss")) == 2)
acc = st.ReadOne("org.freedesktop.appearance", "accent-color", signature="ss")
check("accent-color is the launcher green (0.106, 0.851, 0.416)", [round(float(x), 3) for x in acc] == [0.106, 0.851, 0.416], list(acc))
allv = st.ReadAll(["org.freedesktop.appearance"], signature="as")
check("ReadAll has both keys", set(allv["org.freedesktop.appearance"].keys()) == {"color-scheme", "accent-color"}, allv)
try:
    st.ReadOne("org.example.nothing", "x", signature="ss"); check("an unknown namespace is an error", False)
except dbus.DBusException as e:
    check("an unknown namespace is an error (NotFound)", "NotFound" in str(e), str(e))
r = subprocess.run(["dbus-send", "--session", "--print-reply", "--dest=org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
                    "org.freedesktop.portal.Settings.ReadOne", "string:org.freedesktop.appearance", "string:color-scheme"], env=env, capture_output=True, text=True, timeout=20)
check("dbus-send: ReadOne color-scheme", r.returncode == 0 and "uint32 2" in r.stdout, (r.stdout, r.stderr))

# ---- --dark
pd.terminate(); pd.wait()
pd2, line2 = start_portald("--dark")
time.sleep(0.3)
check("--dark: color-scheme 1", int(dbus.Interface(bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop"), "org.freedesktop.portal.Settings").ReadOne("org.freedesktop.appearance", "color-scheme", signature="ss")) == 1)
# a second portald cannot take the names
pd3 = subprocess.run([PORTALD], env=env, capture_output=True, text=True, timeout=20)
check("a second instance refuses: the names are taken", pd3.returncode == 1 and "owned by another" in pd3.stderr, (pd3.returncode, pd3.stderr))
pd2.terminate(); pd2.wait()
daemon.terminate()
shutil.rmtree(td, ignore_errors=True)
print("portald: %d failed" % len(FAILED) if FAILED else "portald: all checks passed")
sys.exit(1 if FAILED else 0)
