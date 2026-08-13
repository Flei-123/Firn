#!/usr/bin/env python3
"""Ehrliche Messung: Firn gegen Rust (rustc -O), gleicher Rechner, Median.

Jeder Mikrobenchmark liegt doppelt vor: `bench/firn/<name>.fi` und
`bench/rust/<name>.rs`. Beide rechnen dasselbe und GEBEN DAS ERGEBNIS AUS —
der Vergleich der Ausgaben ist Teil des Tests, damit auf keiner Seite Arbeit
wegoptimiert werden kann (auf der Rust-Seite zusaetzlich `black_box`).

Gemessen wird die Gesamtlaufzeit des Prozesses (Median aus N Laeufen).
Ausgabe: Tabelle nach stdout und `bench/RESULTS.md`.
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
        print("FEHLER bei: %s\n%s\n%s" % (" ".join(cmd), r.stdout, r.stderr))
        sys.exit(1)
    return r


def timed(binary):
    """Laufzeit in Sekunden + Ausgabe."""
    t0 = time.perf_counter()
    r = subprocess.run([binary], capture_output=True, text=True)
    dt = time.perf_counter() - t0
    if r.returncode != 0:
        print("FEHLER: %s endete mit %d" % (binary, r.returncode))
        sys.exit(1)
    return dt, r.stdout.strip()


def main():
    os.makedirs(WORK, exist_ok=True)
    if not os.path.exists(FIRNC):
        print("Compiler fehlt: %s (cargo build --release)" % FIRNC)
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
            "   Firn %.3fs | Firn --no-opt %.3fs | Rust -O %.3fs | Faktor %.2fx | Ergebnis %s"
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
        " Gewinn durch Optimierer | Ergebnis |\n"
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
        "Erzeugt von `bench/run.sh` (`bench/bench.py`), %d Laeufe je Programm, **Median**.\n"
        "Jeder Benchmark existiert zweimal — `bench/firn/<name>.fi` und "
        "`bench/rust/<name>.rs` — und beide geben ihr Ergebnis aus; die Ausgaben "
        "muessen uebereinstimmen, sonst bricht die Messung ab.\n"
        "Die Rust-Seite benutzt `std::hint::black_box` und dieselben ungeprueften "
        "Zeigerzugriffe wie die Firn-Seite, damit dieselbe Arbeit gemessen wird.\n\n"
        "* CPU: %s\n* System: %s\n* %s\n* Firn: eigener Codegenerator, keine externen Crates\n\n"
        % (RUNS, cpu, uname, rustc)
    ) + hdr + body + summary
    with open(os.path.join(HERE, "RESULTS.md"), "w") as fh:
        fh.write(text)
    print("\n" + hdr + body + summary)
    print("geschrieben: bench/RESULTS.md")


if __name__ == "__main__":
    main()
