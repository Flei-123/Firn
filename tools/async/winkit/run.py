#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# run.py -- the async IO kit (event loop, streams, TLS, HTTP client, WebSocket
# client, posting from threads) on ANOTHER machine. Needs only Python 3.
#
#   py run.py          (Windows)        python3 run.py          (Linux)
#
# What it runs, in this folder:
#   t2120 .. t2125   the in-process tests (a server and its clients in one
#                    program, on 127.0.0.1); exit code 0 = all checks passed
#   conn inproc 28   28 clients + 28 server sockets in one loop
#   conn + asyncio   the Firn echo server against a Python asyncio client (40 connections)
#   wss              the WebSocket client against REAL wss:// echo services
#                    with the machine's own certificate roots; "unreachable"
#                    (no route, or a firewall) is a SKIP, a failed check is not
# Windows' select() limit (64 sockets) is why the connection counts are small.
import asyncio, os, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
runner = os.environ.get("RUNNER", "").split()   # e.g. RUNNER=wine to try the .exe files on Linux
exe = ".exe" if (os.name == "nt" or runner) else ""
bad = 0
skipped = 0

def say(kind, msg):
    print("  %-5s %s" % (kind, msg))
    sys.stdout.flush()

def run(args, timeout):
    t0 = time.time()
    try:
        p = subprocess.run(runner + args, cwd=here, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr, time.time() - t0
    except subprocess.TimeoutExpired:
        return 124, "", "timeout", time.time() - t0

print("== in-process tests ==")
for name in ("t2120_async_loop", "t2121_async_stream", "t2122_async_tls",
             "t2123_async_http", "t2124_async_ws", "t2125_async_post"):
    path = os.path.join(here, name + exe)
    if not os.path.exists(path):
        say("skip", name + " is not in the kit"); skipped += 1; continue
    code, out, err, dt = run([path], 300)
    if code == 0:
        say("ok", "%s (%.1f s)" % (name, dt))
    else:
        bad += 1
        say("FAIL", "%s exit %d: %s %s" % (name, code, out.strip()[:200], err.strip()[:200]))

print("== connections ==")
conn = os.path.join(here, "conn" + exe)
code, out, err, dt = run([conn, "inproc", "28"], 120)
first = out.split("\n")[0].strip()
if code == 0 and first.startswith("poll OK 28 28"):
    say("ok", "28 clients + 28 server sockets, one loop: %s" % first)
else:
    bad += 1
    say("FAIL", "conn inproc 28: exit %d %r %s" % (code, first, err.strip()[:200]))

async def asyncio_client():
    proc = await asyncio.create_subprocess_exec(*(runner + [conn, "server", "0"]), stdout=asyncio.subprocess.PIPE)
    line = (await asyncio.wait_for(proc.stdout.readline(), 15)).decode().split()
    port = int(line[1])
    n = 40
    pairs = await asyncio.gather(*[asyncio.open_connection("127.0.0.1", port) for _ in range(n)])
    def msg(i): return bytes((i * 11 + k) & 255 for k in range(64))
    for i, (r, w) in enumerate(pairs):
        w.write(msg(i))
    good = 0
    for i, (r, w) in enumerate(pairs):
        if await asyncio.wait_for(r.readexactly(64), 30) == msg(i):
            good += 1
    for r, w in pairs:
        w.close()
    out, _ = await asyncio.wait_for(proc.communicate(), 30)
    return good, n, out.decode().strip().split("\n")[-1]

try:
    good, n, served = asyncio.run(asyncio_client())
    if good == n:
        say("ok", "Python asyncio client against the Firn server: %d/%d echoes exact (%s)" % (good, n, served))
    else:
        bad += 1
        say("FAIL", "asyncio client: %d/%d" % (good, n))
except Exception as e:
    bad += 1
    say("FAIL", "asyncio client: %r" % (e,))

print("== real wss:// echo services (the machine's own certificate roots) ==")
wss = os.path.join(here, "wss" + exe)
reached = 0
for u in ("wss://echo.websocket.org/", "wss://ws.ifelse.io/"):
    code, out, err, dt = run([wss, u], 90)
    last = out.strip().split("\n")[-1] if out.strip() else ""
    if code == 0:
        reached += 1
        say("ok", "%s : %s" % (u, last))
    elif code == 3:
        skipped += 1
        say("skip", "%s : not reachable (%s)" % (u, " | ".join(out.strip().split("\n")[-2:])))
    else:
        bad += 1
        say("FAIL", "%s exit %d: %s" % (u, code, out.strip().replace("\n", " | ")[:300]))
if reached == 0:
    say("note", "no wss service was reached: the certificate roots of this machine were NOT exercised")

print()
print("async kit: %d failed, %d skipped" % (bad, skipped))
sys.exit(1 if bad else 0)
