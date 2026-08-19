#!/usr/bin/env bash
# Benchmark suite: Firn against Rust (rustc -O), same machine, median.
# Usage:  bash bench/run.sh          (5 runs per program)
#          BENCH_RUNS=9 bash bench/run.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --manifest-path compiler/Cargo.toml
exec python3 bench/bench.py
