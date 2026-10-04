#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/async/check_conn.py <conn_main> [n] -- a thousand connections on one
# loop, held against an implementation this repository did not write
# (Python asyncio):
#
#   A  the Firn client opens n connections to a Python echo server; all n are
#      open AT THE SAME TIME (the server counts the peak), each sends 64
#      octets and checks the echo
#   B  a Python client opens n connections to the Firn echo server, the same
#      way; the Firn server counts its own peak and bytes
#   C  both ends in Firn, one process, one loop: the epoll backend and the
#      poll backend (2n sockets)
import asyncio, os, subprocess, sys, time

prog = sys.argv[1]
n = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
bad = 0
def ok(m): print("  ok  ", m)
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)

def msg(i):
    return bytes((i * 11 + k) & 255 for k in range(64))

# ---------------------------------------------------------------- A
async def echo_server(stats):
    async def handle(r, w):
        stats["live"] += 1
        stats["peak"] = max(stats["peak"], stats["live"])
        stats["total"] += 1
        try:
            while True:
                d = await r.read(4096)
                if not d:
                    break
                w.write(d)
                await w.drain()
        finally:
            stats["live"] -= 1
            w.close()
    srv = await asyncio.start_server(handle, "127.0.0.1", 0, backlog=4096)
    return srv

async def part_a():
    stats = {"live": 0, "peak": 0, "total": 0}
    srv = await echo_server(stats)
    port = srv.sockets[0].getsockname()[1]
    proc = await asyncio.create_subprocess_exec(prog, "client", str(port), str(n),
                                                stdout=asyncio.subprocess.PIPE)
    out, _ = await asyncio.wait_for(proc.communicate(), 90)
    srv.close()
    text = out.decode().split("\n")
    first = text[0].split() if text else []
    good = proc.returncode == 0 and first[:1] == ["OK"] and int(first[1]) == n and int(first[2]) == n
    (ok if good else fail)("A: the Firn client held %d connections open at once against Python asyncio (client says %s, server peak %d, total %d)"
                          % (n, " ".join(first), stats["peak"], stats["total"]))
    (ok if stats["peak"] >= n * 0.99 and stats["total"] == n else fail)(
        "A: ... and the Python server really saw them: peak %d of %d" % (stats["peak"], n))
    return text

# ---------------------------------------------------------------- B
async def part_b():
    proc = await asyncio.create_subprocess_exec(prog, "server", "0", stdout=asyncio.subprocess.PIPE)
    line = (await asyncio.wait_for(proc.stdout.readline(), 10)).decode().split()
    port = int(line[1])
    conns = []
    t0 = time.time()
    async def one(i):
        r, w = await asyncio.open_connection("127.0.0.1", port)
        return r, w
    pairs = await asyncio.gather(*[one(i) for i in range(n)])
    ramp = time.time() - t0
    for i, (r, w) in enumerate(pairs):
        w.write(msg(i))
    good = 0
    for i, (r, w) in enumerate(pairs):
        d = await asyncio.wait_for(r.readexactly(64), 30)
        if d == msg(i):
            good += 1
    for r, w in pairs:
        w.close()
    out, _ = await asyncio.wait_for(proc.communicate(), 30)
    served = [l for l in out.decode().split("\n") if l.startswith("SERVED")]
    parts = served[0].split() if served else []
    (ok if good == n else fail)("B: a Python asyncio client held %d connections open at once against the Firn server; %d/%d echoes exact (ramp %.0f ms)" % (n, good, n, ramp * 1000))
    (ok if parts and int(parts[2]) >= n * 0.99 and int(parts[3]) == 64 * n else fail)(
        "B: ... the Firn server's own count: %s" % " ".join(parts))

# ---------------------------------------------------------------- C
def part_c():
    for extra, name in (([], "epoll"), (["poll"], "poll")):
        t0 = time.time()
        p = subprocess.run([prog, "inproc", str(n)] + extra, capture_output=True, text=True, timeout=90)
        lines = p.stdout.split("\n")
        first = lines[0].split()
        good = p.returncode == 0 and len(first) >= 4 and first[1] == "OK" and int(first[2]) == n and int(first[3]) == n
        (ok if good else fail)("C: %d clients + %d server sockets in ONE loop, backend %s: %s (%.0f ms wall)"
                              % (n, n, name, " ".join(first), (time.time() - t0) * 1000))

asyncio.run(part_a())
asyncio.run(part_b())
part_c()
print("conn: %d failed" % bad)
sys.exit(1 if bad else 0)
