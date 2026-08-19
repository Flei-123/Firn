#!/usr/bin/env python3
"""Misst dev / dev-fast / release-fast auf der Benchmark-Suite (Median)."""
import glob, os, statistics, subprocess, sys, time

FIRNC = "compiler/target/release/firnc"
RUNS = int(sys.argv[1]) if len(sys.argv) > 1 else 5
STUFEN = ["dev", "dev-fast", "release-fast"]


def bauen(quelle, stufe, ziel):
    r = subprocess.run([FIRNC, f"--opt-level={stufe}", "-o", ziel, quelle],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"FEHLER beim Bauen von {quelle} ({stufe}):\n{r.stderr}")


def messen(binary):
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
    print(f"Baustufen-Vergleich, {RUNS} Laeufe je Programm, Median\n")
    print(f"{'Benchmark':<14}{'dev':>10}{'dev-fast':>11}{'release':>10}"
          f"{'dev-fast/rel':>14}{'dev/rel':>10}")
    print("-" * 69)
    verh_df, verh_dev, fehler = [], [], 0
    for q in quellen:
        name = os.path.basename(q)[:-3]
        zeit, ausgabe = {}, {}
        for st in STUFEN:
            ziel = f"/tmp/baustufe_{name}_{st}"
            bauen(q, st, ziel)
            zeit[st], ausgabe[st] = messen(ziel)
        # Korrektheit: alle Stufen muessen dasselbe liefern
        if len(set(ausgabe.values())) != 1:
            print(f"{name:<14}  ABWEICHENDE AUSGABE zwischen den Stufen: {ausgabe}")
            fehler += 1
            continue
        r_df = zeit["dev-fast"] / zeit["release-fast"]
        r_dev = zeit["dev"] / zeit["release-fast"]
        verh_df.append(r_df)
        verh_dev.append(r_dev)
        print(f"{name:<14}{zeit['dev']:>10.3f}{zeit['dev-fast']:>11.3f}"
              f"{zeit['release-fast']:>10.3f}{r_df:>13.2f}x{r_dev:>9.2f}x")
    print("-" * 69)
    if fehler:
        raise SystemExit(f"\nFEHLER: {fehler} Benchmark(s) mit abweichender Ausgabe.")
    m_df, m_dev = statistics.median(verh_df), statistics.median(verh_dev)
    print(f"\nMedian dev-fast : {m_df:.2f}x langsamer als release-fast")
    print(f"Median dev      : {m_dev:.2f}x langsamer als release-fast")
    print(f"\nZielwert aus DESIGNZIELE.md §5 fuer dev-fast: 2-3x. "
          f"{'ERREICHT' if m_df <= 3.0 else 'VERFEHLT'}.")
    print("Zum Vergleich: Rust-Debug-Builds liegen typisch bei 10-50x.")


if __name__ == "__main__":
    main()
