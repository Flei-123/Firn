#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/mini/fake_jarvis.py <logfile> -- a stand-in for the JARVIS chat server:
# POST /api/login, GET /api/conversations/<cid>, and the WebSocket
# /?role=ui&dev=<id> with the events of public/mini.html. Prints "PORT <n>".
# Every message the client sends is appended to <logfile> as one JSON line.
# The text of a message picks the behaviour:
#   confirm  -> confirm_request, waits for confirm_result
#   ask      -> ask_request (options A, B), waits for ask_result
#   slow     -> tokens until a "stop" arrives, then done
#   other    -> answer, then a "user" event as if typed on another device
#   drop     -> the server closes the socket (client must reconnect)
#   else     -> thinking, tool, "Hallo " + text split in two tokens, done
import asyncio, json, sys
from urllib.parse import unquote, urlparse, parse_qs
from http import HTTPStatus
from websockets.asyncio.server import serve
from websockets.http11 import Response
from websockets.datastructures import Headers

TOKEN = "fake.tok+en=1_x"
LOG = open(sys.argv[1], "a", buffering=1)
CONV = {}      # cid -> list of {role, text}
seq = 0

def cookie_ok(headers):
    for c in headers.get_all("Cookie"):
        for part in c.split(";"):
            k, _, v = part.strip().partition("=")
            if k == "fleitec_session" and unquote(v) == TOKEN:
                return True
    return False

def reply(status, obj, extra=None):
    body = json.dumps(obj).encode()
    h = Headers([("Content-Type", "application/json"), ("Content-Length", str(len(body))), ("Connection", "close")])
    for k, v in (extra or []):
        h[k] = v
    return Response(status, HTTPStatus(status).phrase, h, body)

async def process_request(conn, request):
    u = urlparse(request.path)
    if u.path.startswith("/api/conversations/"):
        if not cookie_ok(request.headers):
            return reply(401, {"error": "Not logged in"})
        cid = unquote(u.path[len("/api/conversations/"):])
        if cid not in CONV:
            return reply(404, {"error": "Not found"})
        return reply(200, {"conversation": {"messages": CONV[cid]}, "running": False, "partial": ""})
    if u.path == "/":
        if "websocket" not in request.headers.get("Upgrade", "").lower():
            return reply(404, {"error": "no"})
        if not cookie_ok(request.headers):
            return reply(401, {"error": "unauthorized"})
        return None
    return reply(404, {"error": "no"})

async def send(ws, obj):
    await ws.send(json.dumps(obj))

async def handler(ws):
    global seq
    q = asyncio.Queue()
    await send(ws, {"type": "info", "user": "justin", "running": []})
    async for raw in ws:
        m = json.loads(raw)
        LOG.write(json.dumps(m) + "\n")
        t = m.get("type")
        if t == "message":
            text = m["text"]
            cid = m.get("cid")
            if not cid:
                seq += 1
                cid = "fake%d" % seq
                CONV[cid] = []
                await send(ws, {"type": "conversation", "id": cid, "title": "t"})
            CONV[cid].append({"role": "user", "text": text})
            await send(ws, {"type": "user", "cid": cid, "text": text})  # echo of my own message
            if text == "drop":
                await ws.close(1011)
                return
            if text == "confirm":
                await send(ws, {"type": "confirm_request", "id": "c1", "prompt": "Run it?"})
                r = json.loads(await ws.recv()); LOG.write(json.dumps(r) + "\n")
                await send(ws, {"type": "token", "cid": cid, "content": "allow=%s" % str(r.get("allow")).lower()})
            elif text == "ask":
                await send(ws, {"type": "ask_request", "id": "a1", "cid": cid, "question": "Which?", "options": ["A", {"label": "B"}]})
                r = json.loads(await ws.recv()); LOG.write(json.dumps(r) + "\n")
                await send(ws, {"type": "token", "cid": cid, "content": "ans=%s" % r["answers"][0]})
            elif text == "slow":
                while True:
                    await send(ws, {"type": "token", "cid": cid, "content": "."})
                    try:
                        r = json.loads(await asyncio.wait_for(ws.recv(), 0.05))
                        LOG.write(json.dumps(r) + "\n")
                        if r.get("type") == "stop":
                            break
                    except asyncio.TimeoutError:
                        pass
            else:
                await send(ws, {"type": "thinking", "cid": cid})
                await send(ws, {"type": "tool", "cid": cid, "name": "mcp__jarvis__run_shell"})
                await send(ws, {"type": "token", "cid": cid, "content": "Hallo "})
                await send(ws, {"type": "token", "cid": cid, "content": text + "\n>STATUS: status=done"})
            await send(ws, {"type": "done", "cid": cid})
            CONV[cid].append({"role": "assistant", "text": "reply to " + text + "\n>STATUS: status=done"})
            if text == "other":
                await send(ws, {"type": "user", "cid": cid, "text": "from phone"})
                await send(ws, {"type": "token", "cid": "otherchat", "content": "IGNORED"})

async def main():
    # a raw HTTP front for POST /api/login (websockets parses GET only), the
    # WebSocket server behind it on a second port
    async def front(reader, writer):
        head = await reader.readuntil(b"\r\n\r\n")
        lines = head.decode().split("\r\n")
        method, path, _ = lines[0].split(" ", 2)
        hdr = {l.split(":", 1)[0].lower(): l.split(":", 1)[1].strip() for l in lines[1:] if ":" in l}
        body = b""
        if "content-length" in hdr:
            body = await reader.readexactly(int(hdr["content-length"]))
        if method == "POST" and path == "/api/login":
            try:
                j = json.loads(body)
            except Exception:
                j = {}
            LOG.write(json.dumps({"type": "login", "user": j.get("username"), "ctype": hdr.get("content-type")}) + "\n")
            if j.get("username") == "justin" and j.get("password") == "pw":
                out = json.dumps({"username": "justin", "token": TOKEN}).encode()
                st = "200 OK"
            else:
                out = json.dumps({"error": "Wrong user name or password"}).encode()
                st = "401 Unauthorized"
            writer.write(("HTTP/1.1 %s\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % (st, len(out))).encode() + out)
            await writer.drain(); writer.close(); return
        # everything else: tunnel to the websockets server
        r2, w2 = await asyncio.open_connection("127.0.0.1", WSPORT)
        w2.write(head + body)
        await w2.drain()
        async def pipe(a, b):
            try:
                while True:
                    d = await a.read(65536)
                    if not d:
                        break
                    b.write(d); await b.drain()
            except Exception:
                pass
            try: b.close()
            except Exception: pass
        await asyncio.gather(pipe(reader, w2), pipe(r2, writer))
    global WSPORT
    srv = await serve(handler, "127.0.0.1", 0, process_request=process_request, compression=None)
    WSPORT = srv.sockets[0].getsockname()[1]
    fs = await asyncio.start_server(front, "127.0.0.1", 0)
    print("PORT", fs.sockets[0].getsockname()[1], flush=True)
    async with fs:
        await fs.serve_forever()

asyncio.run(main())
