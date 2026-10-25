#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for STAGE in "opt:" "dev:--opt-level=dev-fast"; do
    NAME=${STAGE%%:*}
    OPT=${STAGE#*:}
    "$FIRNC" $OPT -o "$WORK/ssdp_$NAME" lib/ssdp/ssdp_main.fi 2> "$WORK/b.log" || { grep -v RWX "$WORK/b.log" | head -5; exit 1; }
    python3 -I tools/ssdp/check.py "$WORK/ssdp_$NAME" > "$WORK/c_$NAME.log" 2>&1 || { grep -v "^  ok" "$WORK/c_$NAME.log"; rc=1; }
    tail -1 "$WORK/c_$NAME.log"
done
exit $rc
