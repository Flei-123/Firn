import json
import os
import re
import sys

POISON_RE = re.compile(r"use after free detected|exit 86|rc 86|\(exit 86\)|status 86")
FAIL_RE = re.compile(r"^\s*(FAIL|DIFF|MISMATCH)\b.*")
RESULT_RE = re.compile(r"^RESULT (\S+) (\S+) (\S+)")


def read_step(out, name):
    log = os.path.join(out, name + ".log")
    rcp = os.path.join(out, name + ".rc")
    text = open(log, errors="replace").read() if os.path.exists(log) else ""
    rc = int(open(rcp).read().strip()) if os.path.exists(rcp) else None
    lines = text.splitlines()
    fails = sorted({l.strip() for l in lines if FAIL_RE.match(l)})
    poison = sorted({l.strip() for l in lines if POISON_RE.search(l)})
    results = {}
    for l in lines:
        m = RESULT_RE.match(l)
        if m:
            results[m.group(1)] = m.group(2) + " " + m.group(3)
    summary = ""
    for l in lines:
        if re.match(r"^(PASS|FAIL) ", l) or l.startswith("ownership:"):
            summary = l.strip()
    return {"step": name, "rc": rc, "summary": summary, "fails": fails, "poison": poison, "results": results}


def main():
    out = sys.argv[1]
    base = sys.argv[2] if len(sys.argv) > 2 else None
    names = sorted(f[:-4] for f in os.listdir(out) if f.endswith(".rc"))
    steps = [read_step(out, n) for n in names]
    base_steps = {}
    if base:
        base_steps = {s["step"]: s for s in (read_step(base, n) for n in sorted(f[:-4] for f in os.listdir(base) if f.endswith(".rc")))}
    report = {"out": out, "base": base, "steps": []}
    for s in steps:
        b = base_steps.get(s["step"])
        if b is not None:
            s["new_fails"] = [f for f in s["fails"] if f not in b["fails"]]
            s["output_diffs"] = {k: [b["results"].get(k), v] for k, v in s["results"].items() if b["results"].get(k) != v}
            s["base_rc"] = b["rc"]
        else:
            s["new_fails"] = s["fails"]
            s["output_diffs"] = {}
        report["steps"].append(s)
    json.dump(report, open(os.path.join(out, "results.json"), "w"), indent=1)
    with open(os.path.join(out, "results.tsv"), "w") as f:
        f.write("step\trc\tbase_rc\tfails\tnew_fails\tpoison\toutput_diffs\tsummary\n")
        for s in report["steps"]:
            f.write("%s\t%s\t%s\t%d\t%d\t%d\t%d\t%s\n" % (s["step"], s["rc"], s.get("base_rc", "-"), len(s["fails"]), len(s["new_fails"]), len(s["poison"]), len(s["output_diffs"]), s["summary"]))
    total_poison = sum(len(s["poison"]) for s in report["steps"])
    print("step           rc  fails new poison outdiff  summary")
    for s in report["steps"]:
        print("%-14s %-3s %5d %3d %6d %7d  %s" % (s["step"], s["rc"], len(s["fails"]), len(s["new_fails"]), len(s["poison"]), len(s["output_diffs"]), s["summary"]))
    for s in report["steps"]:
        for p in s["poison"]:
            print("POISON %s: %s" % (s["step"], p))
        for p in s["new_fails"]:
            print("NEWFAIL %s: %s" % (s["step"], p))
        for k, v in s["output_diffs"].items():
            print("OUTDIFF %s: %s base=%s now=%s" % (s["step"], k, v[0], v[1]))
    sys.exit(1 if total_poison else 0)


main()
