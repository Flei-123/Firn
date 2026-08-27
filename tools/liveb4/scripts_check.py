#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/scripts_check.py -- `<script src>`, over a REAL socket.

The ordering rules of HTML 8.1.3.1 only become visible with EXTERNAL
scripts: `defer` and `async` do nothing at all on an inline one, so a test
made of inline scripts cannot tell a correct implementation from one that
ignores both attributes. These scripts are therefore fetched over HTTP
from tools/liveb4/server.py, which is the same server the HTTP client is
measured against.

What is checked, and what is NOT claimed:

  * a classic script without `defer`/`async` blocks: they run in document
    order, before anything else;
  * `defer` runs AFTER every parser-blocking script, in document order;
  * `async` runs after the parser-blocking ones and before the deferred
    ones, and never blocks an earlier script.

The third line is this browser's MODEL, not the standard: the fetch here
is synchronous, so "as soon as it has arrived" collapses to "in the order
they were started". Everything the standard actually guarantees -- the
relative order of the first two groups, and that neither `async` nor
`defer` runs before a parser-blocking script -- is checked exactly. The
difference is written down in docs/ROUNDB4.md and repeated here so that
nobody reads more into the number than is in it.
"""
import re
import struct
import subprocess
import sys


def u32(v):
    return struct.pack("<I", v)


def blob(b):
    return u32(len(b)) + b


UA = b"html,body,div,p{display:block}head,script{display:none}"


def start_server():
    p = subprocess.Popen([sys.executable, "tools/liveb4/server.py"],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    line = p.stdout.readline().decode().strip()
    m = re.match(r"port (\d+)", line)
    if not m:
        p.kill()
        raise SystemExit("the test server printed %r" % line)
    return p, int(m.group(1))


def run(binary, html, loc, timeout=60):
    payload = (u32(800) + u32(600) + blob(html) + blob(UA) + blob(b"")
               + blob(loc.encode()) + u32(4) + u32(500))
    p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=timeout)
    out = p.stdout.decode("utf-8", "replace")
    order = []
    rep = {}
    for line in out.splitlines():
        if line.startswith(("SCRIPTS ", "EVENTS ", "STYLED ")):
            parts = line.split()
            for k, v in zip(parts[0::2], parts[1::2]):
                rep[k] = v
        elif line.startswith(("TEXT ", "HTML ")):
            pass
        elif line.strip():
            order.append(line.strip())
    return order, rep, p.returncode


def main():
    binary = sys.argv[1]
    srv, port = start_server()
    base = "http://127.0.0.1:%d" % port
    bad = 0
    good = 0
    try:
        html = ("""<!doctype html><html><head>
<script src="%s/js/d1.js" defer></script>
<script src="%s/js/a1.js" async></script>
<script src="%s/js/b1.js"></script>
</head><body>
<script>print("inline");</script>
<script src="%s/js/b2.js"></script>
<script src="%s/js/d2.js" defer></script>
<script src="%s/js/a2.js" async></script>
</body></html>""" % ((base,) * 6)).encode()
        order, rep, rc = run(binary, html, base + "/page.html")
        want = ["b1", "inline", "b2", "a1", "a2", "d1", "d2"]
        checks = [
            ("every script ran", int(rep.get("SCRIPTS", "0")) == 7,
             "7 scripts, got %s" % rep.get("SCRIPTS")),
            ("nothing failed", int(rep.get("ERRORS", "1")) == 0,
             "ERRORS=%s" % rep.get("ERRORS")),
            ("the parser-blocking ones came first and in document order",
             order[:3] == ["b1", "inline", "b2"], "got %r" % order[:3]),
            ("the deferred ones came LAST and in document order",
             order[-2:] == ["d1", "d2"], "got %r" % order[-2:]),
            ("no deferred script ran before a blocking one",
             order.index("d1") > order.index("b2"), "got %r" % order),
            ("no async script ran before a blocking one",
             order.index("a1") > order.index("b2"), "got %r" % order),
            ("the whole order", order == want,
             "want %r got %r" % (want, order)),
        ]
        for name, ok, detail in checks:
            if ok:
                good += 1
            else:
                bad += 1
                print("      FAIL %s -- %s" % (name, detail))
        print("   script order     %d / %d, %s scripts fetched over HTTP"
              % (good, good + bad, rep.get("SCRIPTS")))

        # COUNTER-CHECK: without the HTTP flag no external script can run
        payload = (u32(800) + u32(600) + blob(html) + blob(UA) + blob(b"")
                   + blob((base + "/page.html").encode()) + u32(0)
                   + u32(500))
        p = subprocess.run([binary], input=payload,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           timeout=60)
        out = p.stdout.decode()
        n_err = 0
        for line in out.splitlines():
            if line.startswith("SCRIPTS "):
                parts = line.split()
                n_err = int(parts[parts.index("ERRORS") + 1])
        if n_err == 6:
            good += 1
            print("   counter-check    with the fetcher switched off all "
                  "six external scripts fail and only the inline one runs")
        else:
            bad += 1
            print("      FAIL counter-check: %d errors, expected 6" % n_err)
        if bad:
            print("SCRIPTS FAIL: %d of %d" % (bad, good + bad))
            return 1
        print("SCRIPTS OK: %d rules, seven scripts over a real socket, "
              "0 wrong" % good)
        return 0
    finally:
        srv.kill()
        srv.wait()


if __name__ == "__main__":
    sys.exit(main())
