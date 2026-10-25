#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
export FIRNLIB="$PWD/lib"
bad=0
for n in av1sym av1cdef av1lr av1recon av1txfm; do
  "$FIRNC" --opt-level=release-safe -o "$D/$n" "tools/av1/unit/${n}_main.fi" >"$D/$n.log" 2>&1 || { tail -5 "$D/$n.log"; echo "unit_tests: build $n FAILED"; exit 1; }
done
python3 -I tools/av1/unit/symtest.py "$D/av1sym" lib/av1/avif_cdf.fi || bad=$((bad + 1))
python3 -I tools/av1/unit/cdef_ref.py "$D/av1cdef" "${CDEF_SEEDS:-1}" || bad=$((bad + 1))
python3 -I tools/av1/unit/lr_ref.py "$D/av1lr" "${LR_SEEDS:-1}" || bad=$((bad + 1))
python3 -I tools/av1/unit/recontest.py "$D/av1recon" || bad=$((bad + 1))
python3 -I tools/av1/unit/txtest.py "$D/av1txfm" || bad=$((bad + 1))
[ $bad -eq 0 ] && echo "unit_tests: ok" || echo "unit_tests: FAILED ($bad)"
[ $bad -eq 0 ]
