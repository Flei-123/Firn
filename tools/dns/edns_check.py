#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/edns_check.py <dns_main> -- EDNS0, search list / ndots and
international names in lib/net/dns.fi against a Python DNS server that
shares no code with it (127.0.0.1, ephemeral port).

What the SERVER saw is what is checked: the OPT record of every question
(present? payload size?), the order of the names that were asked, and the
transport. Real answers are compared with what Python's own `socket` /
`idna` codecs say about the same names.
"""
import socket, socketserver, struct, subprocess, sys, threading

EXE = sys.argv[1]
LOG = []          # (name, qtype, proto, opt_payload or None)
LOCK = threading.Lock()

A = {
    "host.corp.test": ["192.0.2.10"],
    "host.test": ["192.0.2.11"],
    "a.b.corp.test": ["192.0.2.12"],
    "a.b": ["192.0.2.13"],
    "xn--bcher-kva.test": ["192.0.2.20"],
    "formerr.test": ["192.0.2.5"],
    "plain.test": ["192.0.2.1"],
    "single": ["192.0.2.30"],
    "host.x.test": ["192.0.2.31"],
}
BIG = ["198.51.100.%d" % (1 + i) for i in range(40)]


def rd_name(m, off):
    labs = []
    while True:
        b = m[off]
        if b == 0:
            return ".".join(labs), off + 1
        labs.append(m[off + 1:off + 1 + b].decode().lower())
        off += 1 + b


def parse_q(m):
    qd, an, ns, ar = struct.unpack(">4H", m[4:12])
    name, off = rd_name(m, 12)
    qtype, qclass = struct.unpack(">HH", m[off:off + 4])
    off += 4
    opt = None
    if ar:
        # root name, type 41, class = payload
        assert m[off] == 0
        t, c = struct.unpack(">HH", m[off + 1:off + 5])
        if t == 41:
            opt = c
    return name, qtype, off, opt


def enc_name(n):
    out = b""
    for l in n.split("."):
        out += bytes([len(l)]) + l.encode()
    return out + b"\0"


def answer(m, proto):
    name, qtype, qend, opt = parse_q(m)
    with LOCK:
        LOG.append((name, qtype, proto, opt))
    qid = m[:2]
    question = m[12:qend]
    flags = 0x8180
    recs = []
    rcode = 0
    if name == "formerr.test" and opt is not None:
        return qid + struct.pack(">HHHHH", 0x8181, 1, 0, 0, 0) + question
    addrs = None
    if name == "big.test":
        addrs = BIG
    elif name in A:
        addrs = A[name]
    if qtype == 1 and addrs:
        for a in addrs:
            recs.append(b"\xc0\x0c" + struct.pack(">HHIH", 1, 1, 60, 4) + socket.inet_aton(a))
    elif qtype == 1 or addrs is None:
        rcode = 3 if addrs is None else 0
    body = b"".join(recs)
    extra = b""
    if opt is not None:
        extra = b"\0" + struct.pack(">HHIH", 41, 1232, 0, 0)
    total = 12 + len(question) + len(body) + len(extra)
    limit = 512 if opt is None else min(opt, 4096)
    if proto == "udp" and total > limit:
        return qid + struct.pack(">HHHHH", 0x8380, 1, 0, 0, 0) + question  # TC
    return (qid + struct.pack(">HHHHH", flags | rcode, 1, len(recs), 0, 1 if extra else 0)
            + question + body + extra)


class UDP(socketserver.BaseRequestHandler):
    def handle(self):
        data, sock = self.request
        sock.sendto(answer(data, "udp"), self.client_address)


class TCP(socketserver.BaseRequestHandler):
    def handle(self):
        n = struct.unpack(">H", self.request.recv(2))[0]
        data = b""
        while len(data) < n:
            data += self.request.recv(n - len(data))
        r = answer(data, "tcp")
        self.request.sendall(struct.pack(">H", len(r)) + r)


class UDPS(socketserver.ThreadingUDPServer):
    allow_reuse_address = True

class TCPS(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def start():
    # the same port for UDP and TCP
    for _ in range(50):
        u = UDPS(("127.0.0.1", 0), UDP)
        port = u.server_address[1]
        try:
            t = TCPS(("127.0.0.1", port), TCP)
        except OSError:
            u.server_close()
            continue
        threading.Thread(target=u.serve_forever, daemon=True).start()
        threading.Thread(target=t.serve_forever, daemon=True).start()
        return port
    raise SystemExit("no port")


PORT = start()
fails = 0
total = 0


def check(label, got, want):
    global fails, total
    total += 1
    if got != want:
        fails += 1
        print("  FAIL", label, "got", repr(got), "want", repr(want))


def run(args, names):
    with LOCK:
        LOG.clear()
    r = subprocess.run([EXE, "-s", "127.0.0.1:%d" % PORT, "-w", "500", "-r", "1"] + args + names,
                       capture_output=True, timeout=60)
    lines = r.stdout.decode("utf-8", "replace").split("\n")
    with LOCK:
        log = list(LOG)
    return lines, log


def first(line):
    return line.split(" ")

# ---- 1. EDNS0 on the wire
lines, log = run(["-V"], ["plain.test"])
check("edns plain: answer", first(lines[0])[1:3], ["OK", "udp"])
check("edns plain: opt 1232", [(l[0], l[3]) for l in log], [("plain.test", 1232)])
check("edns stats", lines[1], "STATS udp=1 tcp=0 edns=1")

lines, log = run(["-e", "0", "-V"], ["plain.test"])
check("edns off: no OPT", [l[3] for l in log], [None])
check("edns off stats", lines[1], "STATS udp=1 tcp=0 edns=0")

lines, log = run(["-e", "4000"], ["plain.test"])
check("edns 4000", [l[3] for l in log], [4000])

# ---- 2. a 40-record answer: with EDNS one UDP datagram, without TC + TCP
lines, log = run(["-V"], ["big.test"])
check("big edns: no tcp", [l[2] for l in log], ["udp"])
check("big edns: via udp", first(lines[0])[2], "udp")
check("big edns: addresses", sorted(first(lines[0])[5:]), sorted(BIG[:8]))

lines, log = run(["-e", "0", "-V"], ["big.test"])
check("big plain: udp then tcp", [l[2] for l in log], ["udp", "tcp"])
check("big plain: via tcp", first(lines[0])[2], "tcp")

lines, log = run(["-e", "600", "-V"], ["big.test"])
check("big small payload: falls back to tcp", [l[2] for l in log], ["udp", "tcp"])

# ---- 3. FORMERR: asked again without OPT
lines, log = run([], ["formerr.test"])
check("formerr answer", first(lines[0])[1], "OK")
check("formerr log", [(l[2], l[3]) for l in log], [("udp", 1232), ("udp", None)])

# ---- 4. search list and ndots
S = ["-S", "-D", "corp.test"]
lines, log = run(S, ["host"])          # 0 dots < ndots 1: domain first
check("search host", first(lines[0])[1], "OK")
check("search host addr", first(lines[0])[-1], "192.0.2.10")
check("search host order", [l[0] for l in log], ["host.corp.test"])

lines, log = run(S, ["a.b"])           # 1 dot >= ndots 1: as is first
check("search a.b", first(lines[0])[-1], "192.0.2.13")
check("search a.b order", [l[0] for l in log], ["a.b"])

lines, log = run(["-n", "2"] + S, ["a.b"])   # 1 dot < 2: domain first
check("ndots 2 a.b", first(lines[0])[-1], "192.0.2.12")
check("ndots 2 order", [l[0] for l in log], ["a.b.corp.test"])

lines, log = run(["-n", "2", "-S", "-D", "nope.test corp.test"], ["a.b"])
check("two domains order", [l[0] for l in log], ["a.b.nope.test", "a.b.corp.test"])
check("two domains addr", first(lines[0])[-1], "192.0.2.12")

lines, log = run(["-S", "-D", "x.test corp.test"], ["single"])   # no luck with domains: bare last
check("bare last addr", first(lines[0])[-1], "192.0.2.30")
check("bare last order", [l[0] for l in log], ["single.x.test", "single.corp.test", "single"])

lines, log = run(["-S", "-D", "x.test corp.test"], ["host"])
check("second domain wins", [l[0] for l in log], ["host.x.test"])
check("second domain addr", first(lines[0])[-1], "192.0.2.31")

lines, log = run(S, ["host."])          # absolute: no search
check("absolute", [first(lines[0])[1], [l[0] for l in log]], ["ERR", ["host"]])
check("absolute err", first(lines[0])[2], "NxDomain")

lines, log = run(["-S", "-D", "x.test corp.test"], ["nothere"])
check("all NXDOMAIN", [first(lines[0])[1], first(lines[0])[2]], ["ERR", "NxDomain"])
check("all NX order", [l[0] for l in log], ["nothere.x.test", "nothere.corp.test", "nothere"])

lines, log = run(["-S"], ["host.test"])   # no search list: plain
check("no search list", [first(lines[0])[-1], [l[0] for l in log]], ["192.0.2.11", ["host.test"]])

lines, log = run(["host"], [])   # without -S the old behaviour: no search even with -D
lines, log = run(["-D", "corp.test"], ["host"])
check("no -S: bare name only", [first(lines[0])[1], [l[0] for l in log]], ["ERR", ["host"]])

# ---- 5. international names
lines, log = run(["-S"], ["Bücher.test"])
check("idn answer", [first(lines[0])[1], first(lines[0])[-1]], ["OK", "192.0.2.20"])
check("idn asked", [l[0] for l in log], ["xn--bcher-kva.test"])
lines, log = run([], ["bücher.test"])
check("idn plain dns_resolve refuses", [first(lines[0])[1], first(lines[0])[2]], ["ERR", "Name"])
lines, log = run(["-S"], [b"b\xff.test"])
check("idn bad utf8 refused", first(lines[0])[2:3], ["Name"])

print("DNS-EDNS %d checks, %d fail" % (total, fails))
sys.exit(1 if fails else 0)
