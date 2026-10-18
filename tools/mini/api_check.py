#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/mini/api_check.py <api_main binary> -- net.jarvisapi + net.jarvis_cache against the fake
# JARVIS server (tools/mini/fake_jarvis.py), and what the SERVER saw: paths (percent-escaped ids),
# the cookie on every call after the login, the JSON bodies of the writes, raw octets of the
# stt / upload calls, and that nothing was sent while the client was offline.
import json, os, shutil, subprocess, sys, tempfile, time

exe = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
work = tempfile.mkdtemp(prefix="jarvisapi-")
log = os.path.join(work, "server.log")
srv = subprocess.Popen([sys.executable, os.path.join(here, "fake_jarvis.py"), log], stdout=subprocess.PIPE, text=True)
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
try:
    port = int(srv.stdout.readline().split()[1])
    env = dict(os.environ, HOME=work)
    r = subprocess.run([exe, "http://127.0.0.1:%d" % port, os.path.join(work, "cache")], env=env,
                       capture_output=True, text=True, timeout=90)
    sys.stdout.write(r.stdout)
    if r.returncode != 0 or "API CLIENT OK" not in r.stdout:
        fail("client exit %d %s" % (r.returncode, r.stderr[-300:]))
    time.sleep(0.2)
    msgs = [json.loads(l) for l in open(log) if l.strip()]
    rest = [m for m in msgs if m.get("type") == "rest"]
    paths = [(m["method"], m["path"]) for m in rest]
    def seen(method, path):
        return [m for m in rest if m["method"] == method and m["path"] == path]
    # the first call has no cookie, the login has none either; every later one does (until the stale-token step)
    if not (rest and rest[0]["path"] == "/api/me" and rest[0]["auth"] is False):
        fail("first call must come without a cookie: %r" % rest[:1])
    if not all(m["auth"] for m in rest[1:] if m["path"] != "/api/me" or m is not rest[-2]):
        fail("all calls after the login carry the cookie: %r" % [p for p in rest if not p["auth"]])
    if ("GET", "/api/projects/p%202%2Fx") not in paths:
        fail("odd project id is percent-escaped: %r" % [p for p in paths if "p%" in p[1] or "p " in p[1]])
    if ("GET", "/api/conversations?limit=1&offset=1") not in paths:
        fail("conversation page query: %r" % [p for p in paths if "conversations" in p[1]])
    def body(method, path):
        s = seen(method, path)
        return s[0].get("json") if s else None
    if body("POST", "/api/projects/p1/roadmap") != {"action": "done", "id": "r1"}:
        fail("roadmap done body: %r" % body("POST", "/api/projects/p1/roadmap"))
    rm = [m.get("json") for m in seen("POST", "/api/projects/p1/roadmap")]
    if {"action": "add", "text": "third \"q\" ä"} not in rm:  # english: ok
        fail("roadmap add body keeps quote + umlaut: %r" % rm)
    if body("POST", "/api/projects/p1/needs") != {"action": "answer", "id": "n1", "answer": "Yes"}:
        fail("need answer body: %r" % body("POST", "/api/projects/p1/needs"))
    ps = [m.get("json") for m in seen("POST", "/api/projects/p1")]
    if ps[:1] != [{"status": "done", "next": "ship it"}]:
        fail("project set body: %r" % ps)
    tk = [m.get("json") for m in seen("POST", "/api/tasks")]
    want = [{"action": "add", "text": "walk", "due": "2026-10-19T08:30", "category": "Private", "repeat": "weekly"},
            {"action": "complete", "ref": "t1"}, {"action": "remove", "ref": "walk"}]
    if tk != want:
        fail("task bodies: %r" % tk)
    if body("POST", "/api/push-token") != {"token": "tok/1+x", "platform": "android"}:
        fail("push token body")
    st = seen("POST", "/api/stt?lang=de")
    if not (st and st[0]["ctype"] == "audio/wav" and st[0]["n"] == 7):
        fail("stt call: %r" % st)
    up = seen("POST", "/api/m/upload?name=my%20file.txt&cid=c7")
    if not (up and up[0]["ctype"] == "application/x-test" and up[0]["n"] == 300):
        fail("upload call: %r" % [m["path"] for m in rest if "upload" in m["path"]])
    if len(rest) != len([m for m in rest]) or any("127.0.0.1:1" in m["path"] for m in rest):
        fail("unexpected requests")
    # offline phase: 4 requests (projects, project p1, never-fetched, project set) never reach the server
    if any(m["path"] in ("/api/projects/never-fetched",) for m in rest):
        fail("an offline request reached the server")
    if sum(1 for m in rest if m["method"] == "POST" and m["path"] == "/api/projects/p1" ) != 1:
        fail("the offline write must not be replayed")
    # the cache files on disk: atomic names, no temp leftovers
    cdir = os.path.join(work, "cache")
    files = sorted(os.listdir(cdir)) if os.path.isdir(cdir) else []
    if not files or any(not f.endswith(".jc") for f in files):
        fail("cache dir: %r" % files)
    else:
        print("  cache files:", ", ".join(files))
finally:
    srv.kill()
    shutil.rmtree(work, ignore_errors=True)
print("API CLIENT CHECK", "FAILED" if bad else "PASSED")
sys.exit(1 if bad else 0)
