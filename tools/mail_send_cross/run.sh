#!/usr/bin/env bash
# tools/mail_send_cross/run.sh -- cross-check of lib/mail/mail.fi (blocking send) against Python's email package,
# in the four build levels. Needs python3. Exit 0 = no mismatch.
#   bash tools/mail_send_cross/run.sh [cases]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
[ -x "$FIRNC" ] || FIRNC="${FIRN:-/root/firn}/compiler/target/release/firnc"
export FIRNLIB="${FIRNLIB:-$ROOT/lib}"
CASES="${1:-60}"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
bad=0
for flags in "--no-opt" "--opt-level=dev-fast" "--opt-level=release-fast" "--opt-level=release-safe"; do
    "$FIRNC" $flags tools/mail_send_cross/sendtool.fi -o "$D/sendtool" || { echo "FAIL build $flags"; bad=1; continue; }
    out=$(python3 tools/mail_send_cross/mail_send_cross.py "$D/sendtool" "$CASES" 7 2>&1 | tail -3)
    echo "$flags: $out"
    echo "$out" | grep -q "mismatches 0" || bad=1
done
[ $bad = 0 ] && echo "mail_send_cross: OK" || echo "mail_send_cross: FAIL"
exit $bad
