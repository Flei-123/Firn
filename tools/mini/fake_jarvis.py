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
# REST routes of the Android app client (net.jarvisapi, additive; every call is logged
# as {"type":"rest",...}): /api/me, /api/projects[/<id>[/roadmap|/needs]], /api/command-center,
# /api/conversations (list), /api/tasks, /api/remote/devices, /api/push-token, /api/stt,
# /api/m/upload (raw body), /file/<uid>/<cid>/<name>, /api/projects/broken (cut JSON).
import asyncio, json, sys, time
from urllib.parse import unquote, urlparse, parse_qs, quote
from http import HTTPStatus
from websockets.asyncio.server import serve
from websockets.http11 import Response
from websockets.datastructures import Headers

TOKEN = "fake.tok+en=1_x"
LOG = open(sys.argv[1], "a", buffering=1)
CONV = {}      # cid -> list of {role, text}
seq = 0
CONV["c1"] = [{"role": "user", "text": "hi", "at": "2026-10-17T18:00:00Z"}, {"role": "assistant", "text": "hello\n>STATUS: status=done", "at": "2026-10-17T18:00:05Z"}]

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

# ------------------------------------------------------------------ REST (app client)
PROJECTS = {
    "p1": {"id": "p1", "name": "Jarvis \"A\" ä€\U0001F600", "status": "active", "goal": "assistant", "next": "restart",
           "updatedAt": "2026-10-17T18:00:00Z",
           "roadmap": [{"id": "r1", "text": "first", "group": "Git", "done": False, "note": ""},
                       {"id": "r2", "text": "second\nline", "group": "", "done": True, "note": "n"}],
           "needs": [{"id": "n1", "text": "Merge?", "options": ["Yes", "No"], "cid": "c1", "done": False},
                     {"id": "n2", "text": "Free text?", "options": [], "cid": None, "done": False}],
           "chats": [{"id": "c1", "title": "worker one", "running": True}]},
    "p 2/x": {"id": "p 2/x", "name": "Odd id", "status": "paused", "goal": "", "next": "", "updatedAt": "2026-10-16T10:00:00Z",
              "roadmap": [], "needs": [], "chats": []},
}
TASKS = [{"id": "t1", "text": "buy milk", "due": "2026-10-18T09:00", "category": "Private", "repeat": "none", "done": False, "created": 1}]
FILES = {}     # disk name -> bytes
tseq = 1

def pstats(p):
    tot = len(p["roadmap"]); dn = len([r for r in p["roadmap"] if r["done"]])
    return {"total": tot, "done": dn, "open": tot - dn, "pct": round(dn / tot * 100) if tot else 0,
            "needs": len([n for n in p["needs"] if not n["done"]])}

def psummary(p):
    return {k: p[k] for k in ("id", "name", "status", "goal", "next", "updatedAt")} | pstats(p)

def pfull(p):
    return psummary(p) | {"roadmap": p["roadmap"], "needs": p["needs"], "chats": p["chats"]}

def rest(method, rawpath, hdr, body):
    """-> (status, bytes, ctype) or None (not a REST route of the app client)."""
    global tseq
    u = urlparse(rawpath)
    path = u.path
    q = parse_qs(u.query)
    if not (path in ("/api/me", "/api/projects", "/api/command-center", "/api/conversations", "/api/tasks",
                     "/api/remote/devices", "/api/push-token", "/api/stt", "/api/m/upload")
            or path.startswith("/api/projects/") or path.startswith("/file/")):
        return None
    jtype = hdr.get("content-type")
    entry = {"type": "rest", "method": method, "path": rawpath, "ctype": jtype, "n": len(body)}
    cookie = [c for c in hdr.get("cookie", "").split(";")]
    ok = any(c.strip().partition("=")[0] == "fleitec_session" and unquote(c.strip().partition("=")[2]) == TOKEN for c in cookie)
    entry["auth"] = ok
    j = None
    if jtype and "json" in jtype and body:
        try: j = json.loads(body)
        except Exception: j = None
        entry["json"] = j
    LOG.write(json.dumps(entry) + "\n")
    def out(st, obj): return (st, json.dumps(obj).encode(), "application/json")
    if not ok:
        return out(401, {"error": "Not logged in"})
    if path == "/api/me":
        return out(200, {"username": "justin", "admin": True, "deviceToken": "dt1", "settings": {}})
    if path == "/api/projects" and method == "GET":
        return out(200, {"projects": [psummary(p) for p in PROJECTS.values()], "linked": {}, "stale": [{"id": "c9"}]})
    if path.startswith("/api/projects/"):
        parts = [unquote(x) for x in path[len("/api/projects/"):].split("/")]
        if parts[0] == "broken":
            return (200, b'{"project":{"id":"p1","road', "application/json")
        p = PROJECTS.get(parts[0])
        if p is None:
            return out(404, {"error": "No project"})
        if len(parts) == 1:
            if method == "POST" and j:
                if j.get("status"): p["status"] = j["status"]
                if j.get("next"): p["next"] = j["next"]
            return out(200, {"project": pfull(p)})
        if parts[1] == "roadmap" and method == "POST" and j:
            a = j.get("action")
            if a in ("done", "undo"):
                for r in p["roadmap"]:
                    if r["id"] == j.get("id"): r["done"] = (a == "done")
            elif a == "add":
                p["roadmap"].append({"id": "r%d" % (len(p["roadmap"]) + 1), "text": j.get("text", ""), "group": "", "done": False, "note": ""})
            elif a == "remove":
                p["roadmap"] = [r for r in p["roadmap"] if r["id"] != j.get("id")]
            else:
                return out(400, {"error": "Unknown action"})
            return out(200, {"project": pfull(p)})
        if parts[1] == "needs" and method == "POST" and j:
            for n in p["needs"]:
                if n["id"] == j.get("id") and j.get("action") == "answer":
                    n["done"] = True; n["answer"] = j.get("answer", "")
            return out(200, {"project": pfull(p)})
        return out(404, {"error": "Unknown"})
    if path == "/api/command-center":
        return out(200, {"now": 1760000000000, "user": "justin",
            "workers": [{"id": "c1", "title": "worker one", "running": True, "updatedAt": 1760000000001, "last": "doing", "partial": "par"}],
            "orchestrator": {"id": "o1", "title": "Leitstand", "running": False, "updatedAt": 5}, "otherChatCount": 7,
            "devices": ["Dell", "Phone"], "deviceInfo": [], "tasks": [{"id": "t1", "text": "buy milk", "due": None, "category": None, "repeat": None, "overdue": True}],
            "sys": {"cpus": 4, "load": [0.5, 0.4, 0.3], "uptime": 99, "memTotal": 8000000000, "memFree": 1000000000,
                    "disk": {"total": 100, "free": 40}, "host": "srv"},
            "memory": {"count": 12, "knowledge": []},
            "activity": [{"cid": "c1", "conv": "worker one", "kind": "chat", "role": "assistant", "at": "2026-10-17T18:00:00Z", "text": "hi"}]})
    if path == "/api/conversations":
        rows = [{"id": "c1", "title": "worker one", "kind": "worker", "updatedAt": 1760000000001, "fav": True},
                {"id": "o1", "title": "Leitstand", "kind": "orchestrator", "updatedAt": 5}]
        if "limit" in q:
            rows = rows[int(q.get("offset", ["0"])[0]):][:int(q["limit"][0])]
        return out(200, {"conversations": rows, "total": 2, "rev": "x", "folders": []})
    if path == "/api/tasks":
        if method == "POST" and j:
            a = j.get("action")
            if a == "add" and j.get("text"):
                tseq += 1
                TASKS.append({"id": "t%d" % tseq, "text": j["text"], "due": j.get("due"), "category": j.get("category"),
                              "repeat": j.get("repeat") or "none", "done": False, "created": tseq})
            else:
                for t in TASKS:
                    if j.get("ref") in (t["id"], t["text"]):
                        if a == "complete": t["done"] = True
                if a == "remove":
                    TASKS[:] = [t for t in TASKS if j.get("ref") not in (t["id"], t["text"])]
        return out(200, {"tasks": TASKS})
    if path == "/api/remote/devices":
        return out(200, {"devices": ["Dell", "Phone"], "deviceInfo": [{"name": "Dell", "type": "Windows", "online": True}, {"name": "Phone", "type": "Android", "online": False}]})
    if path == "/api/push-token":
        return out(200, {"ok": True, "pushEnabled": True})
    if path == "/api/stt":
        return out(200, {"text": "audio %d bytes %s lang=%s" % (len(body), jtype, q.get("lang", [""])[0])})
    if path == "/api/m/upload":
        name = q.get("name", ["file"])[0]
        disk = "ab12cd34ef56_" + name
        FILES[disk] = body
        return out(200, {"file": {"name": name, "size": len(body), "mime": jtype, "url": "/file/u1/%s/%s" % (q.get("cid", ["c0"])[0], quote(disk))}})
    if path.startswith("/file/"):
        disk = unquote(path.split("/")[-1])
        if disk not in FILES:
            return (404, b"not found", "text/plain")
        return (200, FILES[disk], "application/octet-stream")
    return None

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
        rr = rest(method, path, hdr, body)
        if rr is not None:
            st, data, ct = rr
            writer.write(("HTTP/1.1 %d X\r\nContent-Type: %s\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % (st, ct, len(data))).encode() + data)
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
