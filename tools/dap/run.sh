#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-compiler/target/release/firnc}"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
python3 -I tools/dap/test_dap.py "$FIRNC" docs/gdb_example.fi "$WORK"
