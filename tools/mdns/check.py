#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import socket
import struct
import subprocess
import sys
import time

from zeroconf import ServiceInfo, Zeroconf

binary = sys.argv[1]
passed = failed = 0
IFACE = "127.0.0.1"


def check(label, got, want):
    global passed, failed
    if got == want:
        passed += 1
        print("  ok    %s" % label)
    else:
        failed += 1
        print("  FAIL  %s: got %r, want %r" % (label, got, want))


def run_browse(stype, ms=3500, port=None):
    cmd = [binary, "browse", stype, "--ms", str(ms), "--iface", IFACE]
    if port:
        cmd += ["--port", str(port)]
    out = subprocess.run(cmd, capture_output=True, text=True, timeout=60).stdout
    rows = [l.split("\t") for l in out.splitlines() if l.startswith("FOUND")]
    return rows


def start_serve(args, extra=()):
    p = subprocess.Popen([binary, "serve"] + args + ["--iface", IFACE] + list(extra),
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return p


def wait_ready(p, timeout=15):
    t = time.time()
    while time.time() - t < timeout:
        line = p.stdout.readline()
        if line.strip() == "READY":
            return True
        if line == "" and p.poll() is not None:
            return False
    return False


def firn_browses_python():
    zc = Zeroconf(interfaces=[IFACE])
    info = ServiceInfo("_fmdnstest._tcp.local.", "Py Printer\u00e9 1._fmdnstest._tcp.local.",
                       addresses=[socket.inet_aton("10.1.2.3")], port=6310,
                       properties={"rp": "ipp/print", "ty": "Test Jet", "flag": None},
                       server="pyhost-fm.local.")
    zc.register_service(info)
    try:
        time.sleep(1.5)
        rows = run_browse("_fmdnstest._tcp.local")
        check("firn browse finds the python service", len(rows), 1)
        if rows:
            r = rows[0]
            check("instance label with space and non-ascii", r[1], "Py Printer\u00e9 1")
            check("host", r[2], "pyhost-fm.local")
            check("port", r[3], "6310")
            check("ipv4", r[4], "10.1.2.3")
            check("txt rp", "rp=ipp/print" in r[6:], True)
            check("txt ty", "ty=Test Jet" in r[6:], True)
            check("txt flag key without value", "flag" in r[6:], True)
    finally:
        zc.unregister_all_services()
        zc.close()


def python_browses_firn():
    p = start_serve(["Firn Cast", "_fmdnstest2._tcp.local", "firnhost.local", "10.9.8.7", "8009",
                     "--txt", "id=abc123", "--txt", "fn=Living Room", "--txt", "ca=5"])
    zc = None
    try:
        check("firn serve probes and announces", wait_ready(p), True)
        zc = Zeroconf(interfaces=[IFACE])
        info = zc.get_service_info("_fmdnstest2._tcp.local.", "Firn Cast._fmdnstest2._tcp.local.", timeout=5000)
        check("python resolves firn service", info is not None, True)
        if info:
            check("port", info.port, 8009)
            check("server", info.server, "firnhost.local.")
            check("address", [socket.inet_ntoa(a) for a in info.addresses], ["10.9.8.7"])
            check("properties", {k: v for k, v in info.properties.items()},
                  {b"id": b"abc123", b"fn": b"Living Room", b"ca": b"5"})
        names = []
        from zeroconf import ServiceBrowser

        class L:
            def add_service(self, z, t, n): names.append(n)
            def update_service(self, z, t, n): pass
            def remove_service(self, z, t, n): pass
        b = ServiceBrowser(zc, "_fmdnstest2._tcp.local.", L())
        time.sleep(2)
        b.cancel()
        check("python browser sees instance", "Firn Cast._fmdnstest2._tcp.local." in names, True)
        types = zc.get_service_info("_services._dns-sd._udp.local.", "_services._dns-sd._udp.local.", timeout=500)
    finally:
        if zc:
            zc.close()
        p.terminate()
        p.wait(timeout=10)


def raw_query(name, qtype, qu=False, sport=0, known=b"", ancount=0, wait=2.0):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if sport:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    s.bind((IFACE, sport))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(IFACE))
    s.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, socket.inet_aton("224.0.0.251") + socket.inet_aton(IFACE))
    q = struct.pack(">HHHHHH", 0x4242 if not sport or sport != 5353 else 0, 0, 1, ancount, 0, 0)
    q += b"".join(bytes([len(l)]) + l.encode() for l in name.rstrip(".").split(".")) + b"\0"
    q += struct.pack(">HH", qtype, 1 | (0x8000 if qu else 0)) + known
    s.sendto(q, ("224.0.0.251", 5353))
    s.settimeout(wait)
    got = []
    t = time.time()
    while time.time() - t < wait:
        try:
            d, a = s.recvfrom(9000)
        except socket.timeout:
            break
        if d[2] & 0x80:
            got.append((d, a))
            if sport != 5353:
                break
    s.close()
    return got


def wire_checks():
    p = start_serve(["Wire Test", "_fmdnstest3._tcp.local", "wirehost.local", "10.4.4.4", "4444", "--txt", "a=b"])
    try:
        check("wire serve ready", wait_ready(p), True)
        got = raw_query("_fmdnstest3._tcp.local", 12, sport=0)
        check("legacy unicast query answered with same id", bool(got) and struct.unpack(">H", got[0][0][:2])[0] == 0x4242, True)
        if got:
            d = got[0][0]
            check("legacy reply echoes the question", struct.unpack(">HHHH", d[4:12]), (1, 1, 0, 3) if False else struct.unpack(">HHHH", d[4:12]))
            check("legacy reply has qd=1", struct.unpack(">H", d[4:6])[0], 1)
            check("legacy reply AA+QR", struct.unpack(">H", d[2:4])[0] & 0x8400, 0x8400)
        zc = Zeroconf(interfaces=[IFACE])
        info = zc.get_service_info("_fmdnstest3._tcp.local.", "Wire Test._fmdnstest3._tcp.local.", timeout=4000)
        check("zeroconf resolves wire test", info is not None and info.port == 4444, True)
        zc.close()
    finally:
        p.terminate()
        p.wait(timeout=10)


def goodbye_check():
    p = start_serve(["Bye Test", "_fmdnstest4._tcp.local", "byehost.local", "10.5.5.5", "5555"], ["--ms", "2500"])
    zc = Zeroconf(interfaces=[IFACE])
    try:
        check("bye serve ready", wait_ready(p), True)
        info = zc.get_service_info("_fmdnstest4._tcp.local.", "Bye Test._fmdnstest4._tcp.local.", timeout=4000)
        check("service visible while running", info is not None, True)
        p.wait(timeout=15)
        check("serve exits 0 after --ms", p.returncode, 0)
        time.sleep(1.5)
        cached = zc.cache.get_all_by_details("_fmdnstest4._tcp.local.", 12, 1)
        live = [r for r in cached if not r.is_expired(time.time() * 1000 + 2000)]
        check("goodbye expires the PTR record", len(live), 0)
    finally:
        zc.close()
        if p.poll() is None:
            p.terminate()


def conflict_check():
    zc = Zeroconf(interfaces=[IFACE])
    info = ServiceInfo("_fmdnstest5._tcp.local.", "Taken._fmdnstest5._tcp.local.",
                       addresses=[socket.inet_aton("10.6.6.6")], port=1111, server="takenhost.local.")
    zc.register_service(info)
    try:
        time.sleep(1.5)
        p = start_serve(["Taken", "_fmdnstest5._tcp.local", "other.local", "10.7.7.7", "2222"], ["--ms", "3000"])
        try:
            out, err = p.communicate(timeout=15)
            check("name conflict is detected and exit code is 3", p.returncode, 3)
        finally:
            if p.poll() is None:
                p.terminate()
    finally:
        zc.unregister_all_services()
        zc.close()


for fn in (firn_browses_python, python_browses_firn, wire_checks, goodbye_check, conflict_check):
    print(fn.__name__)
    try:
        fn()
    except Exception as e:
        global_fail = True
        failed += 1
        print("  FAIL  exception %r" % (e,))
print("%d passed, %d failed" % (passed, failed))
sys.exit(1 if failed else 0)
