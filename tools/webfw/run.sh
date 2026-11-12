#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/webfw/run.sh -- lib/webfw against Python requests/http.client (check.py) and mutated requests (fuzz.py).
#
#   bash tools/webfw/run.sh            (FUZZ_RUNS=1500 FUZZ_SEEDS="1 2" to tune)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
rc=0
"$FIRNC" -o "$WORK/app" examples/webfw/app.fi > "$WORK/build.log" 2>&1 || { grep -v RWX "$WORK/build.log" | head; exit 1; }
python3 tools/webfw/check.py "$WORK/app" "$WORK/chk" 2>&1 | tail -5 || rc=1
for seed in ${FUZZ_SEEDS:-1 2}; do
    python3 tools/webfw/fuzz.py "$WORK/app" "$WORK/fz$seed" "$seed" "${FUZZ_RUNS:-1500}" 2>&1 | tail -3 || rc=1
done
exit $rc
