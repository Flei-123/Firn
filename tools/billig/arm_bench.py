#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/billig/arm_bench.py -- WHAT THE ARM64 REGISTER ALLOCATOR REALLY BRINGS.

The same eleven benches with which rounds 43 and 90 measured the x86
allocator (`bench/firn/`), once with the old and once with the new compiler,
both for `--target=aarch64-linux`.

THREE NUMBERS PER BENCH, and each says something different:

  1. STATIC INSTRUCTIONS -- counted in the generated assembly text
     (`--emit=asm`), without labels, comments and directives. Exact,
     deterministic, and the only number here that does not depend on the
     machine at all.

  2. EXECUTED INSTRUCTIONS -- `qemu-aarch64 -singlestep -d cpu` writes one
     register block per executed instruction (counted by
     `tools/billig/arm_icount.sh`). This is the number round 43 measured on
     x86 with callgrind (-26.15 %), just for ARM64. It is exact, but it costs
     about 90 000 instructions per second -- a bench that runs a billion
     instructions cannot be measured this way. Hence a time limit in front,
     and the report says which bench broke it.

  3. THROUGHPUT -- wall clock under `qemu-aarch64`, best of N runs.
     IMPORTANT AND HONEST: this is an emulator, not a phone. TCG translates
     blocks and runs them as x86 code; the number follows the count of
     executed instructions much more closely than real ARM64 hardware would
     (no branch predictor, no out-of-order execution, another cache
     hierarchy). It is a DIFFERENCE between two builds of the same program on
     the same emulator -- not more, but not less either.

Usage:
    python3 tools/billig/arm_bench.py --old <firnc> --new <firnc> \\
        [--stage dev-fast] [--runs 5] [--icount-limit 120] [--json <file>]
"""
import argparse
import json
import math
import os
import re
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BENCHES = ["fib", "sieve", "matmul", "bytecount", "bubblesort", "statemachine",
           "bitmap", "xxhash", "jsonscan", "memstride", "branchy"]

LABEL = re.compile(r"^\s*[.\w$]+:\s*$")
DIRECTIVE = re.compile(r"^\s*\.")
COMMENT = re.compile(r"^\s*(//|#)")


def static_instructions(path):
    n = 0
    for line in open(path, encoding="utf-8", errors="replace"):
        if not line.strip():
            continue
        if LABEL.match(line) or DIRECTIVE.match(line) or COMMENT.match(line):
            continue
        n += 1
    return n


def compile_bench(firnc, source, target, stage, asm=False):
    env = dict(os.environ)
    env["FIRNLIB"] = os.path.join(ROOT, "lib")
    cmd = [firnc, "--target=aarch64-linux", "--opt-level=" + stage]
    if asm:
        cmd.append("--emit=asm")
    cmd += ["-o", target, source]
    p = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if p.returncode != 0:
        return p.stderr[:400]
    return None


def wall_clock(binary, runs):
    times = []
    result = None
    for _ in range(runs):
        t = time.perf_counter()
        p = subprocess.run(["qemu-aarch64", binary], capture_output=True, timeout=3600)
        times.append(time.perf_counter() - t)
        result = (p.returncode, p.stdout)
    return min(times), result


def icount(binary, limit):
    """Exact number of executed instructions -- or None if it takes too long.

    The counting itself runs in `tools/billig/arm_icount.sh` (a named pipe and
    `grep -c`, not Python): qemu writes ten lines of register state per
    executed instruction, and a bench with a few million instructions is
    gigabytes already.
    """
    script = os.path.join(HERE, "arm_icount.sh")
    p = subprocess.run(["bash", script, str(int(limit)), binary],
                       capture_output=True, text=True)
    fields = p.stdout.split()
    if p.returncode != 0 or len(fields) != 2:
        return None
    try:
        return int(fields[1])
    except ValueError:
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--old", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--stage", default="dev-fast")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--icount-limit", type=float, default=120.0)
    ap.add_argument("--only", default="")
    ap.add_argument("--json", default="")
    a = ap.parse_args()
    os.chdir(ROOT)
    work = tempfile.mkdtemp(prefix="arm-bench-")

    results = {}
    print("%-14s %-26s %-27s %s"
          % ("Bench", "static instructions", "executed instructions", "wall clock qemu (s)"))
    print("%-14s %8s %8s %7s  %8s %8s %7s  %9s %9s %s"
          % ("", "old", "new", "diff", "old", "new", "diff", "old", "new", "factor"))
    for name in BENCHES:
        if a.only and a.only not in name:
            continue
        source = os.path.join(ROOT, "bench/firn", name + ".fi")
        if not os.path.exists(source):
            continue
        z = {}
        error = None
        for tag, firnc in (("old", a.old), ("new", a.new)):
            b = os.path.join(work, "%s.%s" % (name, tag))
            s = b + ".s"
            error = compile_bench(firnc, source, s, a.stage, asm=True)
            if error:
                break
            z["stat_" + tag] = static_instructions(s)
            error = compile_bench(firnc, source, b, a.stage)
            if error:
                break
            z["bin_" + tag] = b
        if error:
            print("%-14s COMPILATION FAILED: %s" % (name, error.strip()[:120]))
            continue
        t_old, o_old = wall_clock(z["bin_old"], a.runs)
        t_new, o_new = wall_clock(z["bin_new"], a.runs)
        z["t_old"], z["t_new"] = t_old, t_new
        z["same"] = (o_old == o_new)
        z["ic_old"] = icount(z["bin_old"], a.icount_limit)
        z["ic_new"] = icount(z["bin_new"], a.icount_limit) if z["ic_old"] else None
        results[name] = {k: v for k, v in z.items() if not k.startswith("bin_")}
        if z["ic_old"] and z["ic_new"]:
            ic = "%8d %8d %6.1f%%" % (z["ic_old"], z["ic_new"],
                                      100.0 * (z["ic_new"] - z["ic_old"]) / z["ic_old"])
        else:
            ic = "%25s" % "too big for singlestep"
        print("%-14s %8d %8d %6.1f%%  %s  %9.3f %9.3f %6.3fx%s"
              % (name, z["stat_old"], z["stat_new"],
                 100.0 * (z["stat_new"] - z["stat_old"]) / z["stat_old"],
                 ic, t_old, t_new, t_old / t_new,
                 "" if z["same"] else "  !! RESULTS DIFFER"))
        sys.stdout.flush()
    if a.json:
        json.dump(results, open(a.json, "w"), indent=1)
        print("written", a.json)
    q = [v["t_old"] / v["t_new"] for v in results.values()]
    s_old = sum(v["stat_old"] for v in results.values())
    s_new = sum(v["stat_new"] for v in results.values())
    ics = [(v["ic_old"], v["ic_new"]) for v in results.values()
           if v.get("ic_old") and v.get("ic_new")]
    print()
    print("static instructions total: %d -> %d  (%+.2f %%)"
          % (s_old, s_new, 100.0 * (s_new - s_old) / s_old))
    if ics:
        ia = sum(x for x, _ in ics)
        ib = sum(y for _, y in ics)
        print("executed instructions (%d benches): %d -> %d  (%+.2f %%)"
              % (len(ics), ia, ib, 100.0 * (ib - ia) / ia))
    if q:
        print("throughput under qemu, geometric mean: %.3f x"
              % math.exp(sum(math.log(x) for x in q) / len(q)))
    all_same = all(v["same"] for v in results.values())
    print("results identical in all %d benches: %s" % (len(results), "YES" if all_same else "NO"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
