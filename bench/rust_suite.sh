#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
[ -x "${FIRNC_BIN:-compiler/target/release/firnc}" ] || cargo build --release --manifest-path compiler/Cargo.toml
exec python3 bench/rust_suite.py "$@"
