#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/dbus_check.py -- lib/net/dbus.fi against libdbus (dbus-python):
# every basic and container type in both directions, a 1 MiB message, errors,
# unknown methods, and the queue (a signal that arrives while a call waits).
#
#   usage: dbus_check.py <dbus_main binary>
import os, subprocess, sys, threading, time
import dbus, dbus.service, dbus.bus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

binary = sys.argv[1]
DBusGMainLoop(set_as_default=True)
daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"],
                          stdout=subprocess.PIPE, text=True)
address = daemon.stdout.readline().strip()
os.environ["DBUS_SESSION_BUS_ADDRESS"] = address

class Py(dbus.service.Object):
    def __init__(self, bus):
        bus.request_name("org.firn.PyService")
        dbus.service.Object.__init__(self, bus, "/org/firn/Py")
    @dbus.service.method("org.firn.Py", in_signature="", out_signature="")
    def Slow(self):
        self.Tick()
        time.sleep(0.2)
    @dbus.service.signal("org.firn.PySig", signature="")
    def Tick(self):
        pass

host = dbus.bus.BusConnection(address)
py = Py(host)
loop = GLib.MainLoop()
threading.Thread(target=loop.run, daemon=True).start()

proc = subprocess.Popen([binary], stdout=subprocess.PIPE, text=True, bufsize=1)
first = proc.stdout.readline().strip()
check("driver is ready", first == "READY", first)

client = dbus.bus.BusConnection(address)
obj = client.get_object("org.firn.Rich", "/org/firn/Rich")
IF = "org.firn.Rich"

BYTES = dbus.Array([dbus.Byte(i) for i in range(10)], signature="y")
DICT = dbus.Dictionary({"k1": dbus.String("v1", variant_level=1),
                        "k2": dbus.Int32(-5, variant_level=1),
                        "k3": dbus.Array([dbus.Byte(7), dbus.Byte(8)], signature="y", variant_level=1)},
                       signature="sv")
ARGS = (dbus.Byte(200), dbus.Boolean(True), dbus.Int16(-1234), dbus.UInt16(65535),
        dbus.Int32(-2000000000), dbus.UInt32(4000000000), dbus.Int64(-9000000000000000000),
        dbus.UInt64(18000000000000000000), dbus.Double(1.5), dbus.String("héllo"),
        dbus.ObjectPath("/org/firn/Obj"), dbus.Signature("a{sv}"), dbus.UInt32(77, variant_level=1),
        dbus.Array([dbus.Int32(1), dbus.Int32(-2), dbus.Int32(3)], signature="i"),
        BYTES, DICT, dbus.Struct((dbus.Byte(9), dbus.Int64(-9)), signature="yx"),
        dbus.Array([dbus.Struct((dbus.Byte(1), dbus.Int64(-1)), signature="yx"),
                    dbus.Struct((dbus.Byte(2), dbus.Int64(-2)), signature="yx")], signature="(yx)"))
SIG = "ybnqiuxtdsogvaiaya{sv}(yx)a(yx)"

# 1. libdbus marshals, Firn reads and compares every value itself
obj.get_dbus_method("Probe", IF)(*ARGS, signature=SIG)
# 2. Firn marshals, libdbus unmarshals
got = obj.get_dbus_method("Make", IF)()
check("Make: 18 values came back", len(got) == 18, len(got))
if len(got) == 18:
    check("y", int(got[0]) == 200)
    check("b", bool(got[1]) is True)
    check("n", int(got[2]) == -1234)
    check("q", int(got[3]) == 65535)
    check("i", int(got[4]) == -2000000000)
    check("u", int(got[5]) == 4000000000)
    check("x", int(got[6]) == -9000000000000000000)
    check("t", int(got[7]) == 18000000000000000000)
    check("d", float(got[8]) == 1.5)
    check("s (UTF-8)", str(got[9]) == "héllo")
    check("o", str(got[10]) == "/org/firn/Obj")
    check("g", str(got[11]) == "a{sv}")
    check("v", int(got[12]) == 77)
    check("ai", [int(x) for x in got[13]] == [1, -2, 3])
    check("ay", bytes(bytearray(got[14])) == bytes(range(10)))
    check("a{sv}", str(got[15]["k1"]) == "v1" and int(got[15]["k2"]) == -5 and bytes(bytearray(got[15]["k3"])) == bytes([7, 8]), got[15])
    check("(yx)", (int(got[16][0]), int(got[16][1])) == (9, -9))
    check("a(yx)", [(int(a), int(b)) for a, b in got[17]] == [(1, -1), (2, -2)])
# 3. the body comes back byte for byte (the reader skips by signature, the writer copies)
echo = obj.get_dbus_method("Echo", IF)(*ARGS, signature=SIG)
check("Echo gives the same 18 values", len(echo) == 18 and str(echo[9]) == "héllo" and int(echo[6]) == -9000000000000000000
      and float(echo[8]) == 1.5 and [(int(a), int(b)) for a, b in echo[17]] == [(1, -1), (2, -2)], echo)
# 4. a big message, in both directions at once (partial reads and writes)
big = bytes((i * 7 + 3) & 255 for i in range(1 << 20))
res = obj.get_dbus_method("Echo", IF)(dbus.Array(big, signature="y"), signature="ay")
check("1 MiB array round trip", bytes(bytearray(res)) == big, len(res))
# 5. variants within variants, nested containers
nested = dbus.Dictionary({"a": dbus.Dictionary({"b": dbus.Array([dbus.String("c", variant_level=1), dbus.Int32(4, variant_level=1)], signature="v", variant_level=1)},
                                              signature="sv", variant_level=1)}, signature="sv")
res = obj.get_dbus_method("Echo", IF)(nested, signature="a{sv}")
check("nested dict/array/variant round trip", str(res["a"]["b"][0]) == "c" and int(res["a"]["b"][1]) == 4, res)
# 6. empty arrays and empty strings (padding after the length)
res = obj.get_dbus_method("Echo", IF)(dbus.Array([], signature="(yx)"), dbus.String(""), dbus.Array([], signature="x"), dbus.Byte(5), signature="a(yx)saxy")
check("empty arrays and strings", len(res) == 4 and len(res[0]) == 0 and str(res[1]) == "" and len(res[2]) == 0 and int(res[3]) == 5, res)
# 7. errors
try:
    obj.get_dbus_method("Fail", IF)()
    check("error reply", False)
except dbus.DBusException as e:
    check("error name", e.get_dbus_name() == "org.firn.Error.Nope", e.get_dbus_name())
    check("error text", "that did not work" in str(e), str(e))
try:
    obj.get_dbus_method("NoSuchMethod", IF)()
    check("unknown method", False)
except dbus.DBusException as e:
    check("unknown method is an error", "UnknownMethod" in e.get_dbus_name(), e.get_dbus_name())
# 8. the queue: a signal that arrives while Firn waits for the reply of its own call
obj.get_dbus_method("Slow", IF)()
obj.get_dbus_method("Quit", IF)()
try:
    rest = proc.communicate(timeout=10)[0]
except subprocess.TimeoutExpired:
    proc.kill()
    rest = proc.communicate()[0]
check("Probe: Firn found every value right", "PROBE ok" in rest and "BAD" not in rest, rest)
check("Slow: its call got a reply", "SLOW reply ok" in rest, rest)
check("Slow: the signal that arrived meanwhile was queued, not lost", "SLOW queued signal Tick" in rest, rest)
check("driver ended cleanly", "DONE" in rest and proc.returncode == 0, (rest, proc.returncode))
daemon.terminate()
print("dbus: %d failed" % len(FAILED) if FAILED else "dbus: all checks passed")
sys.exit(1 if FAILED else 0)
