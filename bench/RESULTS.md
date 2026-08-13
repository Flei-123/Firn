# Benchmark-Ergebnisse (real gemessen)

Erzeugt von `bench/run.sh` (`bench/bench.py`), 5 Laeufe je Programm, **Median**.
Jeder Benchmark existiert zweimal — `bench/firn/<name>.fi` und `bench/rust/<name>.rs` — und beide geben ihr Ergebnis aus; die Ausgaben muessen uebereinstimmen, sonst bricht die Messung ab.
Die Rust-Seite benutzt `std::hint::black_box` und dieselben ungeprueften Zeigerzugriffe wie die Firn-Seite, damit dieselbe Arbeit gemessen wird.

* CPU: AMD EPYC 7571 32-Core Processor
* System: Linux 7.0.14-5-pve x86_64
* rustc 1.99.0-nightly (c98d0cb27 2026-08-12)
* Firn: eigener Codegenerator, keine externen Crates

| Benchmark | Firn | Firn `--no-opt` | Rust `-O` | Faktor Firn/Rust | Gewinn durch Optimierer | Ergebnis |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.046 s | 0.136 s | 0.029 s | **1.57x** | 2.95x | 4356618 |
| sieve | 0.116 s | 1.232 s | 0.029 s | **3.97x** | 10.67x | 697026 |
| matmul | 0.140 s | 2.227 s | 0.023 s | **6.04x** | 15.86x | 8291727 |
| bytecount | 0.515 s | 6.203 s | 0.291 s | **1.77x** | 12.05x | 1604208 |
| bubblesort | 0.185 s | 1.752 s | 0.036 s | **5.19x** | 9.45x | 12021846167 |
| statemachine | 0.246 s | 2.142 s | 0.089 s | **2.76x** | 8.70x | 6710880 |

Median ueber alle Benchmarks: **3.36x** langsamer als Rust `-O` (Spanne 1.57x – 6.04x).
Der Optimierer (mem2reg, CSE, Inlining, Registerzuteilung) bringt im Median **10.06x** gegenueber `--no-opt`.
