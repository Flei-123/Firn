#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
WORK="${WORK:?set WORK to a scratch directory}"
PY="${PY:-python3}"
LIMIT="${LIMIT:-400}"
MIN_COUNT="${MIN_COUNT:-3}"
BUDGET="${BUDGET:-4}"
export OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1
bash "$HERE/corpus.sh" "$WORK/corpus"
"$PY" "$HERE/mine.py" "$WORK/windows.json" "$WORK"/corpus/*.s
"$PY" "$HERE/search.py" "$WORK/windows.json" "$WORK/found.json" "$LIMIT" "$MIN_COUNT" "$BUDGET"
"$PY" "$HERE/emit.py" "$WORK/found.json" "$ROOT/compiler/src/superopt/rules.rs" \
    "$ROOT/compiler/src/superopt/rule_tests.rs" "$HERE/rules.json"
