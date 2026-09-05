#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/wpt.py -- the official DOM tests of the Web Platform Tests,
through the browser of round B4.

HOW A TEST IS RUN, and why it is the real harness and not a rewrite of it:

  * the test file is read out of tests/data/wpt-dom (PROVENANCE.md there);
  * every `<script src="...">` it names is replaced by the CONTENTS of
    that file out of the same corpus -- this engine has no loader for
    local files, and inlining changes nothing a DOM test can observe;
  * `/resources/testharnessreport.js`, which in a browser talks to the
    test runner, is replaced by four lines that call
    `add_completion_callback` and print the result. That is the same
    interface `wptrunner` uses;
  * `resources/testharness.js` itself is the ORIGINAL, unmodified, 5207
    lines of it, and it runs inside this engine.

WHAT IS COUNTED. Three columns, and the third is the honest one:

  passed      subtests the harness reported as PASS
  failed      subtests the harness reported as FAIL/TIMEOUT/PRECONDITION
  could not run   files whose harness never reached the completion
              callback at all -- because the test needs an iframe, a
              worker, `fetch`, or an API this round does not have. They
              are NOT silently dropped and NOT counted as passes.

A file that produces ZERO subtests is treated as "could not run" even if
it exits cleanly: a test that asserts nothing passes by doing nothing, and
that is the mistake round B3 wrote down about empty reference pictures.
"""
import argparse
import json
import os
import re
import struct
import subprocess
import sys

ROOT = "tests/data/wpt-dom"
UA = (b"html,body,div,p,h1,h2,h3,table,tr,td,ul,ol,li,form,fieldset,"
      b"section,article,header,footer,nav,main,figure,blockquote,pre,"
      b"dl,dt,dd,hr{display:block}"
      b"span,a,b,i,em,strong,code,small,label{display:inline}"
      b"head,style,script,title,meta,link{display:none}"
      b"body{margin:8px}")

# `<script src=/resources/testharness.js>` -- UNQUOTED, and a third of the
# corpus writes it that way. A regex that insists on quotes silently
# leaves the harness out and every test in the file reports
# "async_test is not defined".
SRC_RE = re.compile(
    r"""<script[^>]*\ssrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"""
    r"""[^>]*>\s*</script\s*>""", re.IGNORECASE)

REPORT = """
add_completion_callback(function(tests, status){
  print("#HARNESS " + status.status);
  for (var i = 0; i < tests.length; i++) {
    var m = (tests[i].message || "").replace(/[\\r\\n]+/g, " ");
    print("#SUB " + tests[i].status + " | " + tests[i].name + " | " + m);
  }
  print("#END " + tests.length);
});
"""


def u32(v):
    return struct.pack("<I", v)


def blob(b):
    return u32(len(b)) + b


def resolve(rel, href):
    href = href.split("?")[0].split("#")[0]
    if href.startswith("/"):
        return href[1:]
    return os.path.normpath(os.path.join(os.path.dirname(rel), href))


def inline(rel, text, missing):
    """Replaces every `<script src>` by the file it names."""
    def sub(m):
        href = m.group(1) or m.group(2) or m.group(3) or ""
        if "testharnessreport" in href:
            return "<script>" + REPORT + "</script>"
        p = os.path.join(ROOT, resolve(rel, href))
        if not os.path.exists(p):
            missing.append(href)
            return "<script></script>"
        body = open(p, "r", encoding="utf-8", errors="replace").read()
        # A script body may not contain `</script>`; the corpus does not.
        return "<script>\n" + body + "\n</script>"
    return SRC_RE.sub(sub, text)


def run_one(binary, rel, timeout, ms):
    path = os.path.join(ROOT, rel)
    text = open(path, "r", encoding="utf-8", errors="replace").read()
    missing = []
    text = inline(rel, text, missing)
    if "add_completion_callback" not in text:
        # no testharnessreport.js in the file -- put the reporter last
        text += "\n<script>" + REPORT + "</script>\n"
    html = text.encode("utf-8", "replace")
    loc = ("http://web-platform.test:8000/" + rel).encode()
    payload = (u32(800) + u32(600) + blob(html) + blob(UA) + blob(b"")
               + blob(loc) + u32(0) + u32(ms))
    try:
        p = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=timeout)
        out = p.stdout.decode("utf-8", "replace")
        rc = p.returncode
    except subprocess.TimeoutExpired:
        return {"file": rel, "ran": False, "why": "timeout", "pass": 0,
                "fail": 0, "subtests": 0, "missing": missing}
    except Exception as ex:
        return {"file": rel, "ran": False, "why": str(ex)[:60], "pass": 0,
                "fail": 0, "subtests": 0, "missing": missing}
    npass = 0
    nfail = 0
    names = []
    done = False
    for line in out.splitlines():
        if line.startswith("#SUB "):
            st = line[5:].split(" | ")[0]
            nm = line[5:].split(" | ")[1] if " | " in line[5:] else ""
            if st == "0":
                npass += 1
            else:
                nfail += 1
                names.append(nm)
        elif line.startswith("#END "):
            done = True
    if not done or (npass + nfail) == 0:
        why = "no result"
        if rc != 0:
            why = "exit %d" % rc
        for line in out.splitlines():
            if line.startswith("SCRIPT-THROW") or line.startswith(
                    "SCRIPT-PARSE") or line.startswith("TIMER-THROW"):
                why = line[:90]
                break
        return {"file": rel, "ran": False, "why": why, "pass": 0, "fail": 0,
                "subtests": 0, "missing": missing}
    return {"file": rel, "ran": True, "why": "", "pass": npass,
            "fail": nfail, "subtests": npass + nfail, "missing": missing,
            "failed_names": names[:5]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("--json", default="")
    ap.add_argument("--timeout", type=float, default=25.0)
    ap.add_argument("--ms", type=int, default=4000)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--verbose", action="store_true")
    a = ap.parse_args()

    files = []
    for base, _, names in os.walk(ROOT):
        for n in sorted(names):
            if n.endswith(".html"):
                files.append(os.path.relpath(os.path.join(base, n), ROOT))
    files.sort()
    if a.only:
        files = [f for f in files if a.only in f]
    if a.limit:
        files = files[:a.limit]

    res = []
    for i, rel in enumerate(files):
        r = run_one(a.binary, rel, a.timeout, a.ms)
        res.append(r)
        if a.verbose:
            print("  %-58s %s" % (rel[:58], "%d/%d" % (r["pass"],
                  r["subtests"]) if r["ran"] else "-- " + r["why"]))
        elif (i + 1) % 25 == 0:
            print("   ... %d/%d files" % (i + 1, len(files)), flush=True)

    ran = [r for r in res if r["ran"]]
    dead = [r for r in res if not r["ran"]]
    npass = sum(r["pass"] for r in ran)
    nfail = sum(r["fail"] for r in ran)
    nsub = npass + nfail
    full = [r for r in ran if r["fail"] == 0]

    print("   files in the corpus          %d" % len(files))
    print("   files whose harness ran      %d" % len(ran))
    print("   files that could not run     %d" % len(dead))
    print("   files with every subtest OK  %d" % len(full))
    print("   subtests                     %d" % nsub)
    print("   subtests passed              %d" % npass)
    print("   subtests failed              %d" % nfail)
    if nsub:
        print("B4-WPT: %d / %d subtests (%.2f %%), %d / %d files whole, "
              "%d files could not run"
              % (npass, nsub, 100.0 * npass / nsub, len(full), len(files),
                 len(dead)))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"files": len(files), "ran": len(ran),
                       "dead": len(dead), "whole": len(full),
                       "subtests": nsub, "pass": npass, "fail": nfail,
                       "detail": res}, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
