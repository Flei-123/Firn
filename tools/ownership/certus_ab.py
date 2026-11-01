#!/usr/bin/env python3
import argparse
import glob
import json
import math
import os
import re
import statistics
import struct
import subprocess
import sys
import tempfile
import time

OCTANE = [
    "octane-box2d", "octane-crypto", "octane-deltablue", "octane-earley-boyer",
    "octane-gbemu", "octane-navier-stokes", "octane-raytrace", "octane-regexp",
    "octane-richards", "octane-splay", "octane-typescript",
]
FREED_MARK = "use after free"
STATE_WORDS = {"runs": 4, "pause_max": 9, "pause_total": 10, "allocs": 13}


def job_names(job_dir):
    names = sorted(os.path.basename(p)[:-3] for p in glob.glob(os.path.join(job_dir, "banks-*.js")))
    names += sorted(os.path.basename(p)[:-3] for p in glob.glob(os.path.join(job_dir, "sunspider-*.js")))
    names += OCTANE
    return names


def load_job(job_dir, name):
    src = open(os.path.join(job_dir, name + ".js"), "rb").read()
    return struct.pack("<II", 0, len(src)) + src


def parse_result(out):
    for line in out.splitlines():
        if line.startswith("@@"):
            try:
                return json.loads(line[2:])
            except ValueError:
                return {"error": "bad json"}
    return {"error": "no result"}


def timed_run(binary, payload, timeout):
    with tempfile.TemporaryDirectory() as d:
        stat = os.path.join(d, "stat")
        inp = os.path.join(d, "in")
        open(inp, "wb").write(payload)
        t0 = time.time()
        try:
            with open(inp, "rb") as fin:
                p = subprocess.run(
                    ["/usr/bin/time", "-f", "%U %S %M %e", "-o", stat, binary],
                    stdin=fin, capture_output=True, timeout=timeout)
            rc = p.returncode
            out = p.stdout.decode(errors="replace")
            err = p.stderr.decode(errors="replace")
        except subprocess.TimeoutExpired:
            rc, out, err = 124, "", "timeout"
        wall = time.time() - t0
        user = sys_t = 0.0
        rss = 0
        try:
            parts = open(stat).read().split()[-4:]
            user, sys_t, rss = float(parts[0]), float(parts[1]), int(parts[2])
        except (OSError, ValueError, IndexError):
            pass
    res = parse_result(out)
    return {
        "rc": rc, "user": user, "sys": sys_t, "cpu": round(user + sys_t, 3), "rss_kb": rss,
        "wall": round(wall, 3), "load1": round(os.getloadavg()[0], 1),
        "error": res.get("error"), "score": res.get("score") or res.get("median_ms"),
        "poison": FREED_MARK in err, "err_tail": err[-160:] if rc not in (0,) else "",
    }


def gdb_state(binary, job_file):
    sym = None
    nm = subprocess.run(["nm", binary], capture_output=True, text=True).stdout
    for line in nm.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2].endswith(".gc_init"):
            sym = parts[2]
            break
    if sym is None:
        return {}
    dis = subprocess.run(["objdump", "-d", "--no-show-raw-insn", "--disassemble=" + sym, binary],
                         capture_output=True, text=True).stdout
    m = re.search(r"lea +0x[0-9a-f]+\(%rip\).*# +([0-9a-f]+)", dis)
    if not m:
        return {}
    addr = "0x" + m.group(1)
    cmd = ["gdb", "-batch", "-q", "-ex", "set pagination off", "-ex", "catch syscall exit exit_group",
           "-ex", "run < " + job_file, "-ex", "x/14gu " + addr, "-ex", "x/1gu %s+296" % addr, binary]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    except subprocess.TimeoutExpired:
        return {}
    wall = time.time() - t0
    words = []
    total = 0
    for line in p.stdout.splitlines():
        if re.match(r"^0x[0-9a-f]+", line):
            vals = [int(x) for x in line.split(":", 1)[1].split()]
            if len(words) < 14:
                words.extend(vals)
            else:
                total = vals[0]
    if len(words) < 14:
        return {}
    r = {k: words[i] for k, i in STATE_WORDS.items()}
    r["alloc_bytes"] = total
    r["gdb_wall"] = round(wall, 3)
    return r


def median(xs):
    return statistics.median(xs) if xs else float("nan")


def geomean(xs):
    xs = [x for x in xs if x > 0 and not math.isnan(x)]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def load_rows(path):
    rows = []
    if os.path.exists(path):
        for line in open(path):
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def report(rows, modes, stats_rows):
    jobs = []
    for r in rows:
        if r["job"] not in jobs:
            jobs.append(r["job"])
    lines = []
    head = "%-34s" % "job" + "".join("%9s" % (m + " cpu") for m in modes)
    head += "".join("%9s" % (m + "/" + modes[0]) for m in modes[1:])
    head += "".join("%10s" % (m + " rss") for m in modes[1:]) + "  flags"
    lines.append(head)
    ratios = {m: [] for m in modes[1:]}
    rss_ratios = {m: [] for m in modes[1:]}
    poison_total = 0
    bad_total = 0
    for j in jobs:
        med = {}
        rss = {}
        flags = []
        for m in modes:
            rs = [r for r in rows if r["job"] == j and r["mode"] == m]
            ok = [r for r in rs if r["rc"] == 0 and not r["error"]]
            med[m] = median([r["cpu"] for r in ok])
            rss[m] = median([r["rss_kb"] for r in ok])
            for r in rs:
                if r["poison"] or r["rc"] == 86:
                    poison_total += 1
                    flags.append("%s:POISON" % m)
                elif r["rc"] != 0 or r["error"]:
                    bad_total += 1
                    flags.append("%s:rc%d" % (m, r["rc"]))
        line = "%-34s" % j + "".join("%9.2f" % med[m] for m in modes)
        for m in modes[1:]:
            q = med[m] / med[modes[0]] if med[modes[0]] > 0 else float("nan")
            ratios[m].append(q)
            line += "%9.3f" % q
        for m in modes[1:]:
            q = rss[m] / rss[modes[0]] if rss[modes[0]] > 0 else float("nan")
            rss_ratios[m].append(q)
            line += "%10.3f" % q
        line += "  " + ",".join(sorted(set(flags)))
        lines.append(line)
    lines.append("")
    for m in modes[1:]:
        lines.append("geomean cpu %s/%s: %.4f   geomean maxrss %s/%s: %.4f" % (
            m, modes[0], geomean(ratios[m]), m, modes[0], geomean(rss_ratios[m])))
    lines.append("poison hits (exit 86 / use after free): %d   other failures: %d" % (poison_total, bad_total))
    if stats_rows:
        lines.append("")
        lines.append("%-34s %-6s %8s %12s %12s %10s %8s" % ("job", "mode", "cycles", "pause_tot_ms", "alloc_count", "alloc_MB", "gc_share"))
        shares = {m: [] for m in modes}
        for s in stats_rows:
            share = (s["pause_total"] / 1e9) / s["gdb_wall"] if s.get("gdb_wall") else float("nan")
            shares[s["mode"]].append(share)
            lines.append("%-34s %-6s %8d %12.1f %12d %10.1f %7.1f%%" % (
                s["job"], s["mode"], s["runs"], s["pause_total"] / 1e6, s["allocs"], s["alloc_bytes"] / 1048576.0, share * 100))
        for m in modes:
            if shares[m]:
                lines.append("mean gc_share %s: %.2f%%" % (m, 100 * sum(x for x in shares[m] if not math.isnan(x)) / max(1, len(shares[m]))))
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", action="append", required=True, help="mode=path, first is the reference")
    ap.add_argument("--out", required=True)
    ap.add_argument("--job-dir", default="/root/bench69/js/job")
    ap.add_argument("--reps", type=int, default=1)
    ap.add_argument("--rep-start", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--timeout", type=int, default=900)
    ap.add_argument("--stats", action="store_true")
    ap.add_argument("--report-only", action="store_true")
    a = ap.parse_args()
    modes = []
    bins = {}
    for item in a.bin:
        m, p = item.split("=", 1)
        modes.append(m)
        bins[m] = p
    os.makedirs(a.out, exist_ok=True)
    runs_path = os.path.join(a.out, "runs.jsonl")
    stats_path = os.path.join(a.out, "stats.jsonl")
    jobs = job_names(a.job_dir)
    if a.only:
        pat = re.compile(a.only)
        jobs = [j for j in jobs if pat.search(j)]
    if not a.report_only:
        done = {(r["job"], r["mode"], r["rep"]) for r in load_rows(runs_path)}
        for rep in range(a.rep_start, a.rep_start + a.reps):
            for j in jobs:
                payload = load_job(a.job_dir, j)
                order = modes[rep % len(modes):] + modes[:rep % len(modes)]
                for m in order:
                    if (j, m, rep) in done:
                        continue
                    r = timed_run(bins[m], payload, a.timeout)
                    r.update({"job": j, "mode": m, "rep": rep})
                    with open(runs_path, "a") as f:
                        f.write(json.dumps(r) + "\n")
                    print(j, m, rep, "rc", r["rc"], "cpu", r["cpu"], "rss", r["rss_kb"], "load", r["load1"], flush=True)
        if a.stats:
            sdone = {(r["job"], r["mode"]) for r in load_rows(stats_path)}
            with tempfile.TemporaryDirectory() as d:
                for j in jobs:
                    jf = os.path.join(d, j + ".bin")
                    open(jf, "wb").write(load_job(a.job_dir, j))
                    for m in modes:
                        if (j, m) in sdone:
                            continue
                        s = gdb_state(bins[m], jf)
                        if not s:
                            continue
                        s.update({"job": j, "mode": m})
                        with open(stats_path, "a") as f:
                            f.write(json.dumps(s) + "\n")
                        print("stats", j, m, s, flush=True)
    text = report(load_rows(runs_path), modes, load_rows(stats_path))
    open(os.path.join(a.out, "report.txt"), "w").write(text + "\n")
    print(text)


if __name__ == "__main__":
    sys.exit(main())
