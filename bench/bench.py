#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""An honest measurement: Firn against Rust (rustc -O), the same machine, median.

Every microbenchmark exists twice: `bench/firn/<name>.fi` and
`bench/rust/<name>.rs`. Both compute the same thing and PRINT THE RESULT --
comparing the outputs is part of the test, so that no work can be optimised
away on either side (on the Rust side additionally `black_box`).

What is measured is the total run time of the process (the median of N runs).
Output: a table on stdout and `bench/RESULTS.md`.
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


def sh(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print("ERROR at: %s\n%s\n%s" % (" ".join(cmd), r.stdout, r.stderr))
        sys.exit(1)
    return r


def timed(binary):
    """Laufzeit in Sekunden + Ausgabe."""
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
        bin_firn = os.path.join(WORK, name + ".firn")
        bin_firn_noopt = os.path.join(WORK, name + ".firn.noopt")
        bin_rust = os.path.join(WORK, name + ".rust")
        print("== uebersetze %s" % name, flush=True)
        sh([FIRNC, "-o", bin_firn, fi])
        sh([FIRNC, "--no-opt", "-o", bin_firn_noopt, fi])
        sh(["rustc", "-O", "-o", bin_rust, rs])

        res = {}
        times = {}
        for key, binary in (
            ("firn", bin_firn),
            ("firn_noopt", bin_firn_noopt),
            ("rust", bin_rust),
        ):
            samples = []
            out = None
            for _ in range(RUNS):
                dt, o = timed(binary)
                samples.append(dt)
                out = o
            times[key] = statistics.median(samples)
            res[key] = out
        if res["firn"] != res["rust"] or res["firn"] != res["firn_noopt"]:
            print(
                "ABBRUCH: unterschiedliche Ergebnisse bei %s: firn=%r firn(--no-opt)=%r rust=%r"
                % (name, res["firn"], res["firn_noopt"], res["rust"])
            )
            sys.exit(2)
        rows.append(
            (
                name,
                times["firn"],
                times["firn_noopt"],
                times["rust"],
                times["firn"] / times["rust"],
                times["firn_noopt"] / times["firn"],
                res["firn"],
            )
        )
        print(
            "   Firn %.3fs | Firn --no-opt %.3fs | Rust -O %.3fs | factor %.2fx | result %s"
            % (
                times["firn"],
                times["firn_noopt"],
                times["rust"],
                times["firn"] / times["rust"],
                res["firn"],
            ),
            flush=True,
        )

    hdr = (
        "| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Faktor Firn/Rust |"
        " gain through the optimiser | result |\n"
        "|---|---:|---:|---:|---:|---:|---:|\n"
    )
    body = ""
    for n, f, fn, r, fac, gain, out in rows:
        body += "| %s | %.3f s | %.3f s | %.3f s | **%.2fx** | %.2fx | %s |\n" % (
            n,
            f,
            fn,
            r,
            fac,
            gain,
            out,
        )
    facs = [x[4] for x in rows]
    gains = [x[5] for x in rows]
    summary = (
        "\nMedian ueber alle Benchmarks: **%.2fx** langsamer als Rust `-O` "
        "(Spanne %.2fx – %.2fx).\n"
        "Der Optimierer (mem2reg, CSE, Inlining, Registerzuteilung) bringt im "
        "Median **%.2fx** gegenueber `--no-opt`.\n"
        % (statistics.median(facs), min(facs), max(facs), statistics.median(gains))
    )
    uname = subprocess.run(["uname", "-srm"], capture_output=True, text=True).stdout.strip()
    try:
        with open("/proc/cpuinfo") as fh:
            cpu = [l.split(":", 1)[1].strip() for l in fh if l.startswith("model name")][0]
    except Exception:
        cpu = "unbekannt"
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    text = (
        "# Benchmark-Ergebnisse (real gemessen)\n\n"
        "Produced by `bench/run.sh` (`bench/bench.py`), %d runs per program, **median**.\n"
        "Every benchmark exists twice -- `bench/firn/<name>.fi` and "
        "`bench/rust/<name>.rs` -- and both print their result; the outputs "
        "have to match, otherwise the measurement stops.\n"
        "The Rust side uses `std::hint::black_box` and the same unchecked "
        "pointer accesses as the Firn side, so that the same work is measured.\n\n"
        "* CPU: %s\n* System: %s\n* %s\n* Firn: eigener Codegenerator, keine externen Crates\n\n"
        % (RUNS, cpu, uname, rustc)
    ) + hdr + body + summary
    with open(os.path.join(HERE, "RESULTS.md"), "w") as fh:
        fh.write(text)
    print("\n" + hdr + body + summary)
    print("written: bench/RESULTS.md")


if __name__ == "__main__":
    main()
