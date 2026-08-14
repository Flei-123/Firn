# Benchmark-Ergebnisse (real gemessen)

Erzeugt von `bench/run.sh` (`bench/bench.py`), 3 Laeufe je Programm, **Median**.
Jeder Benchmark existiert zweimal — `bench/firn/<name>.fi` und `bench/rust/<name>.rs` — und beide geben ihr Ergebnis aus; die Ausgaben muessen uebereinstimmen, sonst bricht die Messung ab.
Die Rust-Seite benutzt `std::hint::black_box` und dieselben ungeprueften Zeigerzugriffe wie die Firn-Seite, damit dieselbe Arbeit gemessen wird.

* CPU: AMD EPYC 7571 32-Core Processor
* System: Linux 7.0.14-5-pve x86_64
* rustc 1.99.0-nightly (c98d0cb27 2026-08-12)
* Firn: eigener Codegenerator, keine externen Crates

| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Faktor Firn/Rust | Gewinn durch Optimierer | Ergebnis |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.046 s | 0.136 s | 0.029 s | **1.56x** | 2.97x | 4356618 |
| sieve | 0.112 s | 1.221 s | 0.028 s | **3.98x** | 10.88x | 697026 |
| matmul | 0.125 s | 1.955 s | 0.022 s | **5.61x** | 15.58x | 8291727 |
| bytecount | 0.475 s | 5.135 s | 0.181 s | **2.62x** | 10.80x | 1604208 |
| bubblesort | 0.105 s | 1.309 s | 0.037 s | **2.87x** | 12.44x | 12021846167 |
| statemachine | 0.222 s | 1.201 s | 0.082 s | **2.69x** | 5.41x | 6710880 |

Median ueber alle Benchmarks: **2.78x** langsamer als Rust `-O` (Spanne 1.56x – 5.61x).
Der Optimierer (mem2reg, CSE, Inlining, Registerzuteilung) bringt im Median **10.84x** gegenueber `--no-opt`.
