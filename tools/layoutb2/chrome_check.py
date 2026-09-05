#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""CALIBRATION, not acceptance: the same corpus through a real Chromium.

`harness.py` compares the numbers of `lib/layout/b2_main.fi` against the
`data-expected-*` attributes of the WPT tests. Two questions can be asked
about that comparison itself, and neither of them can be answered by the
engine under test:

  1. Are the RULES right? `offsetLeft` is measured against the padding
     edge of the offsetParent -- except when the offsetParent is the body,
     where every engine returns the coordinate in the initial containing
     block instead. A harness that gets that wrong marks correct layout as
     wrong.
  2. Which tests can be passed AT ALL? A test of the harvested corpus may
     use a property no engine here knows -- and some of them Chromium
     fails too.

This tool answers both: it runs the corpus through the browser with the
SAME comparison (the JavaScript below reads exactly the attributes
harness.py reads and applies the same tolerance of one pixel) and prints
the quota. Its result is a number to hold the engine against, not a pass
mark for anything.

It needs a Chromium and the Ahem font and is therefore NEVER called from
test.sh. Round 78 froze the browser out of the acceptance on purpose.

Usage:
    python3 tools/layoutb2/chrome_check.py [--only SUB] [--limit N]
                                           [--json out.json]
"""

import argparse
import http.server
import json
import os
import re
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import harness  # noqa: E402

PROBE = r"""
<script>
(function () {
  var KEYS = {
    "data-expected-width": ["num", function (e) { return e.offsetWidth; }],
    "data-expected-height": ["num", function (e) { return e.offsetHeight; }],
    "data-offset-x": ["num", function (e) { return e.offsetLeft; }],
    "data-offset-y": ["num", function (e) { return e.offsetTop; }],
    "data-expected-client-width": ["num", function (e) { return e.clientWidth; }],
    "data-expected-client-height": ["num", function (e) { return e.clientHeight; }],
    "data-expected-scroll-width": ["num", function (e) { return e.scrollWidth; }],
    "data-expected-scroll-height": ["num", function (e) { return e.scrollHeight; }],
    "data-expected-bounding-client-rect-width":
        ["num", function (e) { return e.getBoundingClientRect().width; }],
    "data-expected-bounding-client-rect-height":
        ["num", function (e) { return e.getBoundingClientRect().height; }],
    "data-total-x": ["num", function (e) { return e.clientLeft + e.offsetLeft; }],
    "data-total-y": ["num", function (e) { return e.clientTop + e.offsetTop; }],
    "data-expected-display": ["str", function (e) { return getComputedStyle(e).display; }],
    "data-expected-padding-top": ["px", function (e) { return getComputedStyle(e).paddingTop; }],
    "data-expected-padding-bottom": ["px", function (e) { return getComputedStyle(e).paddingBottom; }],
    "data-expected-padding-left": ["px", function (e) { return getComputedStyle(e).paddingLeft; }],
    "data-expected-padding-right": ["px", function (e) { return getComputedStyle(e).paddingRight; }],
    "data-expected-margin-top": ["px", function (e) { return getComputedStyle(e).marginTop; }],
    "data-expected-margin-bottom": ["px", function (e) { return getComputedStyle(e).marginBottom; }],
    "data-expected-margin-left": ["px", function (e) { return getComputedStyle(e).marginLeft; }],
    "data-expected-margin-right": ["px", function (e) { return getComputedStyle(e).marginRight; }]
  };
  function run() {
    var checks = 0, fails = [];
    var all = document.querySelectorAll("*");
    for (var i = 0; i < all.length; i++) {
      var e = all[i];
      for (var k in KEYS) {
        var want = e.getAttribute(k);
        if (!want) { continue; }
        checks++;
        var kind = KEYS[k][0], got = KEYS[k][1](e);
        if (kind === "str") {
          if (String(got) !== want.trim()) { fails.push([e.tagName, k, want, String(got)]); }
        } else {
          if (kind === "px") { got = parseFloat(String(got)); }
          var w = parseFloat(want);
          var eps = (kind === "num") ? 1.0 : 1.0 / 64.0;
          if (!(Math.abs(got - w) < eps)) { fails.push([e.tagName, k, want, got]); }
        }
      }
    }
    document.title = "@@" + JSON.stringify({ checks: checks, fails: fails,
                                             vw: document.documentElement.clientWidth });
  }
  if (document.readyState === "complete") { run(); }
  else { window.addEventListener("load", run); }
  setTimeout(run, 300);
})();
</script>
"""

def serve(directory):
    class H(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=directory, **kw)

        def log_message(self, *a):
            pass

    socketserver.TCPServer.allow_reuse_address = True
    httpd = socketserver.TCPServer(("127.0.0.1", 0), H)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd.server_address[1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--json")
    ap.add_argument("--group", default="b2")
    args = ap.parse_args()

    chromium = shutil.which("chromium") or shutil.which("chromium-browser") \
        or shutil.which("google-chrome")
    if not chromium:
        print("no chromium found")
        return 2

    tests = [t for t in harness.collect() if t["group"] == args.group]
    if args.only:
        tests = [t for t in tests if args.only in t["name"]]
    if args.limit:
        tests = tests[:args.limit]

    mirror = tempfile.mkdtemp(prefix="b2chrome")
    shutil.copytree(harness.DATA, os.path.join(mirror, "w"))
    for t in tests:
        p = os.path.join(mirror, "w", t["name"])
        text = open(p, encoding="utf-8", errors="replace").read()
        open(p, "w", encoding="utf-8").write(text + PROBE)
    port = serve(os.path.join(mirror, "w"))

    profile = tempfile.mkdtemp(prefix="b2prof")
    results = []
    for t in tests:
        url = "http://127.0.0.1:%d/%s" % (port, t["name"].replace(os.sep, "/"))
        try:
            out = subprocess.run(
                [chromium, "--headless", "--disable-gpu", "--no-sandbox",
                 "--hide-scrollbars", "--window-size=800,600",
                 "--user-data-dir=" + profile, "--virtual-time-budget=1200",
                 "--dump-dom", url],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                timeout=60).stdout.decode("utf-8", "replace")
        except subprocess.TimeoutExpired:
            results.append({"name": t["name"], "checks": 0,
                            "fails": [["timeout", "", "", ""]]})
            continue
        m = re.search(r"<title>@@(.*?)</title>", out, re.S)
        if not m:
            results.append({"name": t["name"], "checks": 0,
                            "fails": [["no-probe", "", "", ""]]})
            continue
        data = json.loads(m.group(1).replace("&quot;", '"').replace("&amp;", "&")
                          .replace("&lt;", "<").replace("&gt;", ">"))
        results.append({"name": t["name"], "checks": data["checks"],
                        "fails": data["fails"], "vw": data.get("vw")})

    ok = sum(1 for r in results if r["checks"] and not r["fails"])
    checks = sum(r["checks"] for r in results)
    good = sum(r["checks"] - len(r["fails"]) for r in results)
    print("Chromium on corpus %s: %d / %d tests, %d / %d checks"
          % (args.group, ok, len(results), good, checks))
    vws = set(r.get("vw") for r in results if r.get("vw"))
    print("layout viewport reported by the browser:", vws)
    for r in results:
        if r["fails"]:
            print("  FAIL %s (%d)" % (r["name"], len(r["fails"])),
                  r["fails"][:2])
    if args.json:
        json.dump(results, open(args.json, "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
