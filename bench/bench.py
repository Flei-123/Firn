#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""An honest measurement: Firn against Rust (rustc -O), the same machine, median.

**AND IT SAYS WHICH BUILD LEVEL IT MEASURED.** That sentence is the whole
point of this file's round-SPEED rewrite. Until then it compiled the Firn
side with

    firnc -o bin bench/firn/<name>.fi

-- no build level at all -- and called the column "Firn". Since round 72 the
DEFAULT level is `dev-fast`, and `dev-fast` (a) CHECKS every integer
operation and (b) does not run the one pass that is not debug preserving,
`inline`. So every number this file used to print held a checked, uninlined
everyday build against a fully optimised unchecked `rustc -O` one and called
the difference "Firn is slower". `sieve` stood in `bench/RESULTS.md` at
4.16x; at `release-fast` it is under 1x. That was not a lie anybody told; it
was a measurement that forgot to say what it measured (`docs/ROUND90.md`
section 2.1 found it, `docs/ROUNDSPEED.md` round 1 acted on it).

FOUR columns per program now, each named:

    firn release-fast   all passes, unchecked  -- the partner of `rustc -O`
    firn release-safe   all passes, CHECKED    -- Firn doing strictly more
    firn dev-fast       the default level      -- what a plain `firnc` gives
    rustc -O            all passes, unchecked

Every microbenchmark exists twice (`bench/firn/<name>.fi` and
`bench/rust/<name>.rs`), both PRINT THEIR RESULT and the outputs have to
match -- so nothing can be optimised away on either side (on the Rust side
additionally `std::hint::black_box`).

For the four-column comparison that also turns the checks ON on the Rust
side, use `tools/bench90/bench.py`; for a difference that the clock of a
shared machine cannot resolve, use `tools/bench90/icount.py` (executed
instructions, exact).

    bash bench/run.sh              # BENCH_RUNS=5 by default
    BENCH_RUNS=9 bash bench/run.sh
"""
import os
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
WORK = os.path.join(HERE, ".work")
RUNS = int(os.environ.get("BENCH_RUNS", "5"))

BENCHES = ["fib", "sieve", "matmul", "bytecount", "bubblesort", "statemachine"]

# column key -> (heading, how it is built)
LEVELS = [
    ("fast", "Firn `release-fast`", ["--opt-level=release-fast"]),
    ("safe", "Firn `release-safe`", ["--opt-level=release-safe"]),
    ("devf", "Firn `dev-fast` (default)", ["--opt-level=dev-fast"]),
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
        print("ERROR: %s ended with %d" % (binary, r.returncode))
        sys.exit(1)
    return dt, r.stdout.strip()


def main():
    os.makedirs(WORK, exist_ok=True)
    if not os.path.exists(FIRNC):
        print("the compiler is missing: %s (cargo build --release)" % FIRNC)
        sys.exit(1)
    rows = []
    for name in BENCHES:
        fi = os.path.join(HERE, "firn", name + ".fi")
        rs = os.path.join(HERE, "rust", name + ".rs")
        print("== building %s" % name, flush=True)
        bins = {}
        for key, _, flags in LEVELS:
            b = os.path.join(WORK, "%s.%s" % (name, key))
            sh([FIRNC] + flags + ["-o", b, fi])
            bins[key] = b
        bins["rust"] = os.path.join(WORK, name + ".rust")
        sh(["rustc", "-O", "-o", bins["rust"], rs])

        times, res = {}, {}
        for key, b in bins.items():
            samples = []
            out = None
            for _ in range(RUNS):
                dt, o = timed(b)
                samples.append(dt)
                out = o
            times[key] = statistics.median(samples)
            res[key] = out
        if len(set(res.values())) != 1:
            print("STOP: differing results for %s: %r" % (name, res))
            sys.exit(2)
        row = dict(name=name, out=res["rust"], **times)
        for key, _, _ in LEVELS:
            row["f_" + key] = times[key] / times["rust"]
        rows.append(row)
        print(
            "   release-fast %.3fs (%.2fx) | release-safe %.3fs (%.2fx) |"
            " dev-fast %.3fs (%.2fx) | rustc -O %.3fs | result %s"
            % (
                times["fast"], row["f_fast"],
                times["safe"], row["f_safe"],
                times["devf"], row["f_devf"],
                times["rust"], res["rust"],
            ),
            flush=True,
        )

    hdr = "| benchmark | " + " | ".join(h for _, h, _ in LEVELS) + " | `rustc -O` | " \
        + " | ".join("factor " + k for k, _, _ in LEVELS) + " | result |\n"
    hdr += "|---" * (2 + 2 * len(LEVELS)) + "|\n"
    hdr = hdr.replace("|---|", "|---|", 1)
    body = ""
    for r in rows:
        body += "| %s | %s | %s | **%s** | %s |\n" % (
            r["name"],
            " | ".join("%.3f s" % r[k] for k, _, _ in LEVELS),
            "%.3f s" % r["rust"],
            "** | **".join("%.2fx" % r["f_" + k] for k, _, _ in LEVELS),
            r["out"],
        )
    summary = "\n"
    for key, head, _ in LEVELS:
        f = [r["f_" + key] for r in rows]
        summary += "Median %s against `rustc -O`: **%.2fx** (range %.2fx - %.2fx).\n" % (
            head, statistics.median(f), min(f), max(f))
    summary += (
        "\n`release-fast` is the like-for-like comparison: all passes, and "
        "integer arithmetic unchecked exactly as `rustc -O` leaves it. "
        "`release-safe` runs the same passes and CHECKS every integer "
        "operation, so it is Firn doing strictly more work than Rust. "
        "`dev-fast` is what a plain `firnc` gives you: checked, and without "
        "the one pass that would make the call stack unreadable.\n"
    )
    uname = subprocess.run(["uname", "-srm"], capture_output=True, text=True).stdout.strip()
    try:
        with open("/proc/cpuinfo") as fh:
            cpu = [l.split(":", 1)[1].strip() for l in fh if l.startswith("model name")][0]
    except Exception:
        cpu = "unknown"
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    text = (
        "# Benchmark results (really measured)\n\n"
        "Produced by `bench/run.sh` (`bench/bench.py`), %d runs per program, **median**.\n"
        "Every benchmark exists twice -- `bench/firn/<name>.fi` and "
        "`bench/rust/<name>.rs` -- and both print their result; the outputs "
        "have to match, otherwise the measurement stops.\n"
        "The Rust side uses `std::hint::black_box` and the same unchecked "
        "pointer accesses as the Firn side, so that the same work is measured.\n\n"
        "* CPU: %s\n* system: %s\n* %s\n* Firn: its own code generator, no external crates\n\n"
        % (RUNS, cpu, uname, rustc)
    ) + hdr + body + summary
    with open(os.path.join(HERE, "RESULTS.md"), "w") as fh:
        fh.write(text)
    print("\n" + hdr + body + summary)
    print("written: bench/RESULTS.md")


if __name__ == "__main__":
    main()
