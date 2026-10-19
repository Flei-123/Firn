#!/usr/bin/env bash
# Firn vs C benchmark suite. Always via the heavy wrapper:
#   /root/jarvis/bin/heavy bash bench/firn_vs_c/run.sh
# Env: BENCH_RUNS (5), ONLY=a,b, FIRNC_BIN, OUT (results.json)
set -euo pipefail
cd "$(dirname "$0")/../.."
[ -x compiler/target/release/firnc ] || cargo build --release --manifest-path compiler/Cargo.toml
exec python3 bench/firn_vs_c/run.py
