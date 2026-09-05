#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/bench90/bench.py -- Firn against Rust, WITH THE BUILD LEVEL NAMED.

WHY THIS EXISTS NEXT TO bench/bench.py
--------------------------------------
`bench/bench.py` compiles the Firn side with `firnc -o x y.fi` -- no build
level at all -- and calls the column "Firn". Since round 72 the DEFAULT
level is `dev-fast`, and `dev-fast` CHECKS integer arithmetic. So every
number in bench/RESULTS.md compares a Firn build WITH overflow checks
against a Rust build WITHOUT them and calls the difference "Firn is slower".
That is not a lie anybody told; it is a measurement that forgot to say what
it measured.

This harness names the level, and it measures FOUR columns per program:

    firn release-fast      no checks   -- the partner of `rustc -O`
    firn release-safe      checked     -- the partner of `rustc -C overflow-checks=yes`
    rust -O                no checks
    rust -O + checks       checked

The two honest questions are then separate:
    1. codegen        release-fast  vs  rustc -O
    2. safety price   release-safe  vs  rustc -O -C overflow-checks=yes

Both sides print their result and the outputs must match, so nothing can be
optimised away on either side.

    python3 tools/bench90/bench.py            # BENCH90_RUNS=7 by default
"""
import json
import os
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
WORK = os.path.join(HERE, ".work")
RUNS = int(os.environ.get("BENCH90_RUNS", "7"))
ONLY = os.environ.get("BENCH90_ONLY", "").split(",") if os.environ.get("BENCH90_ONLY") else None

# name -> short description of what it stresses
BENCHES = [
    ("fib", "recursion / call overhead"),
    ("sieve", "memory, byte writes in a loop"),
    ("matmul", "nested loops, index arithmetic"),
    ("bytecount", "memory, sequential read"),
    ("bubblesort", "memory + branch"),
    ("statemachine", "branches, table dispatch"),
    ("bitmap", "the osum frame allocator (bit set/clear/search)"),
    ("xxhash", "xxHash64 over 64 MiB"),
    ("jsonscan", "JSON scanner over a generated document"),
    ("memstride", "memory bound, cache-hostile stride"),
    ("branchy", "unpredictable branches"),
]


def sh(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print("ERROR at: %s\n%s\n%s" % (" ".join(cmd), r.stdout, r.stderr))
        sys.exit(1)
    return r


def timed(binary):
    t0 = time.perf_counter()
    r = subprocess.run([binary], capture_output=True, text=True)
    dt = time.perf_counter() - t0
    if r.returncode != 0:
        print("ERROR: %s ended with %d\n%s" % (binary, r.returncode, r.stderr[:400]))
        sys.exit(1)
    return dt, r.stdout.strip()


def main():
    os.makedirs(WORK, exist_ok=True)
    if not os.path.exists(FIRNC):
        print("the compiler is missing: %s (cargo build --release)" % FIRNC)
        sys.exit(1)
    rows = []
    for name, what in BENCHES:
        if ONLY and name not in ONLY:
            continue
        fi = os.path.join(ROOT, "bench/firn", name + ".fi")
        rs = os.path.join(ROOT, "bench/rust", name + ".rs")
        if not (os.path.exists(fi) and os.path.exists(rs)):
            print("== skip %s (no pair)" % name)
            continue
        print("== %s" % name, flush=True)
        bins = {
            "firn_fast": os.path.join(WORK, name + ".firn.fast"),
            "firn_safe": os.path.join(WORK, name + ".firn.safe"),
            "rust_fast": os.path.join(WORK, name + ".rust.fast"),
            "rust_safe": os.path.join(WORK, name + ".rust.safe"),
        }
        sh([FIRNC, "--opt-level=release-fast", "-o", bins["firn_fast"], fi])
        sh([FIRNC, "--opt-level=release-safe", "-o", bins["firn_safe"], fi])
        sh(["rustc", "-O", "-C", "overflow-checks=no", "-o", bins["rust_fast"], rs])
        sh(["rustc", "-O", "-C", "overflow-checks=yes", "-o", bins["rust_safe"], rs])

        times, res = {}, {}
        for key in bins:
            samples = []
            out = None
            for _ in range(RUNS):
                dt, o = timed(bins[key])
                samples.append(dt)
                out = o
            times[key] = statistics.median(samples)
            res[key] = out
        outs = set(res.values())
        if len(outs) != 1:
            print("STOP: differing results for %s: %r" % (name, res))
            sys.exit(2)
        row = dict(name=name, what=what, out=res["firn_fast"], **times)
        row["fac_fast"] = times["firn_fast"] / times["rust_fast"]
        row["fac_safe"] = times["firn_safe"] / times["rust_safe"]
        row["safety_price"] = times["firn_safe"] / times["firn_fast"]
        rows.append(row)
        print(
            "   fast %.3fs vs rust %.3fs = %.2fx | safe %.3fs vs rust+checks %.3fs = %.2fx"
            " | price of the checks %.2fx"
            % (times["firn_fast"], times["rust_fast"], row["fac_fast"],
               times["firn_safe"], times["rust_safe"], row["fac_safe"], row["safety_price"]),
            flush=True,
        )

    out_json = os.environ.get("BENCH90_JSON", os.path.join(WORK, "results.json"))
    with open(out_json, "w") as fh:
        json.dump(rows, fh, indent=1)

    print()
    print("| benchmark | what it stresses | firn release-fast | rustc -O | factor |"
          " firn release-safe | rustc -O +checks | factor | price of the checks |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        print("| %s | %s | %.3f s | %.3f s | **%.2fx** | %.3f s | %.3f s | **%.2fx** | %.2fx |"
              % (r["name"], r["what"], r["firn_fast"], r["rust_fast"], r["fac_fast"],
                 r["firn_safe"], r["rust_safe"], r["fac_safe"], r["safety_price"]))
    if rows:
        print()
        print("median release-fast vs rustc -O:      %.2fx"
              % statistics.median([r["fac_fast"] for r in rows]))
        print("median release-safe vs rustc +checks: %.2fx"
              % statistics.median([r["fac_safe"] for r in rows]))
        print("median price of the checks in Firn:   %.2fx"
              % statistics.median([r["safety_price"] for r in rows]))
    print("written: %s" % out_json)


if __name__ == "__main__":
    main()
