#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/mini/check.py <client_main binary> -- net.jarvisc against the fake JARVIS server
# (tools/mini/fake_jarvis.py), and what the SERVER saw: the login body, the tz message,
# message / stop / confirm_result / ask_result, the Cookie (url-encoded token).
import json, os, subprocess, sys, tempfile, time

exe = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
work = tempfile.mkdtemp(prefix="jarvisc-")
log = os.path.join(work, "server.log")
srv = subprocess.Popen([sys.executable, os.path.join(here, "fake_jarvis.py"), log], stdout=subprocess.PIPE, text=True)
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
try:
    port = int(srv.stdout.readline().split()[1])
    env = dict(os.environ, XDG_DATA_HOME=os.path.join(work, "data"), HOME=work)
    r = subprocess.run([exe, "http://127.0.0.1:%d" % port], env=env, capture_output=True, text=True, timeout=60)
    sys.stdout.write(r.stdout)
    if r.returncode != 0 or "MINI CLIENT OK" not in r.stdout:
        fail("client exit %d" % r.returncode)
    time.sleep(0.2)
    msgs = [json.loads(l) for l in open(log) if l.strip()]
    by = lambda t: [m for m in msgs if m.get("type") == t]
    lg = by("login")
    if not (len(lg) == 2 and lg[0]["user"] == "justin" and lg[0]["ctype"] == "application/json"):
        fail("login requests: %r" % lg)
    tz = by("tz")
    if not (len(tz) >= 2 and tz[0]["tz"] == "Europe/Vienna"):
        fail("tz messages: %r" % tz)
    ms = by("message")
    texts = [m["text"] for m in ms]
    if texts != ["hallo", "other", "confirm", "ask", "slow", "drop", "queued"]:
        fail("messages: %r" % texts)
    if ms and not (ms[0]["cid"] is None and ms[0]["jarvisMode"] is True and ms[0]["mode"] == "text"
                   and ms[0]["msgId"].startswith("mini") and ms[0]["images"] == [] and ms[0]["files"] == []):
        fail("first message shape: %r" % ms[0])
    if any(m["cid"] != "fake1" for m in ms[1:]):
        fail("later messages carry the chat id")
    if not by("stop") or by("stop")[0]["cid"] != "fake1":
        fail("stop message: %r" % by("stop"))
    cr = by("confirm_result")
    if not (cr and cr[0]["id"] == "c1" and cr[0]["allow"] is True):
        fail("confirm_result: %r" % cr)
    ar = by("ask_result")
    if not (ar and ar[0]["id"] == "a1" and ar[0]["answers"] == ["B"]):
        fail("ask_result: %r" % ar)
finally:
    srv.kill()
    import shutil
    shutil.rmtree(work, ignore_errors=True)
print("MINI CLIENT CHECK", "FAILED" if bad else "PASSED")
sys.exit(1 if bad else 0)
