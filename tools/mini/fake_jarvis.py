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
# Push: GET /api/m/push/poll?since=N&wait=S is a real long poll (wait capped by FAKE_POLL_MAX, default 5 s); a test injects an event
# with POST /fake/push {"title","body","cid","type"} (no login) -> {"id": N}. Event ids start at 1000 (FAKE_EVENT_BASE).
# BAD NETWORK (REST only; the WebSocket keeps working): POST /fake/net {"delay_ms":3000,"drop":1000000,"status":503} slows the REST
# answers, drops the connection without an answer (drop = how many requests; 1000000 = all) or answers every data route with that
# status (ping stays 200). Start values: FAKE_REST_DELAY_MS / FAKE_REST_DROP / FAKE_REST_STATUS.
# Text "file" in a chat message -> the server sends a file event (report.txt) that the client fetches over /file/.
# REALISTIC DATA (default on, FAKE_REALISTIC=0 = the old tiny answers): the WebSocket `info` is shaped like the real server's
# (server.js ~4154: active_model, efforts, devices, deviceInfo, models, modelIds, caps, labels, providers, running, partials,
# runStarts, beats, asks, newModels) with FAKE_NRUNNING (30) running chats whose live text `partials` are FAKE_PARTIAL_MIN_KB..
# FAKE_PARTIAL_MAX_KB (50..200) each = a few MB; every second WS connection sends it ASCII-escaped (\\ud83d\\ude00 pairs), the others
# as raw UTF-8. /api/m/chats lists FAKE_NCHATS (150) chats (emoji, umlauts, 300-char titles, null/empty titles, null parents) and
# /api/m/chats/<id> serves any of them with big answers (code, tables, CJK, emoji, control chars). FAKE_STREAM=1: after `info` the
# server streams tokens of the other running chats all the time (the client must skip them).
import asyncio, json, os, random, sys, time
from urllib.parse import unquote, urlparse, parse_qs, quote
from http import HTTPStatus
from websockets.asyncio.server import serve
from websockets.http11 import Response
from websockets.datastructures import Headers

TOKEN = "fake.tok+en=1_x"
# bad network on purpose, REST only (POST /fake/net changes it, or the FAKE_REST_* environment at start)
NET = {"delay_ms": int(os.environ.get("FAKE_REST_DELAY_MS", "0")), "drop": int(os.environ.get("FAKE_REST_DROP", "0")),
       "status": int(os.environ.get("FAKE_REST_STATUS", "0"))}
LOG = open(sys.argv[1], "a", buffering=1)
CONV = {}      # cid -> list of {role, text}
seq = 0
CONV["c1"] = [{"role": "user", "text": "hi", "at": "2026-10-17T18:00:00Z"}, {"role": "assistant", "text": "hello\n>STATUS: status=done", "at": "2026-10-17T18:00:05Z"}]

CONV["c2"] = [{"role": "user", "text": "second question", "at": "2026-10-17T19:00:00Z"}, {"role": "assistant", "text": "second answer\n>STATUS: status=done", "at": "2026-10-17T19:00:05Z"}]


# ------------------------------------------------------------------ realistic data (shapes of the real server)
REAL = os.environ.get("FAKE_REALISTIC", "1") != "0"
NCHATS = int(os.environ.get("FAKE_NCHATS", "150"))
NRUNNING = int(os.environ.get("FAKE_NRUNNING", "30"))
PMIN = int(os.environ.get("FAKE_PARTIAL_MIN_KB", "50")) * 1024
PMAX = int(os.environ.get("FAKE_PARTIAL_MAX_KB", "200")) * 1024
STREAM = os.environ.get("FAKE_STREAM", "0") == "1"
MODELS = ["opus", "sonnet", "haiku", "fable", "hy3", "glm"]
SNIPPETS = [
    "Wird erledigt, Boss. Größe: 42 MB, Übergabe über Straße: ä ö ü Ä Ö Ü ß €.\n",
    "```js\nconst a = { \"k\": [1, 2, 3], 'q': `tpl ${x}` }; // \\ backslash \\\\ double\n```\n",
    "| Spalte | Wert | Notiz |\n|---|---|---|\n| ä | \U0001F600 | ✅ |\n| ö | \U0001F680 | ❌ |\n",
    "Emoji: \U0001F600 \U0001F680 \U0001F468‍\U0001F469‍\U0001F467‍\U0001F466 \U0001F1E6\U0001F1F9 ✅ ❌ ⚠️ zero​width rtl‮abc\n",
    "CJK: 你好，世界。日本語のテキスト。한국어 텍스트 Русский текст العربية\n",
    "Tab\there, CRLF here\r\nquotes \"x\" and a literal \\u00e4 and a literal \\n and \\\\ and / slash\n",
    "> quote line\n- list a\n- list b\n  1. nested\n    - deeper\n",
    "# Heading\n**bold** _it_ `code` [link](https://example.com/?a=1&b=2) ~~strike~~\n",
    "<b>html</b> &amp; </script> <!-- x --> <img src=x onerror=1>\n",
    "ctrl \x01\x02\x1f del \x7f nul-ish \x0b\x0c end\n",
    "Astral \U0001d514\U0001d52b\U0001d526\U0001d520\U0001d52c\U0001d521\U0001d522 \U00010348 \U00020b9e plane chars\n",
    "x" * 3000 + "\n",
    "Wort " * 400 + "\n",
]

def rich_text(rng, n):
    out = []; tot = 0
    while tot < n:
        sn = rng.choice(SNIPPETS)
        out.append(sn); tot += len(sn.encode("utf-8"))
    return "".join(out)[:n].rstrip("\ud800") if n else ""

def chat_title(i):
    k = i % 10
    if k == 1: return "\U0001F680 Größe Übergabe äöü – Emoji \U0001F600 Chat %d" % i
    if k == 2: return ("Sehr langer Titel " * 20 + str(i))[:300]
    if k == 3: return None
    if k == 4: return ""
    if k == 5: return "你好世界 日本語 %d" % i
    if k == 6: return "Quote \"x\" \\ back / slash <b> %d" % i
    if k == 7: return "\U0001F468‍\U0001F469‍\U0001F467 family %d" % i
    return "worker task %d" % i

def chat_row(i):
    return {"id": "w%03d" % i, "title": chat_title(i), "kind": "worker" if i % 3 else "chat",
            "updatedAt": 1760000000000 - i * 60000, "running": i < NRUNNING,
            "fav": i % 11 == 0, "parent": None if i % 4 else "o1"}

def sidebar_chats():
    # FAKE_SIDEBAR=1: the chats of the web sidebar reference (AKTIV / HEUTE / GESTERN, sub-workers under OrientOS, a Workers tab)
    now = int(time.time() * 1000)
    h = 3600000
    def row(i, t, ago, run=False, kind="chat", disp=False, parent=None, fav=False):
        return {"id": i, "title": t, "kind": kind, "updatedAt": now - ago, "running": run, "fav": fav, "parent": parent, "dispatched": disp}
    return [row("o1", "Leitstand", 0, kind="orchestrator"),
            row("s1", "Certus", 1000, True), row("s2", "OrientOS", 2000),
            row("s2a", "OrientOS: Treiber", 3000, True, "worker", True, "s2"), row("s2b", "OrientOS: Netz", 4000, False, "worker", True, "s2"),
            row("s2c", "OrientOS: Boot", 5000, False, "worker", True, "s2"),
            row("s3", "Firn", 2 * h), row("s4", "fUi", 3 * h),
            row("s5", "Jarvis", 30 * h), row("s6", "Minecraft", 31 * h), row("s7", "Fleitec All-in-One", 32 * h),
            row("s8", "Diplomarbeit", 100 * h), row("s9", "Anzug-KI", 24 * 20 * h), row("s10", "Russisch", 24 * 90 * h),
            row("w1", "Daidalos: Render", 6 * h, True, "worker", True)]

def all_chats():
    if os.environ.get("FAKE_SIDEBAR") == "1":
        return sidebar_chats()
    base = [{"id": "c1", "title": "worker one", "kind": "worker", "updatedAt": 1760000000001, "running": True, "fav": True, "parent": None},
            {"id": "c2", "title": "worker two", "kind": "worker", "updatedAt": 1760000000002, "running": False, "fav": False, "parent": None},
            {"id": "o1", "title": "Leitstand", "kind": "orchestrator", "updatedAt": 5, "running": False, "fav": False, "parent": None}]
    if not REAL:
        return base
    return base + [chat_row(i) for i in range(max(0, NCHATS - 3))]

def gen_messages(cid):
    seed = sum(ord(ch) * (k + 1) for k, ch in enumerate(cid))
    rng = random.Random(seed)
    n = 8 + seed % 40
    msgs = []
    for i in range(n):
        role = "user" if i % 2 == 0 else "assistant"
        size = rng.choice([40, 200, 1500, 6000, 6000]) if role == "assistant" else rng.choice([20, 80, 300])
        txt = rich_text(rng, size)
        if role == "assistant":
            txt += "\n>STATUS: status=done braucht_nutzer=\"\" naechster_schritt=\"ä\"" 
        m = {"role": role, "text": txt, "at": 1760000000000 + i * 1000}
        if i == 3: m["images"] = [{"url": "/file/u1/%s/img.png" % cid}]
        msgs.append(m)
    return msgs

def real_info(conn_no):
    running = ["c1"] + ["w%03d" % i for i in range(max(0, NRUNNING - 1))]
    rng = random.Random(1000 + conn_no)
    partials = {cid: rich_text(rng, rng.randint(PMIN, PMAX)) for cid in running if cid != "c1"}
    now = int(time.time() * 1000)
    info = {"type": "info", "user": "justin", "active_model": "opus", "active_effort": "max", "thinking": False,
            "efforts": ["low", "medium", "high", "max"],
            "devices": ["Dell", "OrientOS-VM", "HOLO-LAPTOP-3"],
            "deviceInfo": [{"name": "Dell", "type": "Windows", "online": True}, {"name": "OrientOS-VM", "type": "Windows", "online": True},
                           {"name": "HOLO-LAPTOP-3", "type": "Windows", "online": True}, {"name": "Pixel ä\U0001F600", "type": None, "online": False}],
            "models": MODELS, "modelIds": {m: "claude-%s-5-5" % m for m in MODELS},
            "caps": {m: {"thinking": m in ("opus", "fable"), "effort": m != "haiku", "vision": m != "glm"} for m in MODELS},
            "labels": {m: {"label": m.capitalize() + " ★", "blurb": "Modell „%s“ für alles \U0001F680" % m} for m in MODELS},
            "providers": {m: ("anthropic" if m in ("opus", "sonnet", "haiku", "fable") else "other") for m in MODELS},
            "running": running, "partials": partials,
            "runStarts": {cid: now - 1000 * (7 + k) for k, cid in enumerate(running)},
            "beats": {cid: {"startedAt": now - 5000 * (k + 1), "idleMs": 120 * k, "kind": ("tool" if k % 2 else "text"),
                            "name": (None if k % 3 == 0 else "mcp__jarvis__run_shell"), "note": ("" if k % 4 else "warte auf ä\U0001F600"), "sleepMs": 0}
                      for k, cid in enumerate(running)},
            "asks": [{"id": "ask%d" % k, "cid": running[k], "workerCid": None if k == 0 else running[k + 1],
                      "questions": [{"question": "Welche Variante ä \U0001F600?", "options": ["A", {"label": "B ü"}, None], "multi": k == 1}]}
                     for k in range(2)],
            "newModels": ["gamma-1", None]}
    return info

def info_text(conn_no):
    # even connection numbers: raw UTF-8 like node's JSON.stringify; odd: ASCII-escaped (surrogate pairs as \\ud83d\\ude00)
    if not REAL:
        return json.dumps({"type": "info", "user": "justin", "running": []})
    return json.dumps(real_info(conn_no), ensure_ascii=(conn_no % 2 == 1))
CONNS = 0

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
    "p1": {"id": "p1", "name": "Jarvis \"A\" ä€\U0001F600", "status": "active", "goal": "assistant", "next": "restart",  # english: ok
           "updatedAt": "2026-10-17T18:00:00Z",
           "roadmap": [{"id": "r1", "text": "first", "group": "Git", "done": False, "note": ""},
                       {"id": "r2", "text": "second\nline", "group": "", "done": True, "note": "n"}],
           "needs": [{"id": "n1", "text": "Merge?", "options": ["Yes", "No"], "cid": "c1", "done": False},
                     {"id": "n2", "text": "Free text?", "options": [], "cid": None, "done": False}],
           "chats": [{"id": "c1", "title": "worker one", "running": True}]},
    "p 2/x": {"id": "p 2/x", "name": "Odd id", "status": "paused", "goal": "", "next": "", "updatedAt": "2026-10-16T10:00:00Z",
              "roadmap": [], "needs": [], "chats": []},
}
# FAKE_NPROJECTS=N adds projects g1..gN with roadmaps and open needs (the tab bar must stay visible with 16+ projects)
for _i in range(1, int(os.environ.get("FAKE_NPROJECTS", "0")) + 1):
    PROJECTS["g%d" % _i] = {"id": "g%d" % _i, "name": "Project number %d with a longish name" % _i, "status": "active",
        "goal": "goal %d" % _i, "next": "next step of project %d, a sentence that runs a bit long" % _i, "updatedAt": "2026-10-17T18:00:00Z",
        "roadmap": [{"id": "r%d" % k, "text": "item %d" % k, "group": "G", "done": k % 3 == 0, "note": ""} for k in range(1, 9)],
        "needs": [{"id": "n%d" % k, "text": "Question %d of project %d?" % (k, _i), "options": ["Yes", "No"], "cid": None, "done": False}
                  for k in range(1, 9)],
        "chats": []}
TASKS = [{"id": "t1", "text": "buy milk", "due": "2026-10-18T09:00", "category": "Private", "repeat": "none", "done": False, "created": 1}]
FILES = {}     # disk name -> bytes
EVENTS = []    # push events (poll feed)
EVSEQ = int(os.environ.get("FAKE_EVENT_BASE", "1000"))
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
            or path.startswith("/api/projects/") or path.startswith("/api/m/") or path.startswith("/file/")):
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
    if path.startswith("/api/m/") and path != "/api/m/upload" and os.environ.get("FAKE_NO_MOBILE"):
        return out(404, {"error": "Not found"})  # an older server without the mobile routes
    if path == "/api/m/ping":
        return out(200, {"ok": True, "api": 1, "time": 1760726400000, "push": {"tcp": True, "host": "127.0.0.1", "port": 7781, "poll": True}})
    if not ok:
        return out(401, {"error": "Not logged in"})
    if path.startswith("/api/m/") and path != "/api/m/upload":
        mp = path[len("/api/m/"):]
        if mp == "projects" and method == "GET":
            return out(200, {"projects": [psummary(p) for p in PROJECTS.values()]})
        if mp.startswith("projects/") and method == "GET":
            p = PROJECTS.get(unquote(mp[len("projects/"):]))
            return out(200, {"project": pfull(p)}) if p else out(404, {"error": "No project"})
        if mp == "needs/answer" and method == "POST" and j:
            for p in PROJECTS.values():
                if j.get("project") in (p["id"], p.get("name")):
                    for n in p["needs"]:
                        if n["id"] == j.get("id") and not n.get("done"):
                            n["done"] = True; n["answer"] = j.get("answer", "")
                            return out(200, {"ok": True})
            return out(400, {"error": "Need not found or already done."})
        if mp == "tasks" and method == "GET":
            return out(200, {"tasks": TASKS})
        if mp == "tasks" and method == "POST" and j and j.get("text"):
            tseq += 1
            t = {"id": "t%d" % tseq, "text": j["text"], "due": j.get("due"), "category": j.get("category"),
                 "repeat": j.get("repeat") or "none", "done": False, "created": tseq}
            TASKS.append(t)
            return out(200, {"task": t})
        if mp in ("tasks/done", "tasks/remove") and method == "POST" and j:
            for t in TASKS:
                if t["id"] == j.get("id") and mp == "tasks/done": t["done"] = True
            if mp == "tasks/remove":
                TASKS[:] = [t for t in TASKS if t["id"] != j.get("id")]
            return out(200, {"ok": True})
        if mp == "devices":
            return out(200, {"devices": [{"name": "Dell", "type": "Windows", "online": True}, {"name": "Phone", "type": "Android", "online": False}]})
        if mp == "chats" and method == "GET":
            allc = all_chats()
            if not REAL:
                return out(200, {"total": len(allc), "chats": allc})
            lim = max(1, min(100, int(q.get("limit", ["30"])[0] or 30))); off = max(0, int(q.get("offset", ["0"])[0] or 0))
            qq = (q.get("q", [""])[0] or "").lower()
            if qq: allc = [c for c in allc if qq in str(c["title"] or "").lower()]
            return out(200, {"total": len(allc), "chats": allc[off:off + lim]})
        if mp.startswith("chats/") and method == "GET":
            cid = unquote(mp[len("chats/"):])
            row = next((c for c in all_chats() if c["id"] == cid), None)
            if cid not in CONV and not (REAL and row):
                return out(404, {"error": "Not found"})
            msgs = CONV[cid] if cid in CONV else gen_messages(cid)
            lim = max(1, min(100, int(q.get("limit", ["30"])[0] or 30)))
            end = len(msgs)
            if "before" in q: end = max(0, min(end, int(q["before"][0] or 0)))
            lo = max(0, end - lim)
            part = ""
            if REAL and row and row.get("running") and cid != "c1":
                part = rich_text(random.Random(7), 6000)
            return out(200, {"chat": {"id": cid, "title": (row or {}).get("title", "worker one") if row else "worker one", "kind": (row or {}).get("kind", "worker"),
                                      "running": False if not part else True, "partial": part, "beat": None},
                             "first": lo, "hasMore": lo > 0,
                             "messages": [dict(m, i=i, at=1760000000000 + i) for i, m in enumerate(msgs) if lo <= i < end]})
        if mp == "push/register" and method == "POST":
            return out(200, {"pushToken": "00112233445566778899aabbccddeeff", "relay": {"host": "127.0.0.1", "port": 7781, "tls": False, "pin": None, "hello": True, "sni": None}, "poll": "/api/m/push/poll"})
        if mp == "push/test" and method == "POST":
            return out(200, {"ok": True, "sent": 1})
        return out(404, {"error": "Not found"})
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
        import hashlib  # the proof for the file picker check: what the server GOT (name, mime, size, sha256 of the octets)
        LOG.write(json.dumps({"type": "upload_body", "name": name, "mime": q.get("mime", [jtype])[0], "size": len(body), "sha256": hashlib.sha256(body).hexdigest()}) + "\n")
        return out(200, {"file": {"name": name, "size": len(body), "mime": jtype, "url": "/file/u1/%s/%s" % (q.get("cid", ["c0"])[0], quote(disk)), "path": "/srv/uploads/u1/%s/%s" % (q.get("cid", ["mobile"])[0], disk)}})
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
    global CONNS
    CONNS += 1
    itext = info_text(CONNS - 1)
    LOG.write(json.dumps({"type": "info_sent", "bytes": len(itext), "conn": CONNS}) + "\n")
    await ws.send(itext)
    streamer = None
    if STREAM and REAL:
        async def pump():
            rng = random.Random(CONNS)
            k = 0
            try:
                while True:
                    cid = "w%03d" % rng.randint(0, max(0, NRUNNING - 2))
                    k += 1
                    if k % 40 == 0: await send(ws, {"type": "thinking", "cid": cid})
                    elif k % 55 == 0: await send(ws, {"type": "tool", "cid": cid, "name": "mcp__jarvis__run_shell"})
                    else: await send(ws, {"type": "token", "cid": cid, "content": rich_text(rng, rng.randint(20, 600))})
                    await asyncio.sleep(0.02)
            except Exception:
                pass
        streamer = asyncio.ensure_future(pump())
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
            elif text == "file":
                FILES["report.txt"] = b"hello report from the fake server\n"
                await send(ws, {"type": "token", "cid": cid, "content": "Here is the file."})
                await send(ws, {"type": "file", "cid": cid, "name": "report.txt", "url": "/file/u1/%s/report.txt" % cid, "mime": "text/plain"})
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
        if method == "POST" and path == "/fake/net":
            # {"delay_ms":N,"drop":N (1000000 = always, 0 = off),"status":N (0 = off)}: the REST answers get slow / dropped / a fixed status
            try:
                j = json.loads(body)
            except Exception:
                j = {}
            for k in ("delay_ms", "drop", "status"):
                if k in j: NET[k] = int(j[k])
            out = json.dumps(NET).encode()
            writer.write(("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(out)).encode() + out)
            await writer.drain(); writer.close(); return
        if method == "POST" and path == "/fake/push":
            global EVSEQ
            try:
                j = json.loads(body)
            except Exception:
                j = {}
            EVSEQ += 1
            EVENTS.append({"id": EVSEQ, "type": j.get("type", "other"), "title": j.get("title", "JARVIS"), "body": j.get("body", ""),
                           "cid": j.get("cid"), "at": int(time.time() * 1000)})
            out = json.dumps({"id": EVSEQ}).encode()
            writer.write(("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(out)).encode() + out)
            await writer.drain(); writer.close(); return
        if method == "GET" and path.startswith("/api/m/push/poll"):
            pq = parse_qs(urlparse(path).query)
            since = int(pq.get("since", ["0"])[0] or 0)
            wait = min(int(pq.get("wait", ["0"])[0] or 0), int(os.environ.get("FAKE_POLL_MAX", "5")))
            cookie_ok_ = any(c.strip().partition("=")[0] == "fleitec_session" and unquote(c.strip().partition("=")[2]) == TOKEN
                             for c in hdr.get("cookie", "").split(";"))
            LOG.write(json.dumps({"type": "rest", "method": "GET", "path": path, "auth": cookie_ok_}) + "\n")
            if not cookie_ok_:
                out = b'{"error":"Not logged in"}'
                writer.write(("HTTP/1.1 401 X\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(out)).encode() + out)
                await writer.drain(); writer.close(); return
            t0 = time.time()
            while True:
                have = [e for e in EVENTS if e["id"] > since]
                if have or time.time() - t0 >= wait:
                    break
                await asyncio.sleep(0.1)
            out = json.dumps({"events": have, "last": have[-1]["id"] if have else since}).encode()
            writer.write(("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n" % len(out)).encode() + out)
            await writer.drain(); writer.close(); return
        rr = rest(method, path, hdr, body)
        if rr is not None:
            # a bad network on purpose (REST only, the WebSocket keeps working): slow answers, dropped connections, a fixed status
            if NET["delay_ms"] > 0:
                await asyncio.sleep(NET["delay_ms"] / 1000.0)
            if NET["drop"] > 0:
                NET["drop"] -= 1 if NET["drop"] < 1000000 else 0
                LOG.write(json.dumps({"type": "net", "what": "drop", "path": path}) + "\n")
                writer.close(); return
            if NET["status"] and path != "/api/m/ping":
                LOG.write(json.dumps({"type": "net", "what": "status", "path": path, "status": NET["status"]}) + "\n")
                rr = (NET["status"], b'{"error":"fake failure"}', "application/json")
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
