#!/usr/bin/env bash
# tools/perf/rf_repro.sh -- reproduce the release-fast regression (docs/RELEASE-FAST-REGRESSION.md).
# Usage: bash tools/perf/rf_repro.sh [firnc-binary]     (> 1 min: run it as `/root/jarvis/bin/heavy bash tools/perf/rf_repro.sh`)
# Prints per build level: whether the hot function fell out of the register allocator (RA-BASE line),
# the frame size, and the min user time of 5 INTERLEAVED runs. The checksum must be equal everywhere.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
build() { # label, flags...
    local l=$1; shift
    FIRN_RA_WARN=1 "$FIRNC" "$@" -o "$W/$l" tools/perf/rf_repro.fi 2>&1 | grep "RA-BASE shade" | sed "s/^/  [$l] /"
    "$FIRNC" "$@" --emit=asm -o "$W/$l.s" tools/perf/rf_repro.fi 2>/dev/null
    echo "  [$l] frame: $(awk '/^_F0.shade:/{f=1} f&&/sub rsp/{print $3; exit}' "$W/$l.s") bytes"
}
build dev-fast --opt-level=dev-fast
build release-safe --opt-level=release-safe
build release-fast --opt-level=release-fast
build fast-noinline --opt-level=release-fast --no-pass=inline
for r in 1 2 3 4 5; do for l in dev-fast release-safe release-fast fast-noinline; do
    /usr/bin/time -f "$l %U" "$W/$l" 2>&1 >"$W/out.$l" | tail -1
done; done | sort -k1,1 -k2,2n | awk '!s[$1]++{printf "RESULT %-16s min user %s s\n",$1,$2}'
for l in dev-fast release-safe release-fast fast-noinline; do cat "$W/out.$l"; done | sort | uniq -c
