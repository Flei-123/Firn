#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dnsd/check.py -- lib/net/dnsd*.fi as a process, against decoders nobody here wrote.

    check.py <dnsd_main binary> <zone.conf>

The server is started on a free loopback port. Questions go out as raw UDP/TCP octets built
here; the answers are decoded here AND by `dig` (when it is installed), so a wrong field
cannot hide behind a decoder that shares its mistake.
"""
import os
import shutil
import socket
import struct
import subprocess
import sys
import time

binary, zone = sys.argv[1], sys.argv[2]
passed = failed = 0


def check(label, got, want):
    global passed, failed
    if got == want:
        passed += 1
        print("  ok    %s" % label)
    else:
        failed += 1
        print("  FAIL  %s: got %r, want %r" % (label, got, want))


def free_port():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def encode_name(name):
    out = b""
    for lab in name.rstrip(".").split("."):
        if lab:
            out += bytes([len(lab)]) + lab.encode()
    return out + b"\0"


def query(name, qtype, edns=0, ident=0x1234):
    q = struct.pack(">HHHHHH", ident, 0x0100, 1, 0, 0, 1 if edns else 0)
    q += encode_name(name) + struct.pack(">HH", qtype, 1)
    if edns:
        q += b"\0" + struct.pack(">HHIH", 41, edns, 0, 0)
    return q


def read_name(msg, pos):
    labels = []
    jumped = False
    end = pos
    while True:
        n = msg[pos]
        if n >= 192:
            ptr = ((n & 63) << 8) | msg[pos + 1]
            if not jumped:
                end = pos + 2
            jumped = True
            pos = ptr
        elif n == 0:
            if not jumped:
                end = pos + 1
            return ".".join(labels), end
        else:
            labels.append(msg[pos + 1:pos + 1 + n].decode())
            pos += 1 + n


def decode(msg):
    ident, flags, qd, an, ns, ar = struct.unpack(">HHHHHH", msg[:12])
    pos = 12
    for _ in range(qd):
        _, pos = read_name(msg, pos)
        pos += 4
    def rrs(count, pos):
        out = []
        for _ in range(count):
            name, pos = read_name(msg, pos)
            t, c, ttl, rdlen = struct.unpack(">HHIH", msg[pos:pos + 10])
            pos += 10
            out.append((name, t, ttl, msg[pos:pos + rdlen], pos))
            pos += rdlen
        return out, pos
    answers, pos = rrs(an, pos)
    auth, pos = rrs(ns, pos)
    extra, pos = rrs(ar, pos)
    return dict(id=ident, flags=flags, rcode=flags & 15, aa=bool(flags & 0x400), tc=bool(flags & 0x200),
                answers=answers, auth=auth, extra=extra, raw=msg)


def udp(port, q):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(3)
    s.sendto(q, ("127.0.0.1", port))
    data, _ = s.recvfrom(65535)
    s.close()
    return decode(data)


def tcp(port, q):
    s = socket.create_connection(("127.0.0.1", port), timeout=3)
    s.sendall(struct.pack(">H", len(q)) + q)
    n = struct.unpack(">H", s.recv(2))[0]
    data = b""
    while len(data) < n:
        data += s.recv(n - len(data))
    s.close()
    return decode(data)


port = free_port()
srv = subprocess.Popen([binary, zone, "--port", str(port), "--bind", "127.0.0.1"],
                       stderr=subprocess.PIPE)
try:
    for _ in range(50):
        try:
            udp(port, query("mc.example.com", 1))
            break
        except OSError:
            time.sleep(0.1)

    r = udp(port, query("_minecraft._tcp.thunder.mc.example.com", 33))
    check("srv rcode", r["rcode"], 0)
    check("srv aa", r["aa"], True)
    prio, weight, p, = struct.unpack(">HHH", r["answers"][0][3][:6])
    check("srv prio/weight/port", (prio, weight, p), (0, 5, 443))
    check("srv target", read_name(r["raw"], r["answers"][0][4] + 6)[0], "thunder.mc.example.com")
    check("srv ttl", r["answers"][0][2], 60)

    r = udp(port, query("_MINECRAFT._tcp.Odd-Name_1.mc.example.com", 33))
    check("case folded target", read_name(r["raw"], r["answers"][0][4] + 6)[0], "odd-name_1.mc.example.com")

    r = tcp(port, query("_minecraft._tcp.viatcp.mc.example.com", 33))
    check("tcp srv target", read_name(r["raw"], r["answers"][0][4] + 6)[0], "viatcp.mc.example.com")

    r = udp(port, query("anything.mc.example.com", 1))
    check("wildcard a", socket.inet_ntoa(r["answers"][0][3]), "192.0.2.7")
    r = udp(port, query("ns1.mc.example.com", 1))
    check("exact beats wildcard", socket.inet_ntoa(r["answers"][0][3]), "192.0.2.2")
    r = udp(port, query("mc.example.com", 28))
    check("apex aaaa", socket.inet_ntop(socket.AF_INET6, r["answers"][0][3]), "2001:db8::1")
    r = udp(port, query("info.zed.mc.example.com", 16))
    check("txt with capture", r["answers"][0][3], b"\x08what-zed")

    r = udp(port, query("a.b.c.mc.example.com", 1))
    check("nxdomain", (r["rcode"], r["aa"], len(r["answers"]), len(r["auth"])), (3, True, 0, 1))
    check("negative ttl is the soa minimum", r["auth"][0][2], 30)
    r = udp(port, query("anything.mc.example.com", 16))
    check("nodata", (r["rcode"], len(r["answers"]), len(r["auth"])), (0, 0, 1))
    r = udp(port, query("example.org", 1))
    check("refused outside the zone", (r["rcode"], r["aa"]), (5, False))
    r = udp(port, query("x.mc.example.com", 1, edns=1232))
    check("edns answered", (len(r["extra"]), r["extra"][0][1]), (1, 41))

    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(1)
    s.sendto(b"\x00\x01\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00", ("127.0.0.1", port))
    try:
        s.recvfrom(100)
        check("response packets are not answered", "answered", "silent")
    except socket.timeout:
        check("response packets are not answered", "silent", "silent")
    s.sendto(b"\xff" * 7, ("127.0.0.1", port))
    r = udp(port, query("x.mc.example.com", 1))
    check("still alive after garbage", r["rcode"], 0)

    if shutil.which("dig"):
        def dig(*args):
            return subprocess.run(["dig", "@127.0.0.1", "-p", str(port), "+short", "+time=3", "+tries=1", *args],
                                  capture_output=True, text=True).stdout.strip()
        check("dig srv", dig("SRV", "_minecraft._tcp.dig-one.mc.example.com"), "0 5 443 dig-one.mc.example.com.")
        check("dig srv over tcp", dig("+tcp", "SRV", "_minecraft._tcp.dig-two.mc.example.com"), "0 5 443 dig-two.mc.example.com.")
        check("dig a", dig("A", "dig-three.mc.example.com"), "192.0.2.7")
        check("dig soa", dig("SOA", "mc.example.com"), "ns1.example.com. hostmaster.example.com. 7 3600 600 86400 30")
        check("dig ns", dig("NS", "mc.example.com"), "ns1.example.com.")
        check("dig txt", dig("TXT", "info.q.mc.example.com"), '"what-q"')
    else:
        print("  SKIP  dig is not installed")
finally:
    srv.terminate()
    srv.wait()

print("DNSD %d passed, %d failed" % (passed, failed))
sys.exit(1 if failed else 0)
