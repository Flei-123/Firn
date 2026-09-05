#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Measures dev / dev-fast / release-fast on the benchmark suite (median)."""
import glob, os, statistics, subprocess, sys, time

FIRNC = "compiler/target/release/firnc"
RUNS = int(sys.argv[1]) if len(sys.argv) > 1 else 5
STUFEN = ["dev", "dev-fast", "release-fast"]


def build_one(source, stage, target):
    r = subprocess.run([FIRNC, f"--opt-level={stage}", "-o", target, source],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"ERROR while building {source} ({stage}):\n{r.stderr}")


def measure(binary):
    zeiten = []
    for _ in range(RUNS):
        t0 = time.perf_counter()
        p = subprocess.run([binary], capture_output=True)
        zeiten.append(time.perf_counter() - t0)
    return statistics.median(zeiten), p.stdout.decode(errors="replace").strip()


def main():
    quellen = sorted(glob.glob("bench/firn/*.fi"))
    if not quellen:
        raise SystemExit("keine Benchmarks unter bench/firn/ gefunden")
    print(f"build stage comparison, {RUNS} runs per program, median\n")
    print(f"{'Benchmark':<14}{'dev':>10}{'dev-fast':>11}{'release':>10}"
          f"{'dev-fast/rel':>14}{'dev/rel':>10}")
    print("-" * 69)
    ratio_df, ratio_dev, errs = [], [], 0
    for q in quellen:
        name = os.path.basename(q)[:-3]
        times, output = {}, {}
        for st in STUFEN:
            target = f"/tmp/baustufe_{name}_{st}"
            build_one(q, st, target)
            times[st], output[st] = measure(target)
        # Correctness: all stages have to yield the same
        if len(set(output.values())) != 1:
            print(f"{name:<14}  ABWEICHENDE AUSGABE zwischen den Stufen: {output}")
            errs += 1
            continue
        r_df = times["dev-fast"] / times["release-fast"]
        r_dev = times["dev"] / times["release-fast"]
        ratio_df.append(r_df)
        ratio_dev.append(r_dev)
        print(f"{name:<14}{times['dev']:>10.3f}{times['dev-fast']:>11.3f}"
              f"{times['release-fast']:>10.3f}{r_df:>13.2f}x{r_dev:>9.2f}x")
    print("-" * 69)
    if errs:
        raise SystemExit(f"\nERROR: {errs} benchmark(s) with differing output.")
    m_df, m_dev = statistics.median(ratio_df), statistics.median(ratio_dev)
    print(f"\nMedian dev-fast : {m_df:.2f}x langsamer als release-fast")
    print(f"Median dev      : {m_dev:.2f}x langsamer als release-fast")
    print(f"\ntarget value from DESIGN_GOALS.md 5 for dev-fast: 2-3x. "
          f"{'ERREICHT' if m_df <= 3.0 else 'VERFEHLT'}.")
    print("Zum Vergleich: Rust-Debug-Builds liegen typisch bei 10-50x.")


if __name__ == "__main__":
    main()
