#!/usr/bin/env python3
"""Merge examples/vs_python/**/results_*.tsv into one Markdown table.
Per task: first pass (BEFORE_/_before_ row, else the earliest row) and latest row
(chronological order of the result files = order of the workers' rounds).
Usage: table.py [--md | --json | --stats]"""
import glob, json, os, re, statistics, sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CHRONO = ["net", "data", "sys", "media", "lang", "data2", "media2", "lib2", "net2", "text", "fnc", "text2", "meas2", "r737"]


def rank(path):
    m = re.search(r"results_vspy-(\w+)\.tsv", path)
    k = m.group(1) if m else ""
    return CHRONO.index(k) if k in CHRONO else 99


def canon(name):
    n = re.sub(r"^(BEFORE_|AFTER_)", "", name)
    n = re.sub(r"_(before|after)_\w+$", "", n)
    return n, ("before" if name.startswith("BEFORE_") or "_before_" in name else
               "after" if name.startswith("AFTER_") or "_after_" in name else "")


rows = {}
for f in sorted(glob.glob(os.path.join(ROOT, "examples/vs_python/**/results_*.tsv"), recursive=True), key=rank):
    for ln in open(f, encoding="utf-8"):
        p = ln.rstrip("\n").split("\t")
        if len(p) < 12 or p[0] == "name" or p[0] == "task":
            continue
        n, kind = canon(p[0])
        if not re.match(r"^(\d\d_|p\d\d_)", n):
            continue
        rows.setdefault(n, []).append((kind, p))
out = []
for n in sorted(rows):
    v = rows[n]
    before = [p for k, p in v if k == "before"]
    now = [p for k, p in v if k != "before"]
    first = before[0] if before else v[0][1]
    last = now[-1] if now else v[-1][1]
    out.append(dict(task=n, py=last[1], first=first[2], now=last[2], x=last[3], py_tok=last[4], fi_tok=last[5],
                    x_tok=last[6], py_imp=last[7], fi_imp=last[8], py_ms=last[9], fi_ms=last[10], comp_ms=last[11]))
if "--json" in sys.argv:
    print(json.dumps(out, indent=1))
elif "--stats" in sys.argv:
    t = [r for r in out if r["task"][0].isdigit()]
    xs = [float(r["x"]) for r in t]
    xf = [float(r["first"]) / float(r["py"]) for r in t]
    print(f"tasks={len(t)} median_factor_now={statistics.median(xs):.2f} median_factor_first={statistics.median(xf):.2f} "
          f"le1.5_now={sum(1 for x in xs if x <= 1.5)} le1.5_first={sum(1 for x in xf if x <= 1.5)}")
else:
    print("| Task | Py lines | Firn lines first pass | Firn lines now | Factor now | Py tokens | Firn tokens | Factor | Py imports | Firn imports | Py run ms | Firn run ms | Firn compile ms |")
    print("|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|")
    for r in out:
        print("| " + " | ".join(r.values()) + " |")
