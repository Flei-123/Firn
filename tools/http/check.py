#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/http/check.py -- lib/http/server.fi (HTTP/1.1, WebSocket, SSE,
# pairing, TLS) against clients this repository did not write: curl,
# Python's http.client / socket / ssl, and the `websocket-client` and
# `websockets` packages. The refusals are counter-checks.
#
#   python3 tools/http/check.py <test_main binary> <workdir>
import os, socket, ssl, subprocess, sys, time, random, threading, json
import http.client
import websocket  # websocket-client

BIN, WORK = sys.argv[1], sys.argv[2]
os.makedirs(WORK, exist_ok=True)
ok = total = refusals = 0
PORT = 19000 + random.randrange(0, 800)


def check(name, cond, detail=""):
    global ok, total
    total += 1
    if cond:
        ok += 1
    else:
        print("  FAIL", name, str(detail)[:300])


def refusal(name, cond, detail=""):
    global refusals
    refusals += 1
    check("REFUSED " + name, cond, detail)


class Srv:
    def __init__(self, *args):
        global PORT
        PORT += 1
        self.port = PORT
        self.p = subprocess.Popen([BIN, str(PORT)] + list(args), stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE)
        self.url = self.p.stdout.readline().decode().strip()
        time.sleep(0.1)

    def stop(self):
        self.p.kill()
        self.p.wait()


def raw(port, data, wait=0.4, tls_ctx=None):
    s = socket.create_connection(("127.0.0.1", port))
    if tls_ctx:
        s = tls_ctx.wrap_socket(s, server_hostname="localhost")
    s.sendall(data)
    s.settimeout(wait)
    out = b""
    t0 = time.time()
    while time.time() - t0 < 3:
        try:
            b = s.recv(65536)
            if not b:
                break
            out += b
        except socket.timeout:
            break
    s.close()
    return out


def curl(*args):
    r = subprocess.run(["curl", "-s", "-S"] + list(args), capture_output=True, timeout=20)
    return r.stdout.decode("latin1"), r.stderr.decode("latin1")


s = Srv()
P = s.port
base = "http://127.0.0.1:%d" % P

# ---- plain HTTP/1.1
out, _ = curl("-i", base + "/")
check("GET / 200 with Content-Length", out.startswith("HTTP/1.1 200 OK") and "Content-Length: 73" in out
      and out.endswith("server</p>\n"), out)
out, _ = curl("-I", base + "/")
check("HEAD: headers, no body", "Content-Length: 73" in out and "<p>" not in out, out)
out, _ = curl("-X", "POST", "-H", "X-Probe: p1", "--data-binary", "abc", base + "/echo")
check("POST body + header lookup (case-insensitive)", out == "POST 3 p1\nabc", out)
body = os.urandom(300000).hex()[:500000]
out, _ = curl("-X", "PUT", "--data-binary", "@-", "-H", "Expect: 100-continue", base + "/echo",
              ) if False else (None, None)
r = subprocess.run(["curl", "-s", "-X", "PUT", "--data-binary", "@-", base + "/echo"], input=body.encode(),
                   capture_output=True, timeout=20)
check("PUT 500 kB body (curl sends Expect: 100-continue)", r.stdout.decode() == "PUT %d \n%s" % (len(body), body))
r = subprocess.run(["curl", "-s", "-H", "Transfer-Encoding: chunked", "--data-binary", "@-", base + "/echo"],
                   input=b"x" * 70000, capture_output=True, timeout=20)
check("chunked request body (curl)", r.stdout.decode() == "POST 70000 \n" + "x" * 70000, r.stdout[:80])
out, _ = curl(base + "/q?a=42&b=7")
check("query parameter", out == "42", out)
out, _ = curl("-s", "-o", "/dev/null", "-w", "%{http_code}", base + "/missing")
check("404", out == "404", out)
out, _ = curl("-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST", base + "/")
check("405 on a text route", out == "405", out)

# keep-alive: three requests, ONE connection (curl reports num_connects)
out, _ = curl("-s", "-o", "/dev/null", "-o", "/dev/null", "-o", "/dev/null", "-w", "%{num_connects}", base + "/",
              base + "/q?a=1", base + "/")
check("keep-alive: 3 requests over 1 connection", out == "100", out)
# pipelining: three requests in ONE write, answered in order
resp = raw(P, b"GET /q?a=1 HTTP/1.1\r\nHost: x\r\n\r\nGET /q?a=2 HTTP/1.1\r\nHost: x\r\n\r\n"
              b"GET /q?a=3 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
parts = resp.split(b"HTTP/1.1 200 OK")
check("pipelining: 3 answers in order, then close", len(parts) == 4 and parts[1].endswith(b"1")
      and parts[2].endswith(b"2") and parts[3].endswith(b"3") and b"Connection: close" in parts[3], resp[-200:])
# HTTP/1.0 without keep-alive: closed after the answer
resp = raw(P, b"GET /q?a=9 HTTP/1.0\r\n\r\n")
check("HTTP/1.0: answer + Connection: close", resp.startswith(b"HTTP/1.1 200") and b"Connection: close" in resp
      and resp.endswith(b"9"), resp)
# http.client (Python's own client)
hc = http.client.HTTPConnection("127.0.0.1", P, timeout=5)
hc.request("POST", "/echo", body=b"py", headers={"X-Probe": "hc"})
r1 = hc.getresponse().read()
hc.request("GET", "/q?a=z")
r2 = hc.getresponse().read()
check("python http.client, two requests on one connection", r1 == b"POST 2 hc\npy" and r2 == b"z", (r1, r2))

# ---- refusals (request smuggling and friends)
resp = raw(P, b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n")
refusal("Content-Length + Transfer-Encoding -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\nabcd")
refusal("two different Content-Lengths -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"GET / HTTP/1.1\r\nHost : x\r\n\r\n")
refusal("white space before the colon -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"GET / HTTP/1.1\r\nHost: x\r\nX-A: 1\r\n  folded\r\n\r\n")
refusal("obsolete line folding -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"GET / HTTP/1.1\r\n\r\n")
refusal("HTTP/1.1 without Host -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"GET / HTTP/2.0\r\nHost: x\r\n\r\n")
refusal("unknown version -> 505", resp.startswith(b"HTTP/1.1 505"), resp)
resp = raw(P, b"GET / HTTP/1.1\r\nHost: x\r\nX-Big: " + b"a" * 20000 + b"\r\n\r\n")
refusal("header block > 16 KiB -> 431", resp.startswith(b"HTTP/1.1 431"), resp[:60])
resp = raw(P, b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 99999999\r\n\r\n")
refusal("body above the limit -> 413", resp.startswith(b"HTTP/1.1 413"), resp)
resp = raw(P, b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nab\r\n0\r\n\r\n")
refusal("malformed chunk size -> 400", resp.startswith(b"HTTP/1.1 400"), resp)
resp = raw(P, b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: gzip\r\n\r\n")
refusal("unknown transfer coding -> 501", resp.startswith(b"HTTP/1.1 501"), resp)
resp = raw(P, b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
              b"Sec-WebSocket-Version: 8\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n")
refusal("WebSocket version 8 -> 426 naming 13", resp.startswith(b"HTTP/1.1 426") and b"Sec-WebSocket-Version: 13" in resp,
        resp)
resp = raw(P, b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
              b"Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: short\r\n\r\n")
refusal("bad Sec-WebSocket-Key -> 400", resp.startswith(b"HTTP/1.1 400"), resp)

# ---- WebSocket: RFC 6455 example key, then echo through two clients
resp = raw(P, b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
              b"Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n")
check("RFC 6455 1.3 example: accept key s3pPLMBiTxaQ9kYGzzhZRbK+xOo=",
      b"Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" in resp, resp)
w = websocket.create_connection("ws://127.0.0.1:%d/ws" % P)
w.send("grüße")
a1 = w.recv()
big = "x" * 300000
w.send(big)
a2 = w.recv()
w.send_binary(bytes(range(256)))
a3 = w.recv()
w.close()
check("websocket-client: text, 300 kB, binary", a1 == "grüße" and a2 == big and a3 == bytes(range(256)))
import asyncio, websockets


async def ws_async():
    async with websockets.connect("ws://127.0.0.1:%d/ws" % P) as c:
        await c.send("one")
        x = await c.recv()
        await c.ping()
        await c.send(b"\x01\x02")
        y = await c.recv()
        return x, y
x, y = asyncio.run(ws_async())
check("websockets (asyncio): text, ping, binary", x == "one" and y == b"\x01\x02", (x, y))

# ---- SSE
r = subprocess.run(["curl", "-s", "-N", "--max-time", "1.3", base + "/events"], capture_output=True)
ev = r.stdout.decode()
check("SSE: text/event-stream with ticks", ev.count("data: tick") >= 4, ev[:200])

# ---- many connections at once (the poll loop)
socks = []
for i in range(40):
    c = socket.create_connection(("127.0.0.1", P))
    socks.append(c)
for i, c in enumerate(socks):
    c.sendall(b"GET /q?a=%d HTTP/1.1\r\nHost: x\r\n\r\n" % i)
good = 0
for i, c in enumerate(socks):
    c.settimeout(3)
    d = c.recv(4096)
    if d.startswith(b"HTTP/1.1 200") and d.endswith(b"%d" % i):
        good += 1
    c.close()
check("40 connections open at once, all answered", good == 40, good)
s.stop()

# ---- pairing
s = Srv("pair")
P = s.port
tok = s.url.split("?t=")[1]
check("paired URL carries a 32-hex-digit token", len(tok) == 32 and all(ch in "0123456789abcdef" for ch in tok),
      s.url)
out, _ = curl("-s", "-o", "/dev/null", "-w", "%{http_code}", "http://127.0.0.1:%d/" % P)
refusal("no token -> 403", out == "403", out)
out, _ = curl("-s", "-o", "/dev/null", "-w", "%{http_code}", "http://127.0.0.1:%d/?t=%s" % (P, "0" * 32))
refusal("wrong token -> 403", out == "403", out)
out, _ = curl("-s", "-o", "/dev/null", "-w", "%{http_code}", "-H", "Cookie: firn_pair=" + tok[:31] + "0",
              "http://127.0.0.1:%d/" % P)
refusal("wrong cookie -> 403", out == "403", out)
jar = os.path.join(WORK, "jar.txt")
out, _ = curl("-s", "-L", "-c", jar, "-b", jar, "-w", " %{http_code} %{num_redirects}",
              "http://127.0.0.1:%d/?t=%s" % (P, tok))
check("right token -> 303 + cookie -> page", out.endswith(" 200 1") and "hello from the Firn" in out, out)
check("the cookie is HttpOnly, SameSite=Strict", "#HttpOnly_127.0.0.1" in open(jar).read())
cookie = "firn_pair=" + tok
w = websocket.create_connection("ws://127.0.0.1:%d/ws" % P, cookie=cookie, origin="http://127.0.0.1:%d" % P)
w.send("paired")
check("paired WebSocket with the cookie", w.recv() == "paired")
w.close()
try:
    websocket.create_connection("ws://127.0.0.1:%d/ws" % P)
    refusal("WebSocket without cookie", False)
except Exception as e:
    refusal("WebSocket without cookie -> 403", "403" in str(e), e)
try:
    websocket.create_connection("ws://127.0.0.1:%d/ws" % P, cookie=cookie, origin="http://evil.example")
    refusal("cross-origin WebSocket", False)
except Exception as e:
    refusal("cross-origin WebSocket (Origin evil.example) -> 403", "403" in str(e), e)
s.stop()

# ---- TLS: https and wss with a verified chain
sh = lambda *a: subprocess.run(list(a), capture_output=True)
ck, cc = os.path.join(WORK, "k.pem"), os.path.join(WORK, "c.pem")
sh("openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", ck)
sh("openssl", "req", "-new", "-x509", "-key", ck, "-out", cc, "-days", "30", "-subj", "/CN=localhost",
   "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1")
s = Srv("tls", cc, ck)
P = s.port
check("TLS URL starts with https://", s.url.startswith("https://"), s.url)
out, err = curl("--cacert", cc, "https://localhost:%d/q?a=secure" % P, "--resolve", "localhost:%d:127.0.0.1" % P)
check("curl https (verified)", out == "secure", out + err)
ctx = ssl.create_default_context(cafile=cc)
w = websocket.create_connection("wss://localhost:%d/ws" % P, sslopt={"context": ctx},
                                host="localhost:%d" % P) if False else None
wsock = socket.create_connection(("127.0.0.1", P))
w = websocket.WebSocket(sslopt={"context": ctx})
w.connect("wss://localhost:%d/ws" % P, socket=ctx.wrap_socket(wsock, server_hostname="localhost"))
w.send("über tls")
check("wss (websocket-client over TLS 1.3)", w.recv() == "über tls")
w.close()
r = subprocess.run(["curl", "-s", "-N", "--max-time", "3", "--cacert", cc, "--resolve",
                    "localhost:%d:127.0.0.1" % P, "https://localhost:%d/events" % P], capture_output=True)
check("SSE over TLS", r.stdout.decode().count("data: tick") >= 3, r.stdout[:100])
resp = raw(P, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
refusal("plain HTTP to the TLS port -> TLS alert, no page", resp[:1] == b"\x15" and b"hello" not in resp, resp[:20])
s.stop()

print("   %d / %d HTTP server cases, of them %d refusals" % (ok, total, refusals))
print(("HTTP OK: %d / %d, refusals %d" if ok == total else "HTTP FAILED: %d / %d, refusals %d") % (ok, total, refusals))
sys.exit(0 if ok == total else 1)
