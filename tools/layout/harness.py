#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""The test bench of round 61: OWN cases, OWN expectation, FOREIGN witness.

A case is a plain HTML file in `tools/layout/cases/`. It is run twice:

  * through the Firn layout engine, which prints one `B` line per element
    with the border box in viewport coordinates
  * through Chromium in headless mode, which is asked the same question
    with `getBoundingClientRect()`

and it is held against the file `<case>.expected`, which contains the
boxes WRITTEN OUT BY HAND from the standard -- one line per element:

    <tag> <x> <y> <width> <height>

Both comparisons are reported separately and neither replaces the other.
The expectation catches the case where the engine and the browser are
wrong in the same way; the browser catches the case where the expectation
was wrong -- which happens more often, because a number thought up by the
author of the code is not a measurement.

ROUND 78: the browser side is FROZEN by default. `--reference` reads the
boxes out of `tools/layout/reference/cases.json` instead of starting a
browser, so the acceptance needs no foreign program; `--write-reference`
measures live once and writes that file. `--no-chrome` still means: do not
compare against the browser at all.

Usage:
    python3 tools/layout/harness.py <binary> [--json out.json] [--show N]
                                   [--no-chrome] [--reference]
                                   [--write-reference] [--write-expected]
                                   [--only NAME]
"""

import glob
import json
import os
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
CASES = os.path.join(HERE, "cases")
UA = os.path.join(HERE, "ua.css")

VIEWPORT_W = 800
VIEWPORT_H = 600

# A browser computes in LayoutUnit, which is 1/64 px. Anything below that
# cannot be a real difference of the algorithm, only of the arithmetic.
TOLERANCE = 1.0 / 64.0


def encode_job(html, ua, author, vw=VIEWPORT_W, vh=VIEWPORT_H):
    def blob(text):
        raw = text.encode("utf-8")
        return struct.pack("<I", len(raw)) + raw
    return (struct.pack("<II", vw, vh) + blob(html) + blob(ua) + blob(author))


def run_engine(binary, jobs):
    payload = b"".join(encode_job(h, u, a) for h, u, a in jobs)
    out = subprocess.run([binary], input=payload, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, timeout=600)
    if out.returncode != 0:
        raise RuntimeError("engine exited with %d: %s"
                           % (out.returncode, out.stderr[:400]))
    blocks = []
    current = []
    for line in out.stdout.decode("utf-8", "replace").split("\n"):
        if line == "#END":
            blocks.append(current)
            current = []
        elif line:
            current.append(line)
    return blocks


def parse_block(lines):
    """-> list of (tag, x, y, w, h) in document order."""
    tags = {}
    boxes = {}
    for line in lines:
        if line.startswith("E "):
            p = line.split(" ")
            tags[int(p[1])] = p[3]
        elif line.startswith("B "):
            p = line.split(" ")
            boxes[int(p[1])] = tuple(float(v) for v in p[2:6])
    out = []
    for i in sorted(tags):
        b = boxes.get(i, (0.0, 0.0, 0.0, 0.0))
        out.append((tags[i], b[0], b[1], b[2], b[3]))
    return out


def load_expected(path):
    rows = []
    if not os.path.exists(path):
        return None
    for line in open(path, encoding="utf-8"):
        line = line.split("#")[0].strip()
        if not line:
            continue
        p = line.split()
        rows.append((p[0], float(p[1]), float(p[2]), float(p[3]), float(p[4])))
    return rows


def fmt_rows(rows):
    return "".join("%-8s %10.4f %10.4f %10.4f %10.4f\n" % r for r in rows)


def compare(got, want):
    """-> (matching boxes, total boxes, list of complaints)."""
    bad = []
    n = max(len(got), len(want))
    ok = 0
    for i in range(n):
        g = got[i] if i < len(got) else None
        e = want[i] if i < len(want) else None
        if g is None or e is None:
            bad.append("  #%d: %s vs %s" % (i, g, e))
            continue
        if g[0] != e[0]:
            bad.append("  #%d: tag %s vs %s" % (i, g[0], e[0]))
            continue
        d = [abs(g[k] - e[k]) for k in range(1, 5)]
        if max(d) <= TOLERANCE:
            ok += 1
        else:
            bad.append("  #%d <%s> got  x=%.4f y=%.4f w=%.4f h=%.4f\n"
                       "  #%d <%s> want x=%.4f y=%.4f w=%.4f h=%.4f  (d=%.4f)"
                       % (i, g[0], g[1], g[2], g[3], g[4],
                          i, e[0], e[1], e[2], e[3], e[4], max(d)))
    return ok, n, bad


def main():
    args = sys.argv[1:]
    binary = args[0]
    json_out = None
    show = 3
    use_chrome = True
    use_reference = False
    write_reference = False
    write_expected = False
    only = None
    i = 1
    while i < len(args):
        if args[i] == "--json":
            json_out = args[i + 1]
            i += 2
        elif args[i] == "--show":
            show = int(args[i + 1])
            i += 2
        elif args[i] == "--no-chrome":
            use_chrome = False
            i += 1
        elif args[i] == "--reference":
            use_reference = True
            i += 1
        elif args[i] == "--write-reference":
            write_reference = True
            i += 1
        elif args[i] == "--write-expected":
            write_expected = True
            i += 1
        elif args[i] == "--only":
            only = args[i + 1]
            i += 2
        else:
            i += 1

    ua = open(UA, encoding="utf-8").read()
    paths = sorted(glob.glob(os.path.join(CASES, "*.html")))
    if only:
        paths = [p for p in paths if only in os.path.basename(p)]
    if not paths:
        print("NO CASES in %s" % CASES)
        return 1

    names = [os.path.basename(p)[:-5] for p in paths]
    htmls = [open(p, encoding="utf-8").read() for p in paths]
    jobs = [(h, ua, "") for h in htmls]
    blocks = run_engine(binary, jobs)
    if len(blocks) != len(jobs):
        print("ENGINE returned %d blocks for %d cases" % (len(blocks), len(jobs)))
        return 1
    got = [parse_block(b) for b in blocks]

    chrome = {}
    ref_head = None
    if use_chrome:
        sys.path.insert(0, HERE)
        import reference as ref_mod
        if use_reference and not write_reference:
            # NO browser, NO network: the answer Chromium gave once, read
            # out of the repository. A missing file raises -- a reference
            # that is not there has to be a failure, not a skipped section.
            ref_head, data = ref_mod.load("cases")
            print("   %s" % ref_mod.describe(ref_head))
            measured = dict((n, data.get(n)) for n in names)
            missing = [n for n in names if data.get(n) is None]
            if missing:
                print("   NOT IN THE REFERENCE (%d): %s"
                      % (len(missing), ", ".join(missing[:5])))
        else:
            import chrome as chrome_mod
            measured = chrome_mod.measure_many(list(zip(names, htmls)))
            if write_reference:
                ref_head = ref_mod.header(
                    "the boxes of tools/layout/cases/*.html out of "
                    "getBoundingClientRect()",
                    chrome_mod.LAST_EXE, chrome_mod.LAST_WINDOW, (800, 600))
                out = ref_mod.save("cases", ref_head, measured)
                print("   written: %s (%d cases)" % (out, len(measured)))
        for name, rows in measured.items():
            if rows is None:
                chrome[name] = None
            else:
                chrome[name] = [(r[0], r[1], r[2], r[3], r[4]) for r in rows
                                if r[0] != "script" or True]

    report = {"cases": [], "expected_ok": 0, "expected_total": 0,
              "chrome_ok": 0, "chrome_total": 0, "chrome_cases_run": 0}
    if ref_head is not None:
        report["reference"] = ref_head
    complaints = []
    for idx, name in enumerate(names):
        exp_path = os.path.join(CASES, name + ".expected")
        if write_expected:
            open(exp_path, "w", encoding="utf-8").write(fmt_rows(got[idx]))
        want = load_expected(exp_path)
        entry = {"name": name, "boxes": len(got[idx])}
        if want is None:
            entry["expected"] = "missing"
        else:
            ok, n, bad = compare(got[idx], want)
            report["expected_ok"] += ok
            report["expected_total"] += n
            entry["expected_ok"] = ok
            entry["expected_total"] = n
            if bad:
                complaints.append("EXPECTED %s (%d/%d)\n%s"
                                  % (name, ok, n, "\n".join(bad[:6])))
        if use_chrome:
            rows = chrome.get(name)
            if rows is None:
                entry["chrome"] = "not measured"
            else:
                # Chromium also reports the measuring script the harness
                # appends. It is the LAST element and is dropped.
                rows = [r for r in rows]
                if rows and rows[-1][0] == "script":
                    rows = rows[:-1]
                ok2, n2, bad2 = compare(got[idx], rows)
                report["chrome_ok"] += ok2
                report["chrome_total"] += n2
                report["chrome_cases_run"] += 1
                entry["chrome_ok"] = ok2
                entry["chrome_total"] = n2
                entry["chrome_deviation"] = (n2 - ok2)
                if bad2:
                    complaints.append("CHROMIUM %s (%d/%d)\n%s"
                                      % (name, ok2, n2, "\n".join(bad2[:6])))
        report["cases"].append(entry)

    for c in complaints[:show]:
        print(c)
    if len(complaints) > show:
        print("... and %d more" % (len(complaints) - show))

    e_ok = report["expected_ok"]
    e_n = report["expected_total"]
    c_ok = report["chrome_ok"]
    c_n = report["chrome_total"]
    print("")
    print("own expectation : %d / %d boxes in %d cases" % (e_ok, e_n, len(names)))
    if use_chrome and c_n:
        rate = 100.0 * (c_n - c_ok) / c_n
        print("against Chromium: %d / %d boxes equal, deviation %.2f%%"
              % (c_ok, c_n, rate))
        report["chrome_deviation_percent"] = rate
        worst = sorted([e for e in report["cases"] if "chrome_deviation" in e],
                       key=lambda e: -e["chrome_deviation"])
        for e in worst[:5]:
            if e["chrome_deviation"]:
                print("   worst: %-28s %d of %d boxes off"
                      % (e["name"], e["chrome_deviation"], e["chrome_total"]))
    if json_out:
        json.dump(report, open(json_out, "w"), indent=1)
    fail = (e_n - e_ok) + (c_n - c_ok if use_chrome else 0)
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
