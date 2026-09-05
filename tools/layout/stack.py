#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""The PAINT ORDER of round 67, held against a real browser.

A layout can be proven with rectangles.  A paint order cannot: there is no
`getPaintOrder()` in any browser, and a rectangle says nothing about who
lies on top of whom.  But there is one question whose answer IS the paint
order, and every browser answers it:

    document.elementFromPoint(x, y)  ->  the topmost element at this point

So the proof works like this:

  * the Firn engine prints, after the boxes, one `P` line: the element
    numbers in paint order, back to front (lib/layout/stack.fi)
  * for a point, the topmost element is the LAST entry of that list whose
    border box contains the point -- the same list read from the back
  * Chromium is asked the same question at the same points
  * the two answers are compared, point by point

The points are not a blind grid.  A grid of 25 px would mostly hit places
where nothing overlaps and would prove nothing.  So every box contributes
its own centre and four points just inside its corners -- exactly where an
overlap is decided -- and a coarse grid is added on top so that the empty
places are covered too.

ROUND 78: the browser side is FROZEN. `--reference` reads the probe points
AND Chromium's answer at each of them out of
`tools/layout/reference/stack.json` -- no browser, no network. The points
are frozen along with the answers on purpose: a probe point is a fixed
place on the page, and freezing both makes the question as well as the
answer independent of today's engine. `--write-reference` asks a live
browser once and writes the file.

Usage:
    python3 tools/layout/stack.py <binary> [--json out.json] [--show N]
                                 [--reference] [--write-reference]
                                 [--dir tools/layout/stackcases]
"""

import glob
import html as htmlmod
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
UA = os.path.join(HERE, "ua.css")
FONT = os.path.join(HERE, "FirnMetric.ttf")
DEFAULT_CASES = os.path.join(HERE, "stackcases")

VIEWPORT_W = 800
VIEWPORT_H = 600

sys.path.insert(0, HERE)
import chrome as chrome_mod  # noqa: E402
import harness as harness_mod  # noqa: E402


PROBE = """
<script>
(function () {
  var run = function () {
    var els = [];
    var walk = function (e) {
      els.push(e);
      for (var c = e.firstElementChild; c; c = c.nextElementSibling) walk(c);
    };
    walk(document.documentElement);
    var index = new Map();
    for (var i = 0; i < els.length; i++) index.set(els[i], i);
    var boxes = els.map(function (e) {
      var r = e.getBoundingClientRect();
      return [e.tagName.toLowerCase(), r.x, r.y, r.width, r.height];
    });
    var pts = __POINTS__;
    var hits = pts.map(function (p) {
      var e = document.elementFromPoint(p[0], p[1]);
      if (!e) return -1;
      var v = index.get(e);
      return v === undefined ? -2 : v;
    });
    var s = document.createElement('script');
    s.type = 'application/json';
    s.id = 'firn-stack';
    s.textContent = JSON.stringify({boxes: boxes, hits: hits});
    document.documentElement.appendChild(s);
  };
  if (document.fonts && document.fonts.ready) {
    document.fonts.ready.then(run);
  } else {
    run();
  }
}());
</script>
"""

RESULT = re.compile(
    r'<script type="application/json" id="firn-stack">(.*?)</script>', re.S)


LAST_EXE = None
LAST_WINDOW = (0, 0)


def measure(cases, timeout=60):
    """cases: [(name, html, points)] -> {name: {'boxes':…, 'hits':…}}"""
    global LAST_EXE, LAST_WINDOW
    exe = chrome_mod.find_chromium()
    if exe is None:
        raise RuntimeError("no Chromium found (set FIRN_CHROMIUM)")
    work = tempfile.mkdtemp(prefix="firn-stack-")
    try:
        shutil.copyfile(FONT, os.path.join(work, "FirnMetric.ttf"))
        out = {}
        # ROUND 72/78: `--window-size` is the WINDOW, not the layout
        # viewport. Ask the browser what it really hands out and correct
        # for it -- the same probe harness.py uses. Without this the
        # probe points near the bottom edge are 87 px off on the Chromium
        # of Debian 12, and that error would be FROZEN into the reference.
        win_w, win_h = chrome_mod.window_size_for(exe, work, VIEWPORT_W,
                                                  VIEWPORT_H, timeout)
        LAST_EXE, LAST_WINDOW = exe, (win_w, win_h)
        for i, (name, text, points) in enumerate(cases):
            path = os.path.join(work, "case%04d.html" % i)
            probe = PROBE.replace("__POINTS__", json.dumps(points))
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
                fh.write(probe)
            cmd = [exe, "--headless", "--no-sandbox", "--disable-gpu",
                   "--disable-dev-shm-usage", "--hide-scrollbars",
                   "--window-size=%d,%d" % (win_w, win_h),
                   "--force-device-scale-factor=1",
                   "--allow-file-access-from-files",
                   "--host-resolver-rules=MAP * ~NOTFOUND",
                   "--run-all-compositor-stages-before-draw",
                   "--virtual-time-budget=4000",
                   "--dump-dom", "file://" + path]
            try:
                dom = subprocess.run(cmd, capture_output=True,
                                     timeout=timeout).stdout.decode(
                                         "utf-8", "replace")
            except subprocess.TimeoutExpired:
                out[name] = None
                continue
            m = RESULT.search(dom)
            out[name] = json.loads(htmlmod.unescape(m.group(1))) if m else None
        return out
    finally:
        shutil.rmtree(work, ignore_errors=True)


def parse_engine(lines):
    """-> (boxes in document order, paint order as element indices)."""
    tags, boxes, order = {}, {}, []
    for line in lines:
        if line.startswith("E "):
            p = line.split(" ")
            tags[int(p[1])] = p[3]
        elif line.startswith("B "):
            p = line.split(" ")
            boxes[int(p[1])] = tuple(float(v) for v in p[2:6])
        elif line.startswith("P"):
            order = [int(v) for v in line[1:].split()]
    rows = []
    for i in sorted(tags):
        b = boxes.get(i, (0.0, 0.0, 0.0, 0.0))
        rows.append((tags[i], b[0], b[1], b[2], b[3]))
    return rows, order


def sample_points(rows):
    """The points that decide something: the corners and centres of every
    box, plus a coarse grid for the places where nothing lies."""
    pts = set()
    for _tag, x, y, w, h in rows:
        if w <= 0 or h <= 0:
            continue
        cand = [(x + w / 2.0, y + h / 2.0),
                (x + 1, y + 1), (x + w - 1, y + 1),
                (x + 1, y + h - 1), (x + w - 1, y + h - 1)]
        for px, py in cand:
            px, py = int(px), int(py)
            if 0 <= px < VIEWPORT_W and 0 <= py < VIEWPORT_H:
                pts.add((px, py))
    for px in range(5, VIEWPORT_W, 37):
        for py in range(5, VIEWPORT_H, 41):
            pts.add((px, py))
    return sorted(pts)


def top_at(rows, order, px, py):
    """The topmost element index at a point: the paint order, from the back."""
    for idx in reversed(order):
        if idx >= len(rows):
            continue
        _tag, x, y, w, h = rows[idx]
        if w > 0 and h > 0 and x <= px < x + w and y <= py < y + h:
            return idx
    return -1


def main():
    args = sys.argv[1:]
    binary = args[0]
    json_out, show, cases_dir = None, 5, DEFAULT_CASES
    use_reference = False
    write_reference = False
    i = 1
    while i < len(args):
        if args[i] == "--json":
            json_out = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--dir":
            cases_dir = args[i + 1]
            i += 2
        elif args[i] == "--reference":
            use_reference = True
            i += 1
        elif args[i] == "--write-reference":
            write_reference = True
            i += 1
        else:
            i += 1

    ua = open(UA, encoding="utf-8").read()
    paths = sorted(glob.glob(os.path.join(cases_dir, "*.html")))
    if not paths:
        print("NO CASES in %s" % cases_dir)
        return 1
    names = [os.path.basename(p)[:-5] for p in paths]
    htmls = [open(p, encoding="utf-8").read() for p in paths]

    blocks = harness_mod.run_engine(binary, [(h, ua, "") for h in htmls])
    if len(blocks) != len(htmls):
        print("ENGINE returned %d blocks for %d cases" % (len(blocks),
                                                          len(htmls)))
        return 1
    parsed = [parse_engine(b) for b in blocks]

    sys.path.insert(0, HERE)
    import reference as ref_mod
    ref_head = None
    if use_reference and not write_reference:
        ref_head, data = ref_mod.load("stack")
        print("   %s" % ref_mod.describe(ref_head))
        jobs = []
        measured = {}
        for i, name in enumerate(names):
            entry = data.get(name)
            if entry is None:
                jobs.append((name, htmls[i], []))
                measured[name] = None
                continue
            jobs.append((name, htmls[i],
                         [(int(p[0]), int(p[1])) for p in entry["points"]]))
            measured[name] = {"boxes": entry["boxes"], "hits": entry["hits"]}
        missing = [n for n in names if data.get(n) is None]
        if missing:
            print("   NOT IN THE REFERENCE (%d): %s"
                  % (len(missing), ", ".join(missing[:5])))
    else:
        jobs = [(names[i], htmls[i], sample_points(parsed[i][0]))
                for i in range(len(names))]
        measured = measure(jobs)
        if write_reference:
            ref_head = ref_mod.header(
                "the topmost element at each probe point out of "
                "document.elementFromPoint()",
                LAST_EXE, LAST_WINDOW, (VIEWPORT_W, VIEWPORT_H))
            data = {}
            for i, name in enumerate(names):
                got = measured.get(name)
                if got is None:
                    continue
                data[name] = {"points": [list(p) for p in jobs[i][2]],
                              "boxes": got["boxes"], "hits": got["hits"]}
            out = ref_mod.save("stack", ref_head, data)
            print("   written: %s (%d cases)" % (out, len(data)))

    report = {"cases": [], "points_ok": 0, "points_total": 0}
    if ref_head is not None:
        report["reference"] = ref_head
    complaints = []
    for i, name in enumerate(names):
        rows, order = parsed[i]
        pts = jobs[i][2]
        got = measured.get(name)
        if got is None:
            complaints.append("STACK %s: not measured" % name)
            continue
        ok = 0
        bad = []
        for k, (px, py) in enumerate(pts):
            mine = top_at(rows, order, px, py)
            theirs = got["hits"][k]
            # Chromium reports the measuring script as an element too; it
            # is the last one and can never be hit (display: none).
            if mine == theirs:
                ok += 1
            elif mine == -1 and theirs == 0:
                # nothing of ours lies there and Chromium falls back to
                # the root element -- the same answer, said differently.
                ok += 1
            else:
                mt = rows[mine][0] if 0 <= mine < len(rows) else "-"
                tt = (got["boxes"][theirs][0]
                      if 0 <= theirs < len(got["boxes"]) else "-")
                bad.append("  (%d,%d): ours #%d <%s>, Chromium #%d <%s>"
                           % (px, py, mine, mt, theirs, tt))
        report["points_ok"] += ok
        report["points_total"] += len(pts)
        report["cases"].append({"name": name, "ok": ok, "total": len(pts),
                                "boxes": len(rows), "order": len(order)})
        if bad:
            complaints.append("STACK %s (%d/%d)\n%s"
                              % (name, ok, len(pts), "\n".join(bad[:6])))

    for c in complaints[:show]:
        print(c)
    if len(complaints) > show:
        print("... and %d more" % (len(complaints) - show))
    ok, total = report["points_ok"], report["points_total"]
    rate = 100.0 * (total - ok) / total if total else 100.0
    report["deviation_percent"] = rate
    print("")
    print("paint order against Chromium: %d / %d probe points equal in %d "
          "cases, deviation %.2f%%" % (ok, total, len(names), rate))
    if json_out:
        json.dump(report, open(json_out, "w"), indent=1)
    return 1 if ok != total else 0


if __name__ == "__main__":
    sys.exit(main())
