#!/usr/bin/env bash
# Benchmark-Suite: Firn gegen Rust (rustc -O), gleicher Rechner, Median.
# Aufruf:  bash bench/run.sh          (5 Laeufe je Programm)
#          BENCH_RUNS=9 bash bench/run.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --manifest-path compiler/Cargo.toml
exec python3 bench/bench.py
