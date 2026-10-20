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
# a61: GET /api/m/chats sends ETag (W/"sha1") and answers If-None-Match with 304 and no body; FAKE_NO_ETAG=1 = an older server (no ETag, never 304). The log shows "inm" on conditional requests and {"type":"net","what":"304"}.
# Text "file" in a chat message -> the server sends a file event (report.txt) that the client fetches over /file/.
# REALISTIC DATA (default on, FAKE_REALISTIC=0 = the old tiny answers): the WebSocket `info` is shaped like the real server's
# (server.js ~4154: active_model, efforts, devices, deviceInfo, models, modelIds, caps, labels, providers, running, partials,
# runStarts, beats, asks, newModels) with FAKE_NRUNNING (30) running chats whose live text `partials` are FAKE_PARTIAL_MIN_KB..
# FAKE_PARTIAL_MAX_KB (50..200) each = a few MB; every second WS connection sends it ASCII-escaped (\\ud83d\\ude00 pairs), the others
# as raw UTF-8. /api/m/chats lists FAKE_NCHATS (150) chats (emoji, umlauts, 300-char titles, null/empty titles, null parents) and
# /api/m/chats/<id> serves any of them with big answers (code, tables, CJK, emoji, control chars). FAKE_STREAM=1: after `info` the
# server streams tokens of the other running chats all the time (the client must skip them).
import asyncio, hashlib, json, os, random, sys, time
from urllib.parse import unquote, urlparse, parse_qs, quote
from http import HTTPStatus
from websockets.asyncio.server import serve
from websockets.http11 import Response
from websockets.datastructures import Headers

TOKEN = "fake.tok+en=1_x"
HTTPPORT = 0
# bad network on purpose, REST only (POST /fake/net changes it, or the FAKE_REST_* environment at start)
NET = {"delay_ms": int(os.environ.get("FAKE_REST_DELAY_MS", "0")), "drop": int(os.environ.get("FAKE_REST_DROP", "0")),
       "status": int(os.environ.get("FAKE_REST_STATUS", "0"))}
LOG = open(sys.argv[1], "a", buffering=1)
CONV = {}      # cid -> list of {role, text}
seq = 0
CONV["c1"] = [{"role": "user", "text": "hi", "at": "2026-10-17T18:00:00Z"}, {"role": "assistant", "text": "hello\n>STATUS: status=done", "at": "2026-10-17T18:00:05Z"}]
if os.environ.get("FAKE_SYS") == "1": CONV["c1"].insert(1, {"role": "sys", "text": "Worker a64-layout3 gestartet - 3 Aufgaben offen (Umlaute: äöü)", "at": "2026-10-17T18:00:02Z"})

CONV["c2"] = [{"role": "user", "text": "second question", "at": "2026-10-17T19:00:00Z"}, {"role": "assistant", "text": "second answer\n>STATUS: status=done", "at": "2026-10-17T19:00:05Z"}]
if os.environ.get("FAKE_TOOLS") == "1":  # c1 history with tool steps (ok, failed, long output, no output stored)
    _long = "\n".join("line %d of a long output" % k for k in range(1, 31))
    CONV["c1"][1:1] = [
        {"role": "assistant", "text": "Ich schaue kurz nach.", "at": 1760000000400},
        {"role": "tool", "text": "mcp__jarvis__run_parallel", "input": {"commands": ["ls /root", "df -h"]}, "result": {"ok": True, "text": _long, "images": []}, "at": 1760000000450},
        {"role": "tool", "text": "mcp__jarvis__read_file", "input": {"path": "/root/jarvis/server.js"}, "result": {"ok": True, "text": "import express from 'express'\nconst app = express()\napp.listen(3000)", "images": []}, "at": 1760000000500},
        {"role": "assistant", "text": "Die Datei ist da, ich pruefe noch den Rest.", "at": 1760000000550},
        {"role": "tool", "text": "mcp__jarvis__run_shell", "input": {"command": "cat /nonexistent/file"}, "result": {"ok": False, "text": "cat: /nonexistent/file: No such file or directory", "images": []}, "at": 1760000000600},
                {"role": "tool", "text": "mcp__jarvis__list_devices", "input": {}, "at": 1760000000800},
    ]
    CONV["s1"] = CONV["c1"]  # the first chat of the FAKE_SIDEBAR list
    CONV["s2"] = CONV["c1"]


if os.environ.get("FAKE_LONG") == "1":  # a66c: c1 = a long worker chat (bubbles, bullet lists, code) that scrolls: the reader must follow the end
    _msgs = []
    for k in range(1, 9):
        _msgs.append({"role": "user", "text": "question %d: why is the command centre on the phone so slow and what does the manager do about it" % k, "at": 1760000000000 + k * 10})
        _msgs.append({"role": "assistant", "text": "Answer %d, short version:\n\n- **Cause 1:** the kernel spins in a loop (`sched.tget`) with 99 %% host CPU.\n- **Cause 2, open:** the popup manager builds it like the web style, which takes a while.\n- **Your VM:** runs with the new image on 1 core.\n\nNothing needed from you.\n>STATUS: status=done" % k, "at": 1760000000000 + k * 10 + 5})
    CONV["c1"] = _msgs

if os.environ.get("FAKE_CHATJSON"):  # a real server answer ({"conversation": {"messages": [...]}}): the history of c1/s1/s2 (real tool steps, thinking, cut inputs)
    with open(os.environ["FAKE_CHATJSON"]) as _f:
        _real = json.load(_f)["conversation"]["messages"]
    CONV["c1"] = CONV["s1"] = CONV["s2"] = _real

# ------------------------------------------------------------------ realistic data (shapes of the real server)
REAL = os.environ.get("FAKE_REALISTIC", "1") != "0"
NCHATS = int(os.environ.get("FAKE_NCHATS", "150"))
NRUNNING = int(os.environ.get("FAKE_NRUNNING", "30"))
PMIN = int(os.environ.get("FAKE_PARTIAL_MIN_KB", "50")) * 1024
PMAX = int(os.environ.get("FAKE_PARTIAL_MAX_KB", "200")) * 1024
STREAM = os.environ.get("FAKE_STREAM", "0") == "1"
MODELS = ["opus", "sonnet", "haiku", "fable", "hy3", "glm"]
# GET/POST /api/m/models: the models the "admin" enabled, the stored choice (docs/ANDROID-APP-API.md)
ENABLED = ["sonnet", "haiku", "opus"]
EFFORTS = ["low", "medium", "high", "xhigh", "max"]
MODEL_LABELS = {"sonnet": "Claude Sonnet 5.5", "haiku": "Claude Haiku 5", "opus": "Claude Opus 5.5"}
MODEL_STATE = {"model": "sonnet", "effort": "medium", "thinking": False}
THINK_MODELS = ["sonnet", "opus"]  # haiku cannot think (the app greys the switch out)
THINK_LOG = []  # every /thinking command and POST thinking value, newest last (tests read it)


def model_current():
    return {"model": MODEL_STATE["model"], "effort": MODEL_STATE["effort"], "thinking": MODEL_STATE["thinking"], "label": MODEL_LABELS[MODEL_STATE["model"]], "provider": "claude"}
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
    if os.environ.get("FAKE_STABLE_CHATS") == "1": now = now // 3600000 * 3600000  # a61: same list body across requests / restarts (ETag tests)
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

ADMIN_LIMITS = {"maxSubworkers": 5, "heavySlots": 8, "autoLoop": True}
# user settings (A50): GET/POST /api/settings (switches + persona)
SETTINGS = {"personality": "jarvis", "personas": [{"id": "p_demo", "name": "Bro", "base": "jarvis"}], "ttsRate": 1.05, "ttsVoice": "", "ttsSpeaker": "", "autoConfirm": True, "deepMode": True, "expMode": False, "learnMode": False, "planMode": False}
# folders (A48): FAKE_FOLDERS=1 starts with two folders and puts s1/s3/s5 in them; POST /api/m/folders/(add|rename|remove), /api/m/chats/move
FOLDERS = [{"id": "f_work", "name": "Work", "order": 0}, {"id": "f_priv", "name": "Privat", "order": 1}] if os.environ.get("FAKE_FOLDERS") == "1" else []
CHAT_FOLDER = {"s1": "f_work", "s3": "f_work", "s5": "f_priv"} if os.environ.get("FAKE_FOLDERS") == "1" else {}
CHAT_FAV = {}  # POST /api/m/chats/move {fav}: id -> bool
FOLDER_SEQ = [0]

RENAMED = {}   # POST /api/m/chats/rename: id -> new title (FAKE_NO_CHAT_OPS=1: an older server, 404)
REMOVED = set()  # POST /api/m/chats/remove

def all_chats():
    rows = all_chats_raw()
    return [dict(c, title=RENAMED.get(c["id"], c["title"]), folder=CHAT_FOLDER.get(c["id"], ""), fav=CHAT_FAV.get(c["id"], c.get("fav", False))) for c in rows if c["id"] not in REMOVED]

def all_chats_raw():
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
        if i == 7 and os.environ.get("FAKE_NOMD") != "1": m["text"] += "\n\n![md-hist](/img/u1/%s/md.png)\n\nand an absolute one: ![md-abs](http://127.0.0.1:%d/img/u1/%s/abs.png)\n" % (cid, HTTPPORT, cid)
        if i == 3 and os.environ.get("FAKE_NOIMG") != "1": m["images"] = [{"url": "/img/u1/%s/pic.png" % cid}]
        if i == 5: m["files"] = [{"name": "report.txt", "mime": "text/plain", "size": 2048, "url": "/file/u1/%s/report.txt" % cid}]
        msgs.append(m)
    if os.environ.get("FAKE_TOOLS") == "1" and len(msgs) >= 2:
        # tool steps before the last answer: ok, failed, long output (the app's collapsible step groups)
        long_out = "\n".join("line %d of a long output" % k for k in range(1, 31))
        steps = [
            {"role": "tool", "text": "mcp__jarvis__read_file", "input": {"path": "/root/jarvis/server.js"},
             "result": {"ok": True, "text": "import express from 'express'\nconst app = express()\napp.listen(3000)", "images": []}, "at": 1760000000500},
            {"role": "tool", "text": "mcp__jarvis__run_shell", "input": {"command": "cat /nonexistent/file"},
             "result": {"ok": False, "text": "cat: /nonexistent/file: No such file or directory", "images": []}, "at": 1760000000600},
            {"role": "tool", "text": "mcp__jarvis__run_parallel", "input": {"commands": ["ls /root", "df -h"]},
             "result": {"ok": True, "text": long_out, "images": []}, "at": 1760000000700},
            {"role": "tool", "text": "mcp__jarvis__list_devices", "input": {}, "at": 1760000000800},
        ]
        msgs[-1:-1] = steps
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

def a64_fixture():
    # same text as tmp/a64/layout/fixture.md (the web render is made from it)
    md = open(os.environ.get("FAKE_A64_MD", os.path.join(os.path.dirname(os.path.abspath(__file__)), "a64_fixture.md")), encoding="utf-8").read()
    now = int(time.time() * 1000)
    return [
        {"role": "user", "text": "Zeig mir bitte den Status und pr\u00fcfe `check` noch einmal.", "at": now - 60000},
        {"role": "assistant", "text": md, "genMs": 17000, "at": now - 20000},
        {"role": "sys", "text": "\u2705 Worker fertig \u2014 \u201eGer\u00e4te-Status Check\u201c (Chat c_lfh_znA5mgLN): **N-WORT** \u2013 verbunden. Sonst keine Ger\u00e4te online, Boss.", "at": now - 10000},
    ]


async def process_request(conn, request):
    u = urlparse(request.path)
    if u.path.startswith("/api/conversations/"):
        if not cookie_ok(request.headers):
            return reply(401, {"error": "Not logged in"})
        cid = unquote(u.path[len("/api/conversations/"):])
        LOG.write(json.dumps({"type": "rest", "method": "GET", "path": u.path, "auth": True}) + "\n"); LOG.flush()
        if "/input/" in cid:  # a64: the full input of a step the history sent as {"__cut": n} (server lib/conversations.js inputAt)
            _n = cid.rsplit("/input/", 1)[1]
            return reply(200, {"input": {"command": "echo full input %s" % _n}})
        if cid not in CONV and os.environ.get("FAKE_A64") == "1":  # a64: the layout fixture of the web comparison (user bubble, markdown answer with meta line, worker note)
            CONV[cid] = a64_fixture()
        if cid not in CONV and os.environ.get("FAKE_HIST") == "1":
            CONV[cid] = gen_messages(cid)
            for _m in CONV[cid]: _m["text"] = _m["text"].replace("\x02", "")  # jsteps.fi panics on a stray \x02 (open point), not the subject here
            if os.environ.get("FAKE_HISTSHORT") == "1":  # short history: one answer with a markdown picture, an own-host absolute one and images[]
                CONV[cid] = [{"role": "user", "text": "show pics", "at": 1760000000000},
                             {"role": "assistant", "text": "md:\n\n![md-hist](/img/u1/%s/md.png)\n\nabs:\n\n![md-abs](http://127.0.0.1:%d/img/u1/%s/abs.png)\n\ndl alias:\n\n![alias](/downloads/u1/x/dl.png)\n" % (cid, HTTPPORT, cid), "images": [{"url": "/img/u1/%s/pic.png" % cid}], "at": 1760000001000}]
            # a59: any chat id of the sidebar has a history (picture in message 3, markdown pictures in 7)
        if cid not in CONV and os.environ.get("FAKE_PAGING") == "1":
            # a64: 400 messages (200 turns) for pagination; the window rules of server lib/conversations.js full(): ?turns=n / ?before=<idx>&turns=n
            CONV[cid] = []
            for _i in range(200):
                CONV[cid].append({"role": "user", "text": "question %d" % _i, "at": 1760000000000 + _i * 60000})
                CONV[cid].append({"role": "assistant", "text": "answer %d: " % _i + "lorem ipsum dolor sit amet " * (3 + _i % 5), "at": 1760000030000 + _i * 60000, "genMs": 30000})
        if cid not in CONV:
            return reply(404, {"error": "Not found"})
        msgs = CONV[cid]
        q = dict(x.split("=", 1) for x in u.query.split("&") if "=" in x)
        if os.environ.get("FAKE_PAGING") == "1" and ("turns" in q or "before" in q):
            end = min(int(q["before"]), len(msgs)) if "before" in q else len(msgs)
            n = int(q.get("turns", "5")); start = end; c = 0
            while start > 0 and c < n:
                start -= 1
                if msgs[start]["role"] == "user": c += 1
            LOG.write(json.dumps({"type": "page", "before": q.get("before"), "turns": n, "from": start, "to": end}) + "\n"); LOG.flush()
            return reply(200, {"conversation": {"messages": msgs[start:end] if "before" in q else msgs[start:], "from": start, "total": len(msgs)}, "running": False, "partial": ""})
        return reply(200, {"conversation": {"messages": msgs}, "running": False, "partial": ""})
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
TTSN = {"n": 0}
tseq = 1

def pstats(p):
    tot = len(p["roadmap"]); dn = len([r for r in p["roadmap"] if r["done"]])
    return {"total": tot, "done": dn, "open": tot - dn, "pct": round(dn / tot * 100) if tot else 0,
            "needs": len([n for n in p["needs"] if not n["done"]])}

def psummary(p):
    return {k: p[k] for k in ("id", "name", "status", "goal", "next", "updatedAt")} | pstats(p)

def pfull(p):
    return psummary(p) | {"roadmap": p["roadmap"], "needs": p["needs"], "chats": p["chats"]}

PW = {"cur": os.environ.get("FAKE_PW", "secret1234")}
MEM = {"k": [], "t": [], "p": {"Name": "Justin", "Wohnort": "Innsbruck"}, "seq": 0}
def mem_seed():
    for i in range(1, 61):
        MEM["k"].append({"id": "mk%03d" % i, "text": ("Fakt %d: Firn Beat Kotlin Messung, Umlaute aeoeue \u00e4\u00f6\u00fc, langer Text " % i) + "x" * (i * 9 % 500), "tag": ["firn", "osum", "certus", ""][i % 4], "at": "2026-10-%02dT10:00:00.000Z" % (1 + i % 28)})
    MEM["t"].append({"id": "mt001", "text": "Aufgabe: Zahnarzt anrufen", "tag": "", "at": "2026-10-19T08:00:00.000Z", "done": False})
mem_seed()
def mem_items():
    out = [dict(x, type="knowledge") for x in MEM["k"]] + [dict(x, type="task") for x in MEM["t"]]
    out += [{"id": "p:" + k, "type": "profile", "tag": "", "text": k + ": " + v, "at": ""} for k, v in MEM["p"].items()]
    return out
def mem_pub(e, full):
    lim = 4000 if full else 400
    o = {"id": e["id"], "type": e["type"], "tag": e.get("tag", ""), "text": e["text"][:lim], "chars": len(e["text"]), "at": e.get("at", "")}
    if len(e["text"]) > lim: o["trunc"] = True
    return o
def mem_route(method, q, b):
    def out(st, obj): return (st, json.dumps(obj).encode(), "application/json")
    items = mem_items()
    if method == "GET":
        if q.get("id"):
            e = [x for x in items if x["id"] == q["id"][0]]
            return out(200, {"item": mem_pub(e[0], True)}) if e else out(404, {"error": "Nicht gefunden"})
        qq = (q.get("q") or [""])[0].lower().strip(); ty = (q.get("type") or [""])[0]
        lim = int((q.get("limit") or ["40"])[0]); off = int((q.get("offset") or ["0"])[0])
        hits = [x for x in items if (not ty or x["type"] == ty) and (not qq or qq in x["text"].lower() or qq in x.get("tag", ""))]
        hits.sort(key=lambda x: x.get("at", ""), reverse=True)
        tags = {}
        for x in MEM["k"]:
            if x.get("tag"): tags[x["tag"]] = tags.get(x["tag"], 0) + 1
        return out(200, {"total": len(hits), "all": len(items), "items": [mem_pub(x, False) for x in hits[off:off + lim]], "tags": [{"name": k, "n": v} for k, v in tags.items()]})
    act = b.get("action"); text = str(b.get("text", "")).strip(); i = str(b.get("id", ""))
    if act == "add":
        if not text: return out(400, {"error": "Text fehlt"})
        MEM["seq"] += 1; ty = b.get("type")
        if ty == "profile" and ":" in text:
            k, _, v = text.partition(":"); MEM["p"][k.strip()] = v.strip()
        else:
            MEM["t" if ty == "task" else "k"].append({"id": "mn%03d" % MEM["seq"], "text": text, "tag": str(b.get("tag", "")).lower(), "at": "2026-10-20T09:00:00.000Z"})
        return out(200, {"ok": True})
    if act == "edit":
        if not text: return out(400, {"error": "Text fehlt"})
        if i.startswith("p:"):
            MEM["p"].pop(i[2:], None); k, _, v = text.partition(":"); MEM["p"][k.strip() if v else i[2:]] = (v or text).strip(); return out(200, {"ok": True})
        for lst in (MEM["k"], MEM["t"]):
            for e in lst:
                if e["id"] == i:
                    e["text"] = text; e["tag"] = str(b.get("tag", e.get("tag", ""))).lower(); e["at"] = "2026-10-20T09:30:00.000Z"; return out(200, {"ok": True})
        return out(404, {"error": "Nicht gefunden"})
    if act == "delete":
        if i.startswith("p:"):
            return out(200, {"ok": True}) if MEM["p"].pop(i[2:], None) is not None else out(404, {"error": "Nicht gefunden"})
        for lst in (MEM["k"], MEM["t"]):
            for e in lst:
                if e["id"] == i: lst.remove(e); return out(200, {"ok": True})
        return out(404, {"error": "Nicht gefunden"})
    return out(400, {"error": "Unbekannte Aktion"})

ADM = {"users": [{"id": "u1", "username": "justin", "email": "justin@fleitec.com", "admin": True, "online": True, "hasPush": True},
                 {"id": "u2", "username": "papa", "email": "info@fleitec.com", "admin": False, "online": False, "hasPush": True, "rights": {"projects": {"firn": "write"}}},
                 {"id": "u3", "username": "gast", "email": "", "admin": False, "online": False, "hasPush": False}],
       "inv": [{"code": "ABCD-1234", "note": "Volker", "createdBy": "justin", "createdAt": 1, "expiresAt": None, "usedBy": None, "usedByName": None, "revoked": False, "status": "offen"},
               {"code": "WXYZ-9876", "note": "", "createdBy": "justin", "createdAt": 1, "expiresAt": None, "usedBy": "u3", "usedByName": "gast", "revoked": False, "status": "benutzt"}],
       "dauer": False, "borrow": {},
       "acc": [{"id": "a1", "label": "Konto 1", "mode": "login", "active": True, "disabled": False, "free": 60, "runs": 14, "rate": {"fiveHour": {"util": 30}, "sevenDay": {"util": 40}}},
               {"id": "a2", "label": "Token 2", "mode": "token", "active": False, "disabled": True, "free": 100, "runs": 2, "rate": {}}],
       "models": {"enabled": ["opus", "sonnet"], "defaultModel": "sonnet", "defaultEffort": "high"},
       "prov": [{"provider": "openai", "label": "OpenAI", "set": True, "source": "config.json", "obscured": "sk-abc\u2026wxyz"}, {"provider": "openrouter", "label": "OpenRouter", "set": False, "source": "kein", "obscured": ""}],
       "pve": [{"id": "k_1", "label": "Laptop", "active": True, "fp": "SHA256:abcdef", "type": "ssh-ed25519"}],
       "prompt": "Verwende so wenig Worte wie moeglich.\nAntworte immer uebersichtlich."}

def rest(method, rawpath, hdr, body):
    """-> (status, bytes, ctype) or None (not a REST route of the app client)."""
    global tseq
    u = urlparse(rawpath)
    path = u.path
    q = parse_qs(u.query)
    if not (path in ("/api/me", "/api/projects", "/api/command-center", "/api/conversations", "/api/tasks",
                     "/api/remote/devices", "/api/settings", "/api/voices", "/tts", "/api/push-token", "/api/stt", "/api/m/upload", "/api/account/password", "/api/accounts", "/api/usage-all", "/api/usage-insights", "/api/models/new")
            or path.startswith("/api/admin/") or path.startswith("/api/projects/") or path.startswith("/api/m/") or path.startswith("/file/")
            or path.startswith("/vendor/fonts/") or path.startswith("/img/") or path.startswith("/dl/")):
        return None
    jtype = hdr.get("content-type")
    entry = {"type": "rest", "method": method, "path": rawpath, "ctype": jtype, "n": len(body)}
    if hdr.get("if-none-match"): entry["inm"] = hdr["if-none-match"]  # a61: the request log shows the conditional GET
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
    if path.startswith("/vendor/fonts/"):  # public static files (no login), like the real server
        fp = os.path.join(os.environ.get("FAKE_FONT_DIR", "/root/jarvis/public/vendor/fonts"), os.path.basename(path))
        if os.environ.get("FAKE_NO_FONTS") or not os.path.isfile(fp):
            return out(404, {"error": "Not found"})
        with open(fp, "rb") as fh:
            return (200, fh.read(), "font/ttf")
    if path == "/api/m/ping":
        return out(200, {"ok": True, "api": 1, "time": 1760726400000, "push": {"tcp": True, "host": "127.0.0.1", "port": 7781, "poll": True}})
    if not ok:
        return out(401, {"error": "Not logged in"})
    if path.startswith("/api/m/") and path != "/api/m/upload":
        mp = path[len("/api/m/"):]
        if mp == "theme" and method == "GET":  # FAKE_FONT = system|inter|grotesk|mono (FAKE_NO_THEME: an older server)
            if os.environ.get("FAKE_NO_THEME"): return out(404, {"error": "Not found"})
            ff = os.environ.get("FAKE_FONT", "system")
            files = {"inter": {"regular": "/vendor/fonts/Inter-Regular.ttf", "bold": "/vendor/fonts/Inter-Bold.ttf"},
                     "grotesk": {"regular": "/vendor/fonts/SpaceGrotesk-Regular.ttf", "bold": "/vendor/fonts/SpaceGrotesk-Bold.ttf"}}.get(ff)
            return out(200, {"font": ff, "fontCss": "system-ui", "scale": 100, "radius": 12, "density": "kompakt", "preset": os.environ.get("FAKE_PRESET", "fleitec"), "accent": "#3b82f6", "motion": True, "lang": "auto", "fontFiles": files})
        if mp == "usage" and method == "GET":
            if os.environ.get("FAKE_NO_USAGE"): return out(503, {"error": "no usage source"})
            return out(200, {"provider": "claude", "name": "Claude", "p5": 37, "p7": 64, "reset5": None, "reset7": None, "providers": {}})
        if mp.startswith("models") and method == "GET":  # FAKE_NO_USAGE: an older server; FAKE_NO_MODELS=1: 503 only for the list
            if os.environ.get("FAKE_NO_USAGE") or os.environ.get("FAKE_NO_MODELS"): return out(503, {"error": "no models"})
            return out(200, {"models": [{"alias": a, "label": MODEL_LABELS[a], "blurb": "", "provider": "claude", "effort": a != "haiku", "thinking": a in THINK_MODELS, "vision": True} for a in ENABLED],
                             "efforts": EFFORTS, "current": model_current(), "chat": None})
        # /api/m/me + /api/m/admin/*: FAKE_ROLE=user -> admin false and every admin route 403 (like the real server); FAKE_NO_ADMIN=1: older server (404)
        if mp == "me" and method == "GET":
            if os.environ.get("FAKE_NO_ADMIN"): return out(404, {"error": "Not found"})
            return out(200, {"uid": "u1", "username": "justin", "email": "justin@fleitec.com", "admin": os.environ.get("FAKE_ROLE", "admin") != "user", "impersonating": False})
        if mp == "admin" or mp.startswith("admin/"):
            if os.environ.get("FAKE_NO_ADMIN"): return out(404, {"error": "Not found"})
            if os.environ.get("FAKE_ROLE", "admin") == "user": return out(403, {"error": "Nur f\u00fcr Admins."})
            if mp == "admin/users" and method == "GET":
                return out(200, {"total": 3, "users": [{"id": "u1", "username": "justin", "email": "justin@fleitec.com", "admin": True, "createdAt": 1, "projects": 6},
                                                       {"id": "u2", "username": "papa", "email": "info@fleitec.com", "admin": False, "createdAt": 2, "projects": 1},
                                                       {"id": "u3", "username": "gast", "email": "", "admin": False, "createdAt": 3, "projects": 0}]})
            if mp == "admin/limits" and method == "GET": return out(200, dict(ok=True, **ADMIN_LIMITS))
            if mp == "admin/limits" and method == "POST" and j:
                for k in ("maxSubworkers", "heavySlots", "autoLoop"):
                    if k in j: ADMIN_LIMITS[k] = j[k]
                return out(200, dict(ok=True, **ADMIN_LIMITS))
            if mp == "admin/server" and method == "GET":
                return out(200, {"host": "jarvis", "cpu": {"cores": 16, "load1": 2.5, "load5": 2.1, "load15": 1.9, "loadPct": 16}, "mem": {"totalBytes": 68719476736, "freeBytes": 21474836480, "usedPct": 69},
                                 "disk": {"totalBytes": 1099511627776, "freeBytes": 214748364800, "usedPct": 80}, "uptimeSec": 1234567, "processUptimeSec": 4000, "node": "v18.20.8"})
            if mp == "admin/logs" and method == "GET":
                return out(200, {"file": "2026-10-19.log", "files": ["2026-10-19.log"], "size": 100, "lines": ["[2026-10-19T20:00:00Z] server start", "[2026-10-19T20:00:01Z] login justin", "[2026-10-19T20:00:05Z] worker a48 started"]})
            return out(404, {"error": "Unbekannter Pfad"})
        if mp == "models" and method == "POST" and j:
            if j.get("model") is not None:
                if j["model"] not in ENABLED: return out(400, {"error": "Modell nicht verfuegbar"})
                MODEL_STATE["model"] = j["model"]
            if j.get("effort") is not None:
                if j["effort"] not in EFFORTS: return out(400, {"error": "Effort unbekannt"})
                MODEL_STATE["effort"] = j["effort"]
            if isinstance(j.get("thinking"), bool):
                MODEL_STATE["thinking"] = j["thinking"]; THINK_LOG.append("post:" + str(j["thinking"]))
            return out(200, {"ok": True, "current": model_current()})
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
            if os.environ.get("FAKE_SLOW_CHATS"): time.sleep(int(os.environ["FAKE_SLOW_CHATS"]) / 1000.0)  # a53: slow chat list (ms, blocks the whole fake server); FAKE_FAIL_CHATS=1 answers 502
            if os.environ.get("FAKE_FAIL_CHATS"): return out(502, {"error": "bad gateway"})
            allc = all_chats()
            if os.environ.get("FAKE_NO_FOLDERS"):  # an older server: no folders / folder / fav keys in the chat list
                allc = [{k: v for k, v in c.items() if k not in ("folder", "fav")} for c in allc]
            fkey = {} if os.environ.get("FAKE_NO_FOLDERS") else {"folders": FOLDERS}
            if not REAL:
                return out(200, dict({"total": len(allc), "chats": allc}, **fkey))
            lim = max(1, min(100, int(q.get("limit", ["30"])[0] or 30))); off = max(0, int(q.get("offset", ["0"])[0] or 0))
            qq = (q.get("q", [""])[0] or "").lower()
            if qq: allc = [c for c in allc if qq in str(c["title"] or "").lower()]
            return out(200, dict({"total": len(allc), "chats": allc[off:off + lim]}, **fkey))
        if mp.startswith("chats/") and method == "GET":
            cid = unquote(mp[len("chats/"):])
            row = next((c for c in all_chats() if c["id"] == cid), None)
            if cid not in CONV and not (REAL and row):
                return out(404, {"error": "Not found"})
            if os.environ.get("FAKE_GROW") == "1" and cid == "c1" and "before" not in q:  # a66c: the worker grows on every poll
                CONV[cid].append({"role": "assistant", "text": "Grown message %d: one more line of text that wraps over two lines of the phone screen.\n>STATUS: status=active" % len(CONV[cid]), "at": 1760000100000 + len(CONV[cid])})
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
        if os.environ.get("FAKE_NO_FOLDERS") and (mp.startswith("folders") or mp == "chats/move"):  # FAKE_NO_FOLDERS=1: older server, 404
            return out(404, {"error": "Not found"})
        if mp == "folders" and method == "GET":
            return out(200, {"folders": FOLDERS})
        if mp in ("folders/add", "folders/rename", "folders/remove") and method == "POST":
            if os.environ.get("FAKE_NO_CHAT_OPS"): return out(404, {"error": "Not found"})
            j = j or {}
            if mp == "folders/add":
                nm = (j.get("name") or "").strip()[:40]
                if not nm or len(FOLDERS) >= 40: return out(400, {"error": "Name missing or too many folders"})
                FOLDER_SEQ[0] += 1
                f = {"id": "f_new%d" % FOLDER_SEQ[0], "name": nm, "order": len(FOLDERS)}
                FOLDERS.append(f)
                return out(200, {"folder": f, "folders": FOLDERS})
            f = next((x for x in FOLDERS if x["id"] == j.get("id")), None)
            if not f: return out(404, {"error": "Folder not found"})
            if mp == "folders/rename":
                nm = (j.get("name") or "").strip()[:40]
                if not nm: return out(404, {"error": "Name empty"})
                f["name"] = nm
                return out(200, {"folder": f, "folders": FOLDERS})
            FOLDERS.remove(f)
            for k in [k for k, v in CHAT_FOLDER.items() if v == f["id"]]: del CHAT_FOLDER[k]
            return out(200, {"ok": True, "folders": FOLDERS})
        if mp == "chats/move" and method == "POST":
            if os.environ.get("FAKE_NO_CHAT_OPS"): return out(404, {"error": "Not found"})
            j = j or {}
            row = next((c for c in all_chats() if c["id"] == j.get("id")), None)
            if not row: return out(404, {"error": "Not found"})
            if not isinstance(j.get("folder"), str) and not isinstance(j.get("fav"), bool): return out(400, {"error": "folder or fav missing"})
            if isinstance(j.get("folder"), str):
                if j["folder"] and any(x["id"] == j["folder"] for x in FOLDERS): CHAT_FOLDER[row["id"]] = j["folder"]
                else: CHAT_FOLDER.pop(row["id"], None)
            if isinstance(j.get("fav"), bool): CHAT_FAV[row["id"]] = j["fav"]
            return out(200, {"ok": True, "id": row["id"]})
        if mp in ("chats/rename", "chats/remove") and method == "POST":
            if os.environ.get("FAKE_NO_CHAT_OPS"): return out(404, {"error": "Not found"})
            row = next((c for c in all_chats() if c["id"] == (j or {}).get("id")), None)
            if not row: return out(404, {"error": "Not found"})
            if mp == "chats/rename":
                if not (j.get("title") or "").strip(): return out(400, {"error": "id/title missing"})
                RENAMED[row["id"]] = j["title"][:120]
                return out(200, {"ok": True, "id": row["id"], "title": RENAMED[row["id"]]})
            if row.get("running"): return out(409, {"error": "Chat is running"})
            REMOVED.add(row["id"])
            return out(200, {"ok": True, "id": row["id"]})
        if mp == "push/register" and method == "POST":
            return out(200, {"pushToken": "00112233445566778899aabbccddeeff", "relay": {"host": "127.0.0.1", "port": 7781, "tls": False, "pin": None, "hello": True, "sni": None}, "poll": "/api/m/push/poll"})
        if mp == "memory":  # a65: the user's memory (FAKE_NO_MEMORY=1: older server, 404)
            if os.environ.get("FAKE_NO_MEMORY"): return out(404, {"error": "Not found"})
            return mem_route(method, q, j or {})
        if mp == "push/test" and method == "POST":
            return out(200, {"ok": True, "sent": 1})
        return out(404, {"error": "Not found"})
    if path == "/api/account/password" and method == "POST":  # a65: FAKE_PW = current password, FAKE_PW_NOTOKEN=1 = server without the token in the answer, FAKE_PW_LOCK=1 = 429
        b = j or {}
        if os.environ.get("FAKE_PW_LOCK"): return out(429, {"error": "Zu viele Fehlversuche. Bitte in 30s erneut versuchen."})
        if len(str(b.get("password", ""))) < 8: return out(400, {"error": "Passwort zu kurz (min. 8)"})
        if str(b.get("old", "")) == str(b.get("password", "")): return out(400, {"error": "Das neue Passwort ist das alte."})
        if str(b.get("old", "")) != PW["cur"]: return out(403, {"error": "Aktuelles Passwort stimmt nicht."})
        PW["cur"] = str(b.get("password"))
        return out(200, {"ok": True} if os.environ.get("FAKE_PW_NOTOKEN") else {"ok": True, "token": TOKEN})
    if path == "/tts":  # a66: the Piper proxy of the real server: a WAV (22.05 kHz mono 16 bit); a sine tone whose length follows the text and the speed
        # FAKE_TTS=off -> 503 (tts offline), FAKE_TTS=empty -> 204, FAKE_TTS=fail2 -> the SECOND request fails (503); the request (text, speed, voice) is in the log
        import math, struct
        text = q.get("text", [""])[0]
        speed = float(q.get("speed", ["1"])[0] or 1)
        TTSN["n"] += 1
        LOG.write(json.dumps({"type": "tts", "n": TTSN["n"], "text": text, "speed": speed, "voice": q.get("voice", [""])[0], "speaker": q.get("speaker", [""])[0]}) + "\n")
        mode = os.environ.get("FAKE_TTS", "")
        if mode == "off" or (mode == "fail2" and TTSN["n"] == 2): return (503, b"tts offline", "text/plain")
        if mode == "empty" or not text.strip(): return (204, b"", "text/plain")
        rate = 22050
        secs = min(3.0, max(0.3, len(text) * float(os.environ.get("FAKE_TTS_SEC_PER_CHAR", "0.04")) / max(0.5, speed)))
        n = int(rate * secs)
        pcm = b"".join(struct.pack("<h", int(6000 * math.sin(2 * math.pi * 440 * i / rate))) for i in range(n))
        hdr = b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16) + b"data" + struct.pack("<I", len(pcm))
        return (200, hdr + pcm, "audio/wav")
    if path == "/api/usage-all":  # a67: provider rates of the Usage tab
        return out(200, {"rates": {"claude": {"fiveHour": {"util": 42}, "sevenDay": {"util": 17}, "source": "claude", "label": "Claude", "subscription": "max"},
                                   "moonshot": {"fiveHour": None, "sevenDay": {"util": 8}, "source": "moonshot", "label": "Kimi", "subscription": None}}})
    if path == "/api/usage-insights":
        wk = q.get("win", ["day"])[0] == "week"
        return out(200, {"rate": None, "user": "justin", "totals": {"turns": 310 if wk else 44, "in": 1840000 if wk else 210000, "out": 920000 if wk else 98000, "cr": 41000000 if wk else 5200000, "cw": 1},
                         "topChats": [{"cid": "c1", "title": "Firn: Admin Tabs", "turns": 80, "kind": "chat", "in": 500000, "out": 1, "cr": 9000000, "cw": 0, "maxCtx": 1, "avgCr": 1, "pct": 34},
                                      {"cid": "c2", "title": "OrientOS Worker mit einem sehr langen Titel der umbrechen muss", "turns": 41, "kind": "worker", "in": 300000, "out": 1, "cr": 4000000, "cw": 0, "maxCtx": 1, "avgCr": 1, "pct": 18}] if wk else []})
    if path == "/api/models/new":  # a67: model guard
        if method == "POST": ADM["guard_check"] = "2026-10-20T09:55:12.000Z"
        return out(200, {"pending": [{"kind": "upgrade", "alias": "sonnet", "id": "claude-sonnet-5-6", "prevId": "claude-sonnet-5-5", "label": "Sonnet 5.6", "family": "sonnet", "at": "x"},
                                     {"kind": "new", "alias": "haiku", "id": "claude-haiku-5", "prevId": None, "label": "Haiku 5", "family": "haiku", "at": "x"}],
                         "aliases": ["sonnet", "haiku"], "lastCheck": ADM.get("guard_check", "2026-10-20T07:12:33.123Z"), "ok": True})
    if path.startswith("/api/admin/") or path == "/api/accounts":  # a66: the web admin routes the app's Admin tabs use (state in ADM)
        if os.environ.get("FAKE_ROLE", "admin") == "user": return out(403, {"error": "Nur f\u00fcr Admins."})
        b = j if isinstance(j, dict) else {}
        if path == "/api/admin/mcp" and method == "GET":  # a67
            return out(200, {"servers": [{"name": "context7", "transport": "stdio", "summary": "npx -y @upstash/context7-mcp", "enabled": True, "admin_only": False, "accounts": [], "auto_allow": ["*"], "note": ""},
                                         {"name": "logiclab", "transport": "stdio", "summary": "logiclab-mcp --stdio", "enabled": False, "admin_only": True, "accounts": [], "auto_allow": [], "note": "circuits"}],
                             "invalid": [{"name": "kaputt", "error": "command fehlt"}]})
        if path == "/api/admin/mcp/test" and method == "POST":
            if b.get("name") == "logiclab": return out(200, {"ok": False, "error": "Zeitueberschreitung (10 s) - keine initialize-Antwort."})
            return out(200, {"ok": True, "status": 200, "note": "Handshake ok, 3 Tools"})
        if path == "/api/admin/mcp/remove" and method == "POST": return out(200, {"ok": True, "servers": [], "invalid": []})
        if path == "/api/admin/openrouter-models" and method == "GET":
            return out(200, {"models": [{"id": "openai/gpt-5", "name": "OpenAI: GPT-5", "ctx": 400000, "reasoning": True, "effort": True, "vision": True, "prompt": 0.00000125, "completion": 0.00001},
                                        {"id": "anthropic/claude-sonnet-5", "name": "Anthropic: Claude Sonnet 5", "ctx": 1000000, "reasoning": True, "effort": False, "vision": True, "prompt": 0.000003, "completion": 0.000015},
                                        {"id": "meta/llama-free", "name": "Llama (free)", "ctx": 128000, "reasoning": False, "effort": False, "vision": False, "prompt": 0, "completion": 0}]})
        if path == "/api/admin/impersonate" and method == "POST":  # FAKE_IMP_NOTOKEN=1 = server without the app token in the answer
            ADM["imp"] = b.get("uid")
            return out(200, {"ok": True, "username": "papa"} if os.environ.get("FAKE_IMP_NOTOKEN") else {"ok": True, "username": "papa", "token": TOKEN})
        if method == "POST":
            if path == "/api/admin/users":
                for u_ in ADM["users"]:
                    if u_["id"] == b.get("uid") and "admin" in b and "action" not in b: u_["admin"] = bool(b["admin"])
                if b.get("action") == "create": ADM["users"].append({"id": "u%d" % (len(ADM["users"]) + 1), "username": b.get("username"), "email": "", "admin": False, "online": False, "hasPush": False})
                if b.get("action") == "remove": ADM["users"] = [u_ for u_ in ADM["users"] if u_["id"] != b.get("uid")]
            elif path == "/api/admin/invites":
                a_ = b.get("action")
                if a_ == "create":
                    ADM["inv"].insert(0, {"code": "NEW%d" % len(ADM["inv"]), "note": b.get("note", ""), "createdBy": "justin", "createdAt": 1, "expiresAt": None, "usedBy": None, "usedByName": None, "revoked": False, "status": "offen"})
                    return out(200, {"ok": True, "invite": ADM["inv"][0]})
                for i_ in ADM["inv"]:
                    if i_["code"] == b.get("code") and a_ == "revoke": i_["revoked"] = True; i_["status"] = "zur\u00fcckgezogen"
                if a_ == "remove": ADM["inv"] = [i_ for i_ in ADM["inv"] if i_["code"] != b.get("code")]
                if a_ == "dauercode": ADM["dauer"] = b.get("an") is True
            elif path == "/api/accounts":
                for c_ in ADM["acc"]:
                    if c_["id"] == b.get("toggle"): c_["disabled"] = bool(b.get("disabled"))
                if b.get("remove"): ADM["acc"] = [c_ for c_ in ADM["acc"] if c_["id"] != b["remove"]]
                if b.get("token"): ADM["acc"].append({"id": "t%d" % len(ADM["acc"]), "label": b.get("label") or "token", "mode": "token", "active": False, "disabled": False, "free": 100, "runs": 0, "rate": {}})
            elif path == "/api/admin/devices":
                ADM["borrow"][b.get("uid", "") + ":" + b.get("name", "")] = bool(b.get("borrow"))
            elif path == "/api/admin/models":
                ADM["models"].update({k: b[k] for k in ("enabled", "defaultModel", "defaultEffort") if k in b})
            elif path == "/api/admin/providers":
                for p_ in ADM["prov"]:
                    if p_["provider"] == b.get("provider"):
                        p_["set"] = b.get("key") is not None; p_["obscured"] = "sk-xxx\u2026abcd" if p_["set"] else ""
            elif path == "/api/admin/pve-keys":
                a_ = b.get("action")
                if a_ == "add": ADM["pve"].append({"id": "k_%d" % len(ADM["pve"]), "label": b.get("label") or "new", "active": True, "fp": "SHA256:newkey", "type": "ssh-ed25519"})
                if a_ == "toggle":
                    for k_ in ADM["pve"]:
                        if k_["id"] == b.get("id"): k_["active"] = bool(b.get("active"))
                if a_ == "remove": ADM["pve"] = [k_ for k_ in ADM["pve"] if k_["id"] != b.get("id")]
            elif path == "/api/admin/system-prompt":
                ADM["prompt"] = b.get("prompt", "")
                return out(200, {"ok": True, "prompt": ADM["prompt"]})
            elif path == "/api/admin/rights":
                for u_ in ADM["users"]:
                    if u_["id"] == b.get("uid"):
                        pr_ = u_.setdefault("rights", {}).setdefault("projects", {})
                        if b.get("level"): pr_[b["project"]] = b["level"]
                        else: pr_.pop(b["project"], None)
            if os.environ.get("FAKE_ADM_FAIL"): return out(400, {"error": "fake failure"})
        if path == "/api/admin/users": return out(200, {"users": ADM["users"], "me": "u1"})
        if path == "/api/admin/invites": return out(200, {"invites": ADM["inv"], "dauercode": "DAUER-42", "dauercodeAktiv": ADM["dauer"]})
        if path == "/api/accounts": return out(200, {"accounts": ADM["acc"], "activeId": "a1"})
        if path == "/api/admin/devices":
            return out(200, {"devices": [{"uid": "u1", "username": "justin", "name": "Dell", "own": True, "borrowed": False},
                                          {"uid": "u2", "username": "papa", "name": "Laptop", "own": False, "borrowed": ADM["borrow"].get("u2:Laptop", False)}]})
        if path == "/api/admin/models":
            m_ = ADM["models"]
            return out(200, {"all": ["opus", "sonnet", "haiku", "gpt-5", "grok"], "enabled": m_["enabled"], "defaultModel": m_["defaultModel"], "custom": [], "defaultEffort": m_["defaultEffort"], "efforts": ["low", "medium", "high"]})
        if path == "/api/admin/providers": return out(200, {"providers": ADM["prov"]})
        if path == "/api/admin/pve-keys": return out(200, {"host": "pve", "port": 22, "user": "root", "keys": ADM["pve"]})
        if path == "/api/admin/system-prompt": return out(200, {"prompt": ADM["prompt"], "effective": "..."})
        if path == "/api/admin/rights":
            return out(200, {"projects": ["firn", "osum", "certus", "jarvis"], "levels": ["read", "write", "shell"],
                             "users": [{"id": x["id"], "username": x["username"], "admin": x["admin"], "rights": x.get("rights", {"projects": {}})} for x in ADM["users"]], "me": "u1"})
        return out(404, {"error": "Not found"})
    if path == "/api/voices":  # a65: the Piper voice list of the web settings
        return out(200, {"voices": [{"id": "de_thorsten", "label": "Thorsten", "gender": "m", "default": True}, {"id": "de_kerstin", "label": "Kerstin", "gender": "w"}]})
    if path == "/api/settings":
        if method in ("POST", "PATCH"):
            try:
                SETTINGS.update(json.loads(body or b"{}"))
            except Exception:
                return out(400, {"error": "bad json"})
        return out(200, {"settings": SETTINGS})
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
    if path == "/api/command-center" and os.environ.get("FAKE_CC") == "1":  # a64 fixture of the web command centre (rate, deviceInfo, knowledge, tasks with category)
        iso = lambda ago: time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - ago))
        return out(200, {"now": int(time.time() * 1000), "user": "justin",
            "workers": [{"id": "c1", "title": "worker one", "running": True, "updatedAt": 1760000000001, "last": "doing", "partial": "par"}],
            "orchestrator": {"id": "o1", "title": "Leitstand", "running": False, "updatedAt": 5}, "otherChatCount": 7,
            "devices": ["Dell", "Phone"],
            "deviceInfo": [{"name": "Dell", "type": "OrientOS", "online": True, "lastSeen": iso(30)},
                           {"name": "Xiaomi", "type": "Android", "online": False, "lastSeen": iso(7200)}],
            "tasks": [{"id": "t1", "text": "Pull the a64 branches and check the Xiaomi build", "due": iso(-3600), "category": "Projekt", "repeat": None, "overdue": False},
                      {"id": "t2", "text": "Buy milk and a very long second part that has to be cut at the card edge", "due": iso(3600), "category": None, "repeat": "weekly", "overdue": True},
                      {"id": "t3", "text": "Call Papa", "due": None, "category": "Privat", "repeat": None, "overdue": False}],
            "sys": {"cpus": 4, "load": [1.37, 0.9, 0.5], "uptime": 3 * 86400 + 5 * 3600, "memTotal": 16000000000, "memFree": 3000000000,
                    "disk": {"total": 500000000000, "free": 60000000000}, "host": "jarvis-srv"},
            "rate": {"fiveHour": {"util": 6}, "sevenDay": {"util": 85}},
            "memory": {"count": 3281, "knowledge": [
                {"at": iso(600), "text": "Firn Beat Kotlin F: main e396c4988 = a64 Crossing-Analyse (caller-saved x2-x8 are not kept over a call)"},
                {"at": iso(5400), "text": "OrientOS w-bench: Limine timeout 5 s"},
                {"at": iso(90000), "text": "Short note"}]},
            "activity": [{"cid": "c1", "conv": "worker one", "kind": "chat", "role": "assistant", "at": iso(120), "text": "Build finished, APK 0.2.46 checked on the phone"},
                         {"cid": "c1", "conv": "Leitstand", "kind": "chat", "role": "user", "at": iso(400), "text": "next step?"},
                         {"cid": "c2", "conv": "sys", "kind": "chat", "role": "system", "at": iso(900), "text": "worker started"}]})
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
    if path.startswith("/img/") or path.startswith("/dl/"):  # a PNG gradient (320x200), behind the login like the real /img/
        import zlib, struct
        w, h = 320, 200
        raw = b"".join(b"\x00" + b"".join(bytes((x * 255 // w, y * 255 // h, 160)) for x in range(w)) for y in range(h))
        def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
        png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
        return (200, png, "image/png")
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
            if text.startswith("/thinking"):  # the web switch sends "/thinking on|off"
                a = text[9:].strip()
                MODEL_STATE["thinking"] = a in ("on", "an", "1", "true") if a else not MODEL_STATE["thinking"]
                THINK_LOG.append("ws:" + text)
                print("THINK", text, flush=True)
                await send(ws, {"type": "system", "content": "ok " + text, "thinking": MODEL_STATE["thinking"]})
                continue
            if text.startswith("/model ") or text.startswith("/effort "):  # slash commands: stored like the web ones, no chat turn
                cmd, arg = text.split(" ", 1)
                if cmd == "/model" and arg.strip() in ENABLED: MODEL_STATE["model"] = arg.strip()
                if cmd == "/effort" and arg.strip() in EFFORTS: MODEL_STATE["effort"] = arg.strip()
                await send(ws, {"type": "system", "content": "ok " + text})
                continue
            if not cid:
                seq += 1
                cid = "fake%d" % seq
                CONV[cid] = []
                await send(ws, {"type": "conversation", "id": cid, "title": "t"})
            if m.get("editEnd"):  # a65: edit / repeat from a message: cut the n-th user message from the END and all after it (server lib/conversations.js cutAtUserFromEnd)
                cnt = 0
                for k in range(len(CONV[cid]) - 1, -1, -1):
                    if CONV[cid][k]["role"] == "user":
                        cnt += 1
                        if cnt == int(m["editEnd"]):
                            del CONV[cid][k:]
                            break
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
            elif text == "tools":  # live tool steps: ok, failed, long output
                await send(ws, {"type": "token", "cid": cid, "content": "Ich schaue kurz nach. "})
                await asyncio.sleep(0.2)
                await send(ws, {"type": "tool", "cid": cid, "name": "mcp__jarvis__run_shell", "id": "t1", "input": {"command": "ls -la /root"}})
                await asyncio.sleep(0.3)
                await send(ws, {"type": "tool_result", "id": "t1", "ok": True, "text": "total 4\ndrwx------ 2 root root 4096 Oct 19 .\n", "images": []})
                await send(ws, {"type": "token", "cid": cid, "content": "Zwischenstand: das erste Kommando lief."})
                await asyncio.sleep(0.2)
                await send(ws, {"type": "tool", "cid": cid, "name": "mcp__jarvis__read_file", "id": "t2", "input": {"path": "/etc/nothing"}})
                await asyncio.sleep(0.3)
                await send(ws, {"type": "tool_result", "id": "t2", "ok": False, "text": "ENOENT: no such file", "images": []})
                await send(ws, {"type": "tool", "cid": cid, "name": "mcp__jarvis__run_shell", "id": "t3", "input": {"command": "seq 1 25"}})
                await send(ws, {"type": "tool_result", "id": "t3", "ok": True, "text": "\n".join(str(k) for k in range(1, 26)), "images": []})
                await send(ws, {"type": "token", "cid": cid, "content": "Done: 3 steps.\n>STATUS: status=done"})
            elif text == "code":  # a fenced block + inline code: the monospace face (font slot 1) check
                await send(ws, {"type": "token", "cid": cid, "content": "Run `ls -la` here:\n\n```sh\nfor f in *.fi; do echo \"$f\"; done\n```\n"})
            elif text == "img":  # live answer with markdown pictures: relative, absolute own host, delivered (/dl)
                await send(ws, {"type": "token", "cid": cid, "content": "Pic rel:\n\n![rel](/img/u1/%s/rel.png)\n\n" % cid})
                await send(ws, {"type": "token", "cid": cid, "content": "Pic abs:\n\n![abs](http://127.0.0.1:%d/img/u1/%s/abs.png)\n\n" % (HTTPPORT, cid)})
                await send(ws, {"type": "token", "cid": cid, "content": "Pic dl:\n\n![dl](/dl/u1/x/dl.png)\n\nEnd.\n>STATUS: status=done"})
            elif text == "imgfile":  # a66: the assistant delivers an image file (type "file", mime image/png): shown inline + as a file card
                await send(ws, {"type": "token", "cid": cid, "content": "Here is the picture:"})
                await send(ws, {"type": "file", "cid": cid, "name": "chart.png", "url": "/dl/u1/%s/chart.png" % cid, "mime": "image/png"})
                await send(ws, {"type": "token", "cid": cid, "content": "\n\nDone.\n>STATUS: status=done"})
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
                await send(ws, {"type": "thinking", "cid": cid, "content": "I check the shell first.\nThen I answer."})
                if os.environ.get("FAKE_THINKLIVE") == "1": await asyncio.sleep(float(os.environ.get("FAKE_THINKLIVE_S", "6")))
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
            xh = ""
            # a61: GET /api/m/chats carries a weak ETag of the body and answers If-None-Match with 304 and no body (FAKE_NO_ETAG=1: an older server)
            if method == "GET" and path.split("?")[0] == "/api/m/chats" and st == 200 and not os.environ.get("FAKE_NO_ETAG"):
                et = 'W/"%s"' % hashlib.sha1(data).hexdigest()[:20]
                xh = "ETag: %s\r\nCache-Control: private, no-cache\r\n" % et
                if hdr.get("if-none-match") == et:
                    LOG.write(json.dumps({"type": "net", "what": "304", "path": path}) + "\n")
                    writer.write(("HTTP/1.1 304 Not Modified\r\n%sConnection: close\r\n\r\n" % xh).encode())
                    await writer.drain(); writer.close(); return
            writer.write(("HTTP/1.1 %d X\r\nContent-Type: %s\r\n%sContent-Length: %d\r\nConnection: close\r\n\r\n" % (st, ct, xh, len(data))).encode() + data)
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
    fs = await asyncio.start_server(front, "127.0.0.1", int(os.environ.get("FAKE_PORT", "0")))  # a61: FAKE_PORT = fixed port (a restart must keep the address the app saved)
    global HTTPPORT
    HTTPPORT = fs.sockets[0].getsockname()[1]
    print("PORT", fs.sockets[0].getsockname()[1], flush=True)
    async with fs:
        await fs.serve_forever()

asyncio.run(main())
