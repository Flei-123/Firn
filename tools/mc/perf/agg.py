#!/usr/bin/env python3
"""agg.py <out-prefix> <meta.json> <run.txt>...: median + spread of `RESULT key value unit` lines over several runs -> .tsv/.json"""
import json, statistics, sys
out, meta = sys.argv[1], json.load(open(sys.argv[2]))
runs = []
for f in sys.argv[3:]:
    d = {}
    for ln in open(f):
        p = ln.split()
        if len(p) >= 4 and p[0] == "RESULT":
            try: d[p[1]] = (float(p[2]), p[3])
            except ValueError: d[p[1]] = (p[2], p[3])
    runs.append(d)
keys = [k for k in runs[0] if all(k in r for r in runs)] if runs else []
rows = []
for k in keys:
    vals = [r[k][0] for r in runs]
    if all(isinstance(v, float) for v in vals):
        rows.append((k, statistics.median(vals), min(vals), max(vals), runs[0][k][1], len(vals)))
    else:
        rows.append((k, vals[0], "", "", runs[0][k][1], len(vals)))
with open(out + ".tsv", "w") as t:
    t.write("metric\tmedian\tmin\tmax\tunit\truns\n")
    for r in rows: t.write("\t".join(str(x) for x in r) + "\n")
json.dump({"meta": meta, "results": {r[0]: {"median": r[1], "min": r[2], "max": r[3], "unit": r[4], "runs": r[5]} for r in rows}},
          open(out + ".json", "w"), indent=1)
print(f"wrote {out}.tsv / .json ({len(rows)} metrics, {len(runs)} runs)")
