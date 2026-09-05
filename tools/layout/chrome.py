#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Measures the boxes of a case with a REAL browser (Chromium, headless).

This is the counter-witness of round 61.  An expectation written by hand
only proves that the code does what its author thought; the boxes of a
foreign engine prove that the thought was right.

How it works, and why this way:

  * the case is copied into a temporary directory together with the
    measuring font, so that `@font-face` can reach it over a relative URL
  * a measuring script is appended to the copy.  It walks the elements in
    DOCUMENT ORDER -- the same order the Firn driver uses -- calls
    `getBoundingClientRect()` on each and writes the result into a
    `<script type="application/json">`.  A script element carries
    `display: none`, so the measurement cannot move what it measures, and
    the result element is created AFTER the measurement anyway.
  * `chrome-headless-shell --dump-dom` prints the DOM after the script has
    run, and the JSON is read back out of it.

`getBoundingClientRect()` returns the BORDER box in viewport coordinates.
That is exactly what the Firn driver prints, so the two can be subtracted
without a conversion.
"""

import html as htmlmod
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = os.path.join(HERE, "FirnMetric.ttf")

CANDIDATES = [
    os.path.expanduser("~/.cache/ms-playwright/chromium_headless_shell-1228/"
                       "chrome-headless-shell-linux64/chrome-headless-shell"),
]


def find_chromium():
    env = os.environ.get("FIRN_CHROMIUM")
    if env and os.path.exists(env):
        return env
    for c in CANDIDATES:
        if os.path.exists(c):
            return c
    for name in ("chromium", "chromium-browser", "google-chrome",
                 "chrome-headless-shell"):
        p = shutil.which(name)
        if p:
            return p
    # last resort: anything playwright unpacked
    base = os.path.expanduser("~/.cache/ms-playwright")
    if os.path.isdir(base):
        for root, _dirs, files in os.walk(base):
            for f in files:
                if f in ("chrome-headless-shell", "chrome", "headless_shell"):
                    return os.path.join(root, f)
    return None



def chromium_version(exe=None):
    """The version string of the browser -- so a frozen measurement can say
    WHO measured it (round 78, tools/layout/reference.py)."""
    exe = exe or find_chromium()
    if exe is None:
        return None
    try:
        out = subprocess.run([exe, "--version"], capture_output=True,
                             timeout=60).stdout.decode("utf-8", "replace")
    except (OSError, subprocess.TimeoutExpired):
        return None
    out = out.strip().splitlines()
    return out[0].strip() if out else None


PROBE = """
<script>
(function () {
  var run = function () {
    var out = [];
    var walk = function (e) {
      var r = e.getBoundingClientRect();
      out.push([e.tagName.toLowerCase(), r.x, r.y, r.width, r.height]);
      for (var c = e.firstElementChild; c; c = c.nextElementSibling) walk(c);
    };
    walk(document.documentElement);
    var s = document.createElement('script');
    s.type = 'application/json';
    s.id = 'firn-boxes';
    s.textContent = JSON.stringify(out);
    document.documentElement.appendChild(s);
  };
  // The measuring font arrives over @font-face.  Measuring before it is
  // there would compare our engine against a FALLBACK font -- the whole
  // cross-check would be worthless and would look like a bug in the
  // engine.  So: wait for the font, then measure.
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(run);
  } else {
    run();
  }
}());
</script>
"""

RESULT = re.compile(
    r'<script type="application/json" id="firn-boxes">(.*?)</script>',
    re.S)


# ROUND 72 (found while making the suite green again, not part of the
# round's own subject): `--window-size=W,H` sets the WINDOW, not the
# LAYOUT VIEWPORT. Old headless Chromium reserved nothing and the two were
# the same number; the Chromium in Debian 12 (/usr/bin/chromium, the
# fallback used when playwright's `chrome-headless-shell` is not
# installed) subtracts 87 px of browser interface even with `--headless`,
# so `--window-size=800,600` lays out into 800x513. Every `position: fixed`
# and `bottom:`/`sticky` case then measured 87 px away from the Firn side,
# which lays out into exactly 800x600 -- five boxes out of 1087, all of
# them in the four cases that ask where the bottom of the viewport is.
#
# The fix is not a magic number: ASK the browser what viewport it gave us
# and add the difference back to the window size, once, at the start.
# That is correct for a browser that reserves nothing (the difference is
# 0) and for any amount it might reserve in a future version.
VIEWPORT_PROBE = """<html><body><script>
var s = document.createElement('script');
s.type = 'application/json';
s.id = 'firn-boxes';
s.textContent = JSON.stringify([['viewport', 0, 0,
    document.documentElement.clientWidth, document.documentElement.clientHeight]]);
document.documentElement.appendChild(s);
</script></body></html>
"""


def _chrome_cmd(exe, w, h, path):
    return [exe, "--headless", "--no-sandbox", "--disable-gpu",
            "--disable-dev-shm-usage", "--hide-scrollbars",
            "--window-size=%d,%d" % (w, h),
            "--force-device-scale-factor=1",
            "--allow-file-access-from-files",
            "--host-resolver-rules=MAP * ~NOTFOUND",
            "--run-all-compositor-stages-before-draw",
            "--virtual-time-budget=4000",
            "--dump-dom", "file://" + path]


def _measure_viewport(exe, work, w, h, timeout):
    """The layout viewport the browser really hands out for `--window-size=w,h`.
    Returns (width, height) or None when the probe does not come back."""
    path = os.path.join(work, "viewport-probe.html")
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(VIEWPORT_PROBE)
    try:
        dom = subprocess.run(_chrome_cmd(exe, w, h, path), capture_output=True,
                             timeout=timeout).stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        return None
    m = RESULT.search(dom)
    if not m:
        return None
    try:
        box = json.loads(m.group(1))[0]
    except (ValueError, IndexError):
        return None
    return int(box[3]), int(box[4])


def window_size_for(exe, work, want_w, want_h, timeout=60):
    """The `--window-size` that yields a LAYOUT VIEWPORT of want_w x want_h."""
    got = _measure_viewport(exe, work, want_w, want_h, timeout)
    if got is None or got == (want_w, want_h):
        return want_w, want_h
    adj_w = want_w + (want_w - got[0])
    adj_h = want_h + (want_h - got[1])
    check = _measure_viewport(exe, work, adj_w, adj_h, timeout)
    if check == (want_w, want_h):
        return adj_w, adj_h
    # The browser does not do what it is told; the caller is better off
    # with the honest deviation than with a size that is wrong twice.
    return want_w, want_h


# ROUND 78: what the LAST live measurement really used -- the browser and
# the window size that produced the 800x600 layout viewport. The frozen
# reference writes both into its header, and a header that is reconstructed
# afterwards would be a guess.
LAST_EXE = None
LAST_WINDOW = (0, 0)
LAST_VIEWPORT = (800, 600)


def measure_many(cases, timeout=60):
    """cases: list of (name, html).  Returns {name: [[tag,x,y,w,h], ...]}."""
    global LAST_EXE, LAST_WINDOW
    exe = find_chromium()
    if exe is None:
        raise RuntimeError("no Chromium found (set FIRN_CHROMIUM)")
    work = tempfile.mkdtemp(prefix="firn-chrome-")
    try:
        shutil.copyfile(FONT, os.path.join(work, "FirnMetric.ttf"))
        files = []
        for i, (name, text) in enumerate(cases):
            path = os.path.join(work, "case%04d.html" % i)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
                fh.write(PROBE)
            files.append((name, path))
        result = {}
        # One process per batch of cases: starting Chromium costs ~200 ms,
        # so the cases go in as several URLs of one run would not work with
        # --dump-dom.  It prints exactly one DOM.  So: one call per case,
        # but with the cheapest possible flags.
        # ROUND 72: what window size gives a LAYOUT VIEWPORT of 800x600 on
        # THIS browser? Asked once, used for every case.
        win_w, win_h = window_size_for(exe, work, 800, 600, timeout)
        LAST_EXE, LAST_WINDOW = exe, (win_w, win_h)
        for name, path in files:
            cmd = _chrome_cmd(exe, win_w, win_h, path)
            try:
                dom = subprocess.run(cmd, capture_output=True, timeout=timeout
                                     ).stdout.decode("utf-8", "replace")
            except subprocess.TimeoutExpired:
                result[name] = None
                continue
            m = RESULT.search(dom)
            if not m:
                result[name] = None
                continue
            result[name] = json.loads(htmlmod.unescape(m.group(1)))
        return result
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    text = open(sys.argv[1], encoding="utf-8").read()
    got = measure_many([("x", text)])["x"]
    for row in got or []:
        print("%-10s %10.4f %10.4f %10.4f %10.4f" % tuple(row))
