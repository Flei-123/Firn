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
| fib | 0.053 s | 0.141 s | 0.031 s | **1.70x** | 2.67x | 4356618 |
| sieve | 0.120 s | 1.325 s | 0.031 s | **3.87x** | 11.09x | 697026 |
| matmul | 0.107 s | 2.107 s | 0.024 s | **4.46x** | 19.72x | 8291727 |
| bytecount | 0.481 s | 5.617 s | 0.210 s | **2.29x** | 11.67x | 1604208 |
| bubblesort | 0.112 s | 1.387 s | 0.039 s | **2.88x** | 12.37x | 12021846167 |
| statemachine | 0.252 s | 1.401 s | 0.104 s | **2.42x** | 5.55x | 6710880 |

Median ueber alle Benchmarks: **2.65x** langsamer als Rust `-O` (Spanne 1.70x – 4.46x).
Der Optimierer (mem2reg, CSE, Inlining, Registerzuteilung) bringt im Median **11.38x** gegenueber `--no-opt`.
