#!/usr/bin/env python3
"""Cross-check of net.ip6, dns SRV, net.addrinfo and IPv6 sockets against
Python (ipaddress, socket) with local servers. Usage: netmc_test.py <probe>."""
import ipaddress, random, socket, struct, subprocess, sys, threading, time

PROBE = sys.argv[1]
fails = 0
checks = 0


def check(name, ok, detail=""):
    global fails, checks
    checks += 1
    if not ok:
        fails += 1
        print("  FAIL", name, detail)


def run(*args, timeout=15):
    r = subprocess.run([PROBE, *args], capture_output=True, text=True, timeout=timeout)
    return r.stdout


# ------------------------------------------------------------------ ip6 text
random.seed(7)
cases = ["::", "::1", "1::", "::ffff:1.2.3.4", "2001:db8::1", "[::1]", "fe80::1:2:3:4",
         "1:2:3:4:5:6:7:8", "1:2:3:4:5:6:1.2.3.4", "0:0:0:0:0:0:0:0", "1:0:0:2:0:0:0:3",
         "1:0:0:0:2:0:0:3", "0:1:0:0:0:0:0:0", "2001:0db8:0000:0000:0000:ff00:0042:8329",
         "ABCD:EF01:2345:6789:ABCD:EF01:2345:6789", "::0.0.0.1", "64:ff9b::192.0.2.33"]
bad = ["", ":", ":::", "1:::2", "1::2::3", "12345::", "1:2:3:4:5:6:7", "1:2:3:4:5:6:7:8:9",
       "::1.2.3", "::1.2.3.4.5", "::300.1.1.1", "g::1", "1:2:3:4:5:6:7:8::", "::1:", ":1::",
       "1.2.3.4", "::01.2.3.4", "1:2:3:4:5:6:7::8:9", " ::1", "::1 ", "1::2:", "::%eth0",
       "2001:db8::/32", "1:2:3:4:5:6:7:1.2.3.4"]
for _ in range(500):
    v = random.getrandbits(128)
    if random.random() < 0.5:  # many zero groups
        groups = [(v >> (16 * i)) & 0xFFFF for i in range(8)]
        for i in range(8):
            if random.random() < 0.55:
                groups[i] = 0
        v = sum(g << (16 * i) for i, g in enumerate(groups))
    a = ipaddress.IPv6Address(v)
    cases.append(a.exploded if random.random() < 0.5 else a.compressed)
    cases.append(a.compressed.upper())


def canon(a):
    if a.ipv4_mapped is not None:
        return None  # dotted or hex tail both legal; compare semantically
    return a.compressed


bad_seen = 0
for t in cases:
    try:
        a = ipaddress.IPv6Address(t.strip("[]") if t.startswith("[") else t)
    except ValueError:
        a = None
    out = run("ip6", t).strip()
    if a is None:
        check("ip6 reject " + t, out == "BAD", out)
        continue
    c = canon(a)
    if c is None:
        back = ipaddress.IPv6Address(out)
        check("ip6 mapped " + t, back == a, out)
    else:
        check("ip6 " + t, out == c, out + " vs " + c)
for t in bad:
    out = run("ip6", t).strip()
    py_ok = True
    try:
        ipaddress.IPv6Address(t)
    except ValueError:
        py_ok = False
    check("ip6 bad " + repr(t), out == "BAD" and (not py_ok or "%" in t), out)
n_ip6 = checks
print("  ip6 text: %d checks" % n_ip6)

# ---------------------------------------------------------------- fake DNS
PORT_A, PORT_H, PORT_B = 25601, 25602, 25603


def qname_bytes(n):
    return b"".join(bytes([len(p)]) + p.encode() for p in n.split(".") if p) + b"\0"


def rr(owner_ptr, typ, ttl, rdata):
    return owner_ptr + struct.pack(">HHIH", typ, 1, ttl, len(rdata)) + rdata


DB = {
    ("_minecraft._tcp.mc.test", 33): [
        (10, 5, PORT_B, "b.mc.test"), (5, 1, PORT_A, "a.mc.test"), (5, 9, PORT_H, "heavy.mc.test")],
    ("_minecraft._tcp.off.test", 33): [(0, 0, 0, ".")],
    ("a.mc.test", 1): ["127.0.0.1"],
    ("heavy.mc.test", 1): ["127.0.0.1"],
    ("heavy.mc.test", 28): ["::1"],
    ("nosrv.test", 1): ["127.0.0.1"],
    ("nosrv.test", 28): ["::1"],
    ("dual.test", 1): ["127.0.0.1", "127.0.0.2"],
    ("dual.test", 28): ["::1", "2001:db8::7"],
    ("only6.test", 28): ["::1"],
}
QUERIES = []


def answer(data):
    tid = data[:2]
    i = 12
    while data[i]:
        i += data[i] + 1
    qn = data[12:i + 1]
    qtype, qclass = struct.unpack(">HH", data[i + 1:i + 5])
    name = ".".join(p.decode() for p in split_labels(qn))
    QUERIES.append((name, qtype))
    ans = []
    key = (name, qtype)
    rcode = 0
    if key in DB:
        for v in DB[key]:
            if qtype == 33:
                prio, w, port, tgt = v
                rd = struct.pack(">HHH", prio, w, port) + (b"\0" if tgt == "." else qname_bytes(tgt))
                ans.append(rr(b"\xc0\x0c", 33, 60, rd))
            elif qtype == 1:
                ans.append(rr(b"\xc0\x0c", 1, 60, socket.inet_aton(v)))
            else:
                ans.append(rr(b"\xc0\x0c", 28, 60, socket.inet_pton(socket.AF_INET6, v)))
    else:
        known = {k[0] for k in DB}
        if name not in known and not any(k[0] == name for k in DB):
            rcode = 3
    hdr = tid + struct.pack(">HHHHH", 0x8180 | rcode, 1, len(ans), 0, 0)
    return hdr + data[12:i + 5] + b"".join(ans)


def split_labels(qn):
    out, i = [], 0
    while qn[i]:
        out.append(qn[i + 1:i + 1 + qn[i]])
        i += qn[i] + 1
    return out


dsock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
dsock.bind(("127.0.0.1", 0))
DNS = "127.0.0.1:%d" % dsock.getsockname()[1]


def dns_loop():
    while True:
        try:
            d, a = dsock.recvfrom(512)
        except OSError:
            return
        dsock.sendto(answer(d), a)


threading.Thread(target=dns_loop, daemon=True).start()

# ---------------------------------------------------------------------- SRV
out = run("-s", DNS, "srv", "_minecraft._tcp.mc.test").split("\n")
exp = ["srv 5 9 %d heavy.mc.test" % PORT_H, "srv 5 1 %d a.mc.test" % PORT_A, "srv 10 5 %d b.mc.test" % PORT_B]
check("srv order prio/weight", out[:3] == exp, repr(out))
check("srv nxdomain", run("-s", DNS, "srv", "_minecraft._tcp.zzz.test").strip() == "ERR NxDomain")
check("srv nodata", run("-s", DNS, "srv", "_minecraft._tcp.nosrv.test").strip() in ("ERR NxDomain", "ERR NoData"))
check("srv bad name", run("-s", DNS, "srv", "a..b").strip() == "ERR Name")

# ------------------------------------------------------------------- addrs
out = run("-s", DNS, "addrs", "dual.test").split("\n")
check("addrs v6/v4 interleave", out[:4] == ["addr ::1", "addr 127.0.0.1", "addr 2001:db8::7", "addr 127.0.0.2"], repr(out))
out = run("-s", DNS, "addrs", "only6.test").split("\n")
check("addrs only AAAA", out[0] == "addr ::1", repr(out))
check("addrs literal v6", run("addrs", "[2001:db8::1]").startswith("addr 2001:db8::1"))
check("addrs literal v4", run("addrs", "10.1.2.3").startswith("addr 10.1.2.3"))
check("addrs mapped -> v4", run("addrs", "::ffff:10.1.2.3").startswith("addr 10.1.2.3"))
check("addrs localhost", run("addrs", "localhost").split("\n")[:2] == ["addr ::1", "addr 127.0.0.1"])
check("addrs nx", run("-s", DNS, "addrs", "nope.test").strip() == "ERR NxDomain")

# --------------------------------------------------------------------- svc
out = run("-s", DNS, "svc", "mc.test").split("\n")
check("svc uses SRV", out[0] == "svc port=%d srv=1 host=heavy.mc.test" % PORT_H and out[1] == "addr ::1" and out[2] == "addr 127.0.0.1", repr(out))
n0 = len(QUERIES)
out = run("-s", DNS, "svc", "mc.test", "25565").split("\n")
check("svc explicit port: no SRV", out[0].startswith("svc port=25565 srv=0") or out[0].startswith("ERR"), repr(out))
check("svc explicit port: no SRV query", not any(q[1] == 33 for q in QUERIES[n0:]))
out = run("-s", DNS, "svc", "nosrv.test").split("\n")
check("svc fallback 25565", out[0] == "svc port=25565 srv=0 host=nosrv.test", repr(out))
out = run("-s", DNS, "svc", "off.test").split("\n")
check("svc '.' target -> fallback", out[0].startswith("ERR") or out[0].startswith("svc port=25565 srv=0"), repr(out))
n0 = len(QUERIES)
out = run("-s", DNS, "svc", "127.0.0.1").split("\n")
check("svc literal: no SRV", out[0] == "svc port=25565 srv=0 host=127.0.0.1" and not any(q[1] == 33 for q in QUERIES[n0:]), repr(out))
print("  dns/addrinfo: %d checks so far" % checks)

# --------------------------------------------------------------------- dial


def serve(sock, tag):
    def loop():
        while True:
            try:
                c, _ = sock.accept()
            except OSError:
                return
            try:
                c.recv(64)
                c.sendall(tag + b"\n")
            finally:
                c.close()
    threading.Thread(target=loop, daemon=True).start()


def pair(v4=True, v6=True, hang6=False):
    s4 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s4.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s4.bind(("127.0.0.1", 0))
    port = s4.getsockname()[1]
    s6 = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
    s6.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s6.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
    s6.bind(("::1", port))
    keep = []
    if v4:
        s4.listen(8)
        serve(s4, b"v4")
    else:
        s4.close()
    if v6 and not hang6:
        s6.listen(8)
        serve(s6, b"v6")
    elif hang6:
        s6.listen(0)  # backlog 0 + filled: further SYNs are dropped -> connect hangs
        for _ in range(3):
            k = socket.socket(socket.AF_INET6)
            k.setblocking(False)
            try:
                k.connect(("::1", port))
            except BlockingIOError:
                pass
            keep.append(k)
        time.sleep(0.2)
    else:
        s6.close()
    return port, (s4, s6, keep)


def dial(host, port, stagger=250):
    t = time.time()
    out = run("-s", DNS, "dial", host, str(port), str(stagger))
    return out, (time.time() - t) * 1000


port, keep = pair()
out, _ = dial("dual.test", port)
check("dial both up -> v6 first", "connected via ::1" in out and "reply v6" in out, repr(out))
port, keep = pair(v6=False)
out, ms = dial("dual.test", port)
check("dial v6 refused -> fast v4 fallback", "connected via 127.0.0.1" in out and "reply v4" in out, repr(out))
mine = int(out.split("ms=")[1].split()[0]) if "ms=" in out else 9999
check("dial fallback did not wait for stagger", mine < 120, "ms=%d" % mine)
port, keep = pair(hang6=True)
out, _ = dial("dual.test", port, 150)
mine = int(out.split("ms=")[1].split()[0]) if "ms=" in out else 9999
check("dial hung v6 -> v4 after ~stagger", "connected via 127.0.0.1" in out and 120 <= mine < 900, repr(out))
print("    stagger 150 ms with a hung v6 listener: connected after %d ms" % mine)
port, keep = pair(v4=False, v6=False)
out, _ = dial("dual.test", port)
check("dial all refused", out.startswith("ERR connect errno=111"), repr(out))
port, keep = pair(v4=False)
out, _ = dial("only6.test", port)
check("dial AAAA-only host", "connected via ::1" in out, repr(out))

# serve6: Firn IPv6 listener, Python client
p = subprocess.Popen([PROBE, "serve6"], stdout=subprocess.PIPE, text=True)
fp = int(p.stdout.readline())
c = socket.socket(socket.AF_INET6)
c.connect(("::1", fp))
c.sendall(b"hello6")
got = b""
while True:
    d = c.recv(64)
    if not d:
        break
    got += d
c.close()
p.wait(timeout=5)
check("firn listen_tcp6 + python v6 client", got == b"v6ok:hello6", repr(got))

# throughput: 300 sequential dials v6 vs python
N = 300
srv = socket.socket(socket.AF_INET6)
srv.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
srv.bind(("::1", 0))
srv.listen(64)
serve(srv, b"v6")
sp = srv.getsockname()[1]
t = time.time()
for _ in range(N):
    run("dial", "::1", str(sp))
dt = (time.time() - t) / N * 1000
print("    dial+ping v6 incl. process start: %.1f ms each (N=%d)" % (dt, N))

print("RESULT netmc: %s (%d checks, %d failed)" % ("ok" if not fails else "FAIL", checks, fails))
sys.exit(1 if fails else 0)
