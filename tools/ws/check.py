#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/ws/check.py -- the WebSocket CLIENT of lib/ws/ws.fi against a server
# this repository did not write (Python `websockets`), and against a
# `websockets` server that misbehaves (the refusals), plus wss:// against
# the Firn server with a verified certificate.
#
#   python3 tools/ws/check.py <client_main> <test_main (http)> <workdir>
import asyncio, os, random, subprocess, sys, threading, time
import websockets

CLI, SRV, WORK = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(WORK, exist_ok=True)
ok = total = refusals = 0


def check(name, cond, detail=""):
    global ok, total
    total += 1
    if cond:
        ok += 1
    else:
        print("  FAIL", name, str(detail)[:400])


EXPECT = ["T 5 hello", "T 200000 abcdefghijklmnop", "B 256 32640", "T 10 after ping", "closed 1000"]


def run_client(port, *extra):
    r = subprocess.run([CLI, str(port), "/ws"] + list(extra), capture_output=True, timeout=30)
    return r.returncode, r.stdout.decode().strip().split("\n")


def serve(handler, port, stop):
    async def main():
        async with websockets.serve(handler, "127.0.0.1", port, max_size=None):
            while not stop.is_set():
                await asyncio.sleep(0.05)
    asyncio.run(main())


async def echo(ws):
    async for m in ws:
        await ws.send(m)


port = 20000 + random.randrange(0, 900)
stop = threading.Event()
th = threading.Thread(target=serve, args=(echo, port, stop), daemon=True)
th.start()
time.sleep(0.5)
rc, lines = run_client(port)
check("client against python websockets: echo, 200 kB, binary, ping, close 1000", lines == EXPECT, lines)
stop.set()
th.join(3)


# a server that masks its frames (forbidden, RFC 6455 5.1)
async def masking(ws):
    await ws.recv()
    tr = ws.transport
    tr.write(bytes([0x81, 0x85, 1, 2, 3, 4]) + bytes(a ^ b for a, b in zip(b"hello", b"\x01\x02\x03\x04\x01")))
    await asyncio.sleep(0.5)

port += 1
stop = threading.Event()
th = threading.Thread(target=serve, args=(masking, port, stop), daemon=True)
th.start()
time.sleep(0.5)
rc, lines = run_client(port)
refusals += 1
check("REFUSED a masked frame from the server", lines[0] != "T 5 hello", lines)
stop.set()
th.join(3)


# invalid UTF-8 in a text frame
async def badutf(ws):
    await ws.recv()
    ws.transport.write(bytes([0x81, 0x02, 0xC3, 0x28]))
    await asyncio.sleep(0.5)

port += 1
stop = threading.Event()
th = threading.Thread(target=serve, args=(badutf, port, stop), daemon=True)
th.start()
time.sleep(0.5)
rc, lines = run_client(port)
refusals += 1
check("REFUSED invalid UTF-8 in a text message", lines[0] != "T 2 \xc3(" and not lines[0].startswith("T 2"), lines)
stop.set()
th.join(3)

# wrong accept key: a plain HTTP server that answers 101 with garbage
import socket
ls = socket.socket()
ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
port += 1
ls.bind(("127.0.0.1", port))
ls.listen(1)


def fake():
    c, _ = ls.accept()
    c.recv(4096)
    c.sendall(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
              b"Sec-WebSocket-Accept: AAAAAAAAAAAAAAAAAAAAAAAAAAA=\r\n\r\n")
    time.sleep(0.5)
    c.close()
threading.Thread(target=fake, daemon=True).start()
rc, lines = run_client(port)
refusals += 1
check("REFUSED a wrong Sec-WebSocket-Accept", lines == ["connect failed"], lines)

# wss:// against the Firn server, certificate verified by the Firn client
ck, cc = os.path.join(WORK, "k.pem"), os.path.join(WORK, "c.pem")
subprocess.run(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", ck], capture_output=True)
subprocess.run(["openssl", "req", "-new", "-x509", "-key", ck, "-out", cc, "-days", "30", "-subj", "/CN=localhost",
                "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1", "-addext", "basicConstraints=critical,CA:TRUE"],
               capture_output=True)
port += 1
p = subprocess.Popen([SRV, str(port), "tls", cc, ck], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
p.stdout.readline()
time.sleep(0.2)
rc, lines = run_client(port, cc)
check("wss:// Firn client -> Firn server (TLS 1.3 both ends, verified)", lines == EXPECT, lines)
# the same with roots that do not contain the certificate: refused
other = os.path.join(WORK, "other.pem")
subprocess.run(["openssl", "req", "-new", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
                "-nodes", "-keyout", os.path.join(WORK, "ok.pem"), "-out", other, "-days", "30", "-subj",
                "/CN=localhost"], capture_output=True)
rc, lines = run_client(port, other)
refusals += 1
check("REFUSED wss:// with a certificate the roots do not vouch for", lines == ["connect failed"], lines)
p.kill()

print("   %d / %d WebSocket client cases, of them %d refusals" % (ok, total, refusals))
print(("WS CLIENT OK: %d / %d, refusals %d" if ok == total else "WS CLIENT FAILED: %d / %d, refusals %d")
      % (ok, total, refusals))
sys.exit(0 if ok == total else 1)
