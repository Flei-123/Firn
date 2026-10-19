#!/usr/bin/env python3
"""Firn vs C benchmark: identical algorithms, same machine, median of N runs.

Columns: Firn release-fast / release-safe / dev-fast, gcc -O2, gcc -O3, clang -O2.
Every program prints exactly one line (a checksum); all builds must agree,
otherwise the program is reported as MISMATCH and gets no timing.
Time = user+sys CPU seconds of the child (rusage), robust on a loaded host.

  /root/jarvis/bin/heavy bash bench/firn_vs_c/run.sh      (see run.sh)
  BENCH_RUNS=5 ONLY=qsort,fib  FIRNC_BIN=...  OUT=results.json
"""
import json, os, shutil, statistics, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
RUNS = int(os.environ.get("BENCH_RUNS", "5"))
ONLY = [x for x in os.environ.get("ONLY", "").split(",") if x]
OUT = os.environ.get("OUT", os.path.join(HERE, "results.json"))
TIMEOUT = int(os.environ.get("BENCH_TIMEOUT", "120"))

COLS = [
    ("firn_fast", "Firn release-fast", "firn", ["--opt-level=release-fast"]),
    ("firn_safe", "Firn release-safe", "firn", ["--opt-level=release-safe"]),
    ("firn_dev", "Firn dev-fast", "firn", ["--opt-level=dev-fast"]),
    ("gcc_o2", "gcc -O2", "gcc", ["-O2"]),
    ("gcc_o3", "gcc -O3", "gcc", ["-O3", "-march=x86-64"]),
    ("clang_o2", "clang -O2", "clang", ["-O2"]),
]


def build(kind, flags, src, out):
    if kind == "firn":
        cmd = [FIRNC] + flags + [src, "-o", out]
    elif kind == "gcc":
        cmd = ["gcc"] + flags + ["-fno-strict-aliasing", src, "-o", out, "-lm"]
    else:
        cmd = ["clang"] + flags + ["-fno-strict-aliasing", src, "-o", out, "-lm"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.returncode == 0, (r.stderr or r.stdout)[-300:]


def run_once(binary):
    t0 = time.perf_counter()
    p = subprocess.Popen([binary], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        out, _ = p.communicate(timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        p.kill()
        return None, "TIMEOUT"
    wall = time.perf_counter() - t0
    return wall, out.decode().strip()


def run_cpu(binary):
    # fork+exec via resource usage of children
    import resource
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    wall, out = run_once(binary)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if wall is None:
        return None, out
    cpu = (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    return cpu, out


def main():
    names = sorted(f[:-3] for f in os.listdir(os.path.join(HERE, "firn")) if f.endswith(".fi"))
    names = [n for n in names if os.path.exists(os.path.join(HERE, "c", n + ".c"))]
    if ONLY:
        names = [n for n in names if n in ONLY]
    work = tempfile.mkdtemp(prefix="firnvsc.")
    res = {"runs": RUNS, "cpu": open("/proc/cpuinfo").read().split("model name")[1].split(":")[1].split("\n")[0].strip(),
           "firnc": subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip(),
           "gcc": subprocess.run(["gcc", "-dumpfullversion"], capture_output=True, text=True).stdout.strip(),
           "load": open("/proc/loadavg").read().split()[:3], "benchmarks": {}}
    try:
        for n in names:
            entry = {"sum": None, "t": {}, "err": {}}
            sums = {}
            bins = {}
            for key, title, kind, flags in COLS:
                src = os.path.join(HERE, "firn" if kind == "firn" else "c", n + (".fi" if kind == "firn" else ".c"))
                out = os.path.join(work, n + "." + key)
                ok, msg = build(kind, flags, src, out)
                if not ok:
                    entry["err"][key] = "build: " + msg
                    continue
                bins[key] = out
            # interleave runs across columns to even out host noise
            samples = {k: [] for k in bins}
            for r in range(RUNS + 1):
                for k, b in bins.items():
                    cpu, out = run_cpu(b)
                    if cpu is None:
                        entry["err"][k] = out
                        continue
                    sums.setdefault(k, out)
                    if sums[k] != out:
                        entry["err"][k] = "unstable output"
                    if r > 0:
                        samples[k].append(cpu)
            vals = set(sums.values())
            entry["sums"] = sums
            if len(vals) == 1:
                entry["sum"] = vals.pop()
            else:
                entry["err"]["_"] = "MISMATCH %r" % sums
            for k, s in samples.items():
                if s and "_" not in entry["err"] and k not in entry["err"]:
                    entry["t"][k] = {"median": statistics.median(s), "min": min(s), "max": max(s)}
            res["benchmarks"][n] = entry
            print(n, entry["sum"], {k: round(v["median"], 3) for k, v in entry["t"].items()}, entry["err"] or "", flush=True)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    with open(OUT, "w") as f:
        json.dump(res, f, indent=1)
    write_md(res, OUT.replace(".json", ".md"))


def geo(xs):
    import math
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def write_md(res, path):
    lines = ["| benchmark | Firn fast | Firn safe | Firn dev | gcc -O2 | gcc -O3 | clang -O2 | fast/O2 | fast/O3 | safe/O2 |", "|---|---|---|---|---|---|---|---|---|---|"]
    f2, f3, s2 = [], [], []
    for n, e in res["benchmarks"].items():
        t = e["t"]
        def g(k):
            return t[k]["median"] if k in t else None
        def fmt(v):
            return "%.3f s" % v if v is not None else "-"
        def ratio(a, b):
            return a / b if a is not None and b else None
        r1, r2, r3 = ratio(g("firn_fast"), g("gcc_o2")), ratio(g("firn_fast"), g("gcc_o3")), ratio(g("firn_safe"), g("gcc_o2"))
        for r, l in ((r1, f2), (r2, f3), (r3, s2)):
            if r:
                l.append(r)
        fr = lambda r: "%.2fx" % r if r else "-"
        lines.append("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (n, fmt(g("firn_fast")), fmt(g("firn_safe")), fmt(g("firn_dev")), fmt(g("gcc_o2")), fmt(g("gcc_o3")), fmt(g("clang_o2")), fr(r1), fr(r2), fr(r3)))
    lines.append("")
    lines.append("Geomean Firn release-fast / gcc -O2: %.2fx; / gcc -O3: %.2fx; Firn release-safe / gcc -O2: %.2fx (n=%d)" % (geo(f2), geo(f3), geo(s2), len(f2)))
    lines.append("CPU: %s, firn %s, gcc %s, load at start %s, %d runs (median of user+sys)" % (res["cpu"], res["firnc"], res["gcc"], " ".join(res["load"]), res["runs"]))
    open(path, "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
