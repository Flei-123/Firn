#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""The update test of lib/appkit, as a kit that runs on ONE machine with Python 3.

It is the same story as tools/appkit/e2e.sh (check, download, a changed byte
refused, a wrong signature refused, the real replacement of the running
program, rollback after a crash and after a hang) in a form you can copy to a
Windows PC (or run on Linux) and start with

    py run.py            (Windows)        python3 run.py            (Linux)

It needs nothing but Python 3: a small web server on 127.0.0.1 plays the
store (three signed catalogs made on the build machine, the files of three
versions), the programs under test are the ones in this folder, and every
program keeps its settings and state in a temporary folder (APPKIT_HOME).
Nothing leaves this machine, nothing is installed, and the only thing left
behind is what the operating system itself caches.

The catalogs are valid for 14 days from the day the kit was made (the store
format's freeze protection); after that the programs refuse them -- make a
new kit with tools/appkit/winkit.sh.

Options:  --keep   keep the temporary folder (and print where it is)
          --only N run one section only (1..8)
Exit code 0 = every check passed.
"""
import hashlib
import json
import os
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading
import http.server
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PORT = int(open(os.path.join(HERE, "port.txt")).read().strip()) if os.path.exists(os.path.join(HERE, "port.txt")) else 18765
EXT = ".exe" if os.name == "nt" else ""
RUNNER = os.environ.get("KIT_RUNNER", "").split()          # e.g. "wine" to try the Windows kit on Linux
if RUNNER:
    EXT = ".exe"
PASS = 0
FAIL = 0
FAILED = []


def ok(name):
    global PASS
    PASS += 1
    print("  ok    %s" % name, flush=True)


def bad(name, why=""):
    global FAIL
    FAIL += 1
    FAILED.append(name)
    print("  FAIL  %s" % name, flush=True)
    if why:
        for line in str(why).splitlines()[:8]:
            print("        %s" % line, flush=True)


def expect(name, hay, needle):
    if needle in hay:
        ok(name)
    else:
        bad(name, "wanted %r in:\n%s" % (needle, hay))


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class Store:
    """A plain file server on 127.0.0.1 over the folder `root`."""

    def __init__(self, root, port):
        self.root = root

        class H(http.server.SimpleHTTPRequestHandler):
            def __init__(s, *a, **k):
                super().__init__(*a, directory=root, **k)

            def log_message(s, *a):
                pass

        class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
            daemon_threads = True
            allow_reuse_address = True

        self.srv = S(("127.0.0.1", port), H)
        self.thread = threading.Thread(target=self.srv.serve_forever, daemon=True)
        self.thread.start()

    def stop(self):
        self.srv.shutdown()
        self.srv.server_close()


def winpath(p):
    """For Wine: a Unix path as the Windows program sees it."""
    return "Z:" + p.replace("/", "\\") if RUNNER else p


def main():
    keep = "--keep" in sys.argv
    only = None
    if "--only" in sys.argv:
        only = int(sys.argv[sys.argv.index("--only") + 1])
    W = tempfile.mkdtemp(prefix="appkit-kit-")
    served = os.path.join(W, "served")
    inst = os.path.join(W, "inst")
    home = os.path.join(W, "home")
    os.makedirs(served)
    os.makedirs(inst)
    os.makedirs(home)
    # the files every state needs: the payloads
    shutil.copytree(os.path.join(HERE, "speicher"), os.path.join(served, "speicher"))

    def state(name):
        """Put a catalog state (entry, index and their signatures) in front of the programs."""
        src = os.path.join(HERE, "state-" + name)
        for f in os.listdir(src):
            shutil.copy2(os.path.join(src, f), os.path.join(served, f))

    store = Store(served, PORT)
    app = os.path.join(inst, "app" + EXT)
    shutil.copy2(os.path.join(HERE, "app-1.0.0" + EXT), app)
    os.chmod(app, 0o755)

    def run(args, h=None, prog=None, timeout=180):
        env = dict(os.environ)
        env["APPKIT_HOME"] = winpath(h or home)
        cmd = RUNNER + [prog or app] + list(args)
        try:
            r = subprocess.run(cmd, env=env, capture_output=True, timeout=timeout, cwd=inst)
            return (r.stdout + r.stderr).decode("utf-8", "replace").replace("\r", "")
        except subprocess.TimeoutExpired as e:
            return "TIMEOUT\n" + ((e.stdout or b"") + (e.stderr or b"")).decode("utf-8", "replace")

    def settings(**kv):
        d = os.path.join(home, "config", "e2eapp")
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, "settings.json")
        if not kv:
            if os.path.exists(p):
                os.remove(p)
        else:
            json.dump(kv, open(p, "w"))

    payload = {}
    for v in ("1.1.0", "1.2.0", "1.3.0"):
        payload[v] = os.path.join(HERE, "app-%s%s" % (v, EXT))
    size11 = os.path.getsize(payload["1.1.0"])

    def sect(n, title):
        print("== %d. %s" % (n, title), flush=True)
        return only is None or only == n

    try:
        if sect(1, "the program itself, on " + sys.platform):
            out = run(["version"])
            expect("the installed program is 1.0.0", out, "VERSION 1.0.0")

        state("a")
        if sect(2, "check: blocking and in a second process"):
            for mode in ("blocking", "process"):
                out = run(["check", mode])
                expect("check (%s): update available" % mode, out, "RESULT update available err=0 ver=1.1.0")
                expect("check (%s): the size is the file's" % mode, out, "size=%d" % size11)
            out = run(["state"])
            expect("state: the catalog revision is remembered", out, "STATE seen.revision=")
            expect("state: the entry was seen", out, "STATE entry.seen=1")

        if sect(3, "fetch: download, progress, the staged file"):
            out = run(["fetch", "process"])
            expect("fetch: ready", out, "RESULT ready to install err=0 ver=1.1.0")
            expect("fetch: progress reached the end", out, "PROGRESS %d %d" % (size11, size11))
            staged = app + ".new"
            if os.path.exists(staged) and sha(staged) == sha(payload["1.1.0"]):
                ok("the staged file is byte for byte the published one")
            else:
                bad("the staged file differs from the published one or is missing")
            if os.path.exists(staged):
                os.remove(staged)

        if sect(4, "a changed byte and a wrong signature are refused"):
            speicher = os.path.join(served, "speicher")
            target = None
            for d, _, files in os.walk(speicher):
                for f in files:
                    if sha(os.path.join(d, f)) == sha(payload["1.1.0"]):
                        target = os.path.join(d, f)
            good = open(target, "rb").read()
            bad_bytes = bytearray(good)
            bad_bytes[len(bad_bytes) // 2] ^= 1
            open(target, "wb").write(bytes(bad_bytes))
            out = run(["fetch", "process"])
            expect("a changed byte in the download is refused (hash)", out, "err=7")
            if not os.path.exists(app + ".new"):
                ok("and the file is deleted")
            else:
                bad("the refused download was left behind")
            open(target, "wb").write(good)
            sig = os.path.join(served, "entry.json.sig")
            good_sig = open(sig, "rb").read()
            b = bytearray(good_sig)
            b[10] ^= 1
            open(sig, "wb").write(bytes(b))
            out = run(["check", "process"])
            expect("a wrong signature is refused", out, "err=2")
            open(sig, "wb").write(good_sig)

        if sect(5, "the real replacement, and the confirmation"):
            settings(**{"update.healthy_s": 8})
            out = run(["update", "process"])
            expect("update: the new version is downloaded and verified", out, "RESULT ready to install err=0 ver=1.1.0")
            expect("update: the program file was replaced", out, "APPLY 0")
            expect("update: the new version confirmed itself", out, "SUPERVISE healthy")
            out = run(["version"])
            expect("the program on disk is now 1.1.0", out, "VERSION 1.1.0")
            if sha(app) == sha(payload["1.1.0"]):
                ok("and it is byte for byte the published file")
            else:
                bad("the replaced program differs from the published one")
            out = run(["state"])
            expect("state: the update is confirmed", out, "STATE pending.state=confirmed")
            out = run(["check", "process"])
            expect("after the update: up to date (same file as the store's)", out, "RESULT up to date")
            time.sleep(1)
            if not os.path.exists(app + ".old"):
                ok("the backup is cleaned up after the confirmation")
            else:
                bad("the backup file was left")

        if sect(6, "a developer's own build of the same version is left alone"):
            dev = os.path.join(inst, "dev" + EXT)
            shutil.copy2(os.path.join(HERE, "app-1.1.0-dev" + EXT), dev)
            os.chmod(dev, 0o755)
            out = run(["check", "process"], prog=dev)
            expect("the published 1.1.0 does not overwrite a developer's 1.1.0", out, "RESULT up to date")

        if sect(7, "a new version that CRASHES is rolled back"):
            state("b")
            settings(**{"update.healthy_s": 8})
            out = run(["update", "process"])
            expect("crash: the update was applied", out, "APPLY 0")
            expect("crash: the supervisor rolled back", out, "SUPERVISE rolledback")
            out = run(["version"])
            expect("the program on disk is 1.1.0 again", out, "VERSION 1.1.0")
            if sha(app) == sha(payload["1.1.0"]):
                ok("and it is byte for byte the good file")
            else:
                bad("the rolled back program differs from the good one")
            time.sleep(1)
            out = run(["state"])
            expect("state: the rollback is recorded", out, "STATE pending.state=rolledback")
            expect("state: the bad build is remembered", out, "STATE bad.0=")
            out = run(["check", "process"])
            expect("the failed build is not offered again", out, "RESULT up to date")

        if sect(8, "a new version that HANGS is rolled back after the time limit"):
            state("c")
            settings(**{"update.healthy_s": 4})
            out = run(["update", "process"])
            expect("hang: the update was applied", out, "APPLY 0")
            expect("hang: the supervisor rolled back", out, "SUPERVISE rolledback")
            out = run(["version"])
            expect("the program on disk is 1.1.0 again", out, "VERSION 1.1.0")
            time.sleep(1.5)
            alive = False
            if os.name == "nt":
                r = subprocess.run(["tasklist", "/FI", "IMAGENAME eq app.exe", "/FO", "CSV", "/NH"],
                                   capture_output=True, text=True)
                alive = "app.exe" in r.stdout
            elif RUNNER:
                r = subprocess.run("ps -eo args | grep -c '[i]nst.app\\.exe'", shell=True, capture_output=True, text=True)
                alive = r.stdout.strip() not in ("", "0")
            else:
                r = subprocess.run(["pgrep", "-f", inst + "/app"], capture_output=True, text=True)
                alive = bool(r.stdout.strip())
            if alive:
                bad("the hung new version is still running")
                if os.name == "nt":
                    subprocess.run(["taskkill", "/F", "/IM", "app.exe"], capture_output=True)
                elif RUNNER:
                    subprocess.run(["wineserver", "-k"], capture_output=True)
                else:
                    subprocess.run(["pkill", "-f", inst + "/app"])
            else:
                ok("the hung new version was stopped")
    finally:
        store.stop()
        if keep:
            print("kept: %s" % W)
        else:
            time.sleep(0.5)
            shutil.rmtree(W, ignore_errors=True)

    print()
    print("appkit kit: %d passed, %d failed" % (PASS, FAIL))
    if FAIL:
        print("failed:\n  - " + "\n  - ".join(FAILED))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
