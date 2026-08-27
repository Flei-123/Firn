#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""The cross-check on REAL pages -- the number that cannot be talked up.

The cases in `tools/layout/cases/` are written by the same person who
wrote the engine, and they test what that person thought of. The eight
documents in `testdata/realweb/` were not: Wikipedia, Hacker News, the
HTML specification, rustdoc. Tens of thousands of boxes, nested twenty
deep, with every construction the case files avoid.

To make the comparison FAIR and not a measurement of font files, both
sides get the same preparation:

  * every `<script>` is removed -- a page that rewrites itself while it is
    being measured measures nothing
  * every `<link rel=stylesheet>` is removed and no request leaves the
    machine (`--host-resolver-rules`), so both engines see exactly the
    stylesheets that stand IN the document
  * one stylesheet is prepended that puts the measuring font on
    everything, so that the width of a letter is the same on both sides

What is left is a comparison of the LAYOUT, and every deviation is one.

ROUND 78: the browser side is FROZEN in
`tools/layout/reference/realweb.json` (`--reference`, the default of
tools/layout/run.sh). `--write-reference` asks a live browser once and
writes the file. The pages do not change, the browser's answer to them
does not change either -- what changes is the Firn engine, and that is the
only thing this number is supposed to be about.
"""

import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WEB = os.path.join(ROOT, "testdata", "realweb")

FORCE = """<style>
@font-face { font-family: FirnMetric; src: url(FirnMetric.ttf) }
* { font-family: FirnMetric !important }
</style>"""

SCRIPT = re.compile(r"<script\b.*?</script\s*>", re.S | re.I)
SCRIPT_SHORT = re.compile(r"<script\b[^>]*/?>", re.I)
LINK = re.compile(r"<link\b[^>]*>", re.I)
NOSCRIPT = re.compile(r"</?noscript\s*>", re.I)


def prepare(text):
    text = SCRIPT.sub("", text)
    text = SCRIPT_SHORT.sub("", text)
    text = LINK.sub("", text)
    text = NOSCRIPT.sub("", text)
    # The forced stylesheet goes in FRONT of everything, so that the rules
    # of the page can still beat it where they are more specific -- only
    # the font family is forced with `!important`.
    low = text.lower()
    at = low.find("<head")
    if at >= 0:
        at = text.find(">", at) + 1
        return text[:at] + FORCE + text[at:]
    return FORCE + text


def distances(got, want):
    out = []
    for i in range(min(len(got), len(want))):
        g, e = got[i], want[i]
        if g[0] != e[0]:
            out.append(float("inf"))
        else:
            out.append(max(abs(g[k] - e[k]) for k in range(1, 5)))
    for _i in range(abs(len(got) - len(want))):
        out.append(float("inf"))
    return out


def median(v):
    if not v:
        return 0.0
    v = sorted(x for x in v if x != float("inf"))
    if not v:
        return float("inf")
    return v[len(v) // 2]


def percentile(v, p):
    v = sorted(x for x in v if x != float("inf"))
    if not v:
        return float("inf")
    return v[min(len(v) - 1, int(len(v) * p / 100.0))]


def main():
    binary = sys.argv[1]
    limit = None
    json_out = None
    use_reference = False
    write_reference = False
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--json":
            json_out = args[i + 1]
            i += 2
        elif args[i] == "--limit":
            limit = int(args[i + 1])
            i += 2
        elif args[i] == "--reference":
            use_reference = True
            i += 1
        elif args[i] == "--write-reference":
            write_reference = True
            i += 1
        else:
            i += 1

    sys.path.insert(0, HERE)
    import harness as H
    import chrome as C

    ua = open(os.path.join(HERE, "ua.css"), encoding="utf-8").read()
    names = sorted(f for f in os.listdir(WEB) if f.endswith(".html"))
    if limit:
        names = names[:limit]
    pages = []
    for n in names:
        raw = open(os.path.join(WEB, n), encoding="utf-8", errors="replace").read()
        pages.append((n, prepare(raw)))

    blocks = H.run_engine(binary, [(t, ua, "") for _n, t in pages])
    got = [H.parse_block(b) for b in blocks]

    import reference as R
    ref_head = None
    if use_reference and not write_reference:
        ref_head, data = R.load("realweb")
        print(R.describe(ref_head))
        measured = dict((n, data.get(n)) for n, _t in pages)
    else:
        measured = C.measure_many(pages, timeout=180)
        if write_reference:
            ref_head = R.header(
                "the boxes of testdata/realweb/*.html out of "
                "getBoundingClientRect(), scripts and <link> removed",
                C.LAST_EXE, C.LAST_WINDOW, (800, 600))
            out = R.save("realweb", ref_head,
                         dict((n, v) for n, v in measured.items()
                              if v is not None))
            print("written: %s (%d pages)" % (out, len(measured)))

    total = 0
    equal = 0
    deviations = []
    report = {"pages": []}
    if ref_head is not None:
        report["reference"] = ref_head
    for idx, (name, _t) in enumerate(pages):
        rows = measured.get(name)
        if rows is None:
            report["pages"].append({"name": name, "state": "not measured"})
            continue
        rows = [tuple(r) for r in rows]
        if rows and rows[-1][0] == "script":
            rows = rows[:-1]
        ok, n, _bad = H.compare(got[idx], rows)
        total += n
        equal += ok
        d = distances(got[idx], rows)
        deviations.extend(d)
        report["pages"].append({"name": name, "boxes": n, "equal": ok,
                                "deviation": n - ok,
                                "median_px": median(d),
                                "p90_px": percentile(d, 90),
                                "percent": 100.0 * (n - ok) / max(n, 1)})
        print("%-28s %6d boxes  %6d exact  %6.2f %% off   median %6.2f px  p90 %8.2f px"
              % (name, n, ok, 100.0 * (n - ok) / max(n, 1),
                 median(d), percentile(d, 90)))
    rate = 100.0 * (total - equal) / max(total, 1)
    report["total"] = total
    report["equal"] = equal
    report["deviation_percent"] = rate
    report["median_px"] = median(deviations)
    report["p90_px"] = percentile(deviations, 90)
    print("")
    print("REALWEB %d / %d boxes to the bit, deviation %.2f %%"
          % (equal, total, rate))
    # A binary quota says nothing about HOW far off a page is. On a
    # document of twenty thousand pixels one wrong box at the top moves
    # everything below it, and the difference is two pixels, not two
    # hundred. So: the distribution.
    for edge in (0.5, 1.0, 2.0, 4.0, 16.0, 64.0):
        within = sum(1 for v in deviations if v <= edge)
        print("   within %5.1f px : %6d of %6d boxes  (%5.2f %%)"
              % (edge, within, len(deviations),
                 100.0 * within / max(len(deviations), 1)))
        report["within_%g" % edge] = within
    print("   median %.2f px, 90th percentile %.2f px"
          % (median(deviations), percentile(deviations, 90)))
    if json_out:
        json.dump(report, open(json_out, "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
