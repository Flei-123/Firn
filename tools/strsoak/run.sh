#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/strsoak/run.sh -- THE LEAK PROOF for `str` (round 70).
#
# `a + b` on `str` allocates in the GC heap. The endurance run builds many
# short lived strings and measures the REAL memory consumption of the process
# (RSS out of /proc/self/statm) -- not the self-report of the runtime, which
# would otherwise certify its own correctness.
#
# THE COUNTER-CHECK RUNS EVERY TIME: the same loop with the collection
# threshold set so high that no cycle is ever started (`--leak`,
# `gc_set_limit`). If that one stays flat TOO, the measuring method is broken
# and this script fails -- a measurement that cannot show anything would be
# worse than none.
#
# It runs in BOTH compilers: `firnc0` (Rust) and the self-hosting `firnc1`
# have to produce a program that behaves the same.
#
# Environment:
#   STRSOAK_ROUNDS   rounds per run (default 200000, 8 concatenations each)
#   STRSOAK_MAX_KIB  upper limit for the collected run in KiB (default 8192)
#   STRSOAK_MIN_KIB  lower limit for the counter-check in KiB (default 16384)
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
SRC=tools/strsoak/soak.fi
ROUNDS=${STRSOAK_ROUNDS:-200000}
MAX_KIB=${STRSOAK_MAX_KIB:-8192}
MIN_KIB=${STRSOAK_MIN_KIB:-16384}
W=$(mktemp -d /tmp/firn-strsoak.XXXXXX)
trap 'rm -rf "$W"' EXIT
export FIRNLIB="$(pwd)/lib"
ERRORS=0
report() { echo "ERROR: $1"; ERRORS=1; }

field() { sed -n "s/.*$1=\([0-9]*\).*/\1/p" <<< "$2"; }

run_pair() {
    local who="$1" bin="$2"
    local kept leaked
    kept=$("$bin" "$ROUNDS")            || { report "$who: the collected run did not finish"; return; }
    leaked=$("$bin" "$ROUNDS" --leak)   || { report "$who: the counter-check did not finish"; return; }
    echo "  $who collected : $kept"
    echo "  $who leaking   : $leaked"

    local kib_kept kib_leak runs_kept runs_leak built_kept built_leak
    kib_kept=$(field rss_peak_kib "$kept")
    kib_leak=$(field rss_peak_kib "$leaked")
    runs_kept=$(field runs "$kept")
    runs_leak=$(field runs "$leaked")
    built_kept=$(field bytes_built "$kept")
    built_leak=$(field bytes_built "$leaked")

    [ -n "$kib_kept" ] && [ -n "$kib_leak" ] || { report "$who: no measured values"; return; }
    [ "$built_kept" = "$built_leak" ] || report "$who: the two runs did not do the same work"
    [ "$runs_kept" -gt 0 ] || report "$who: the collector never ran -- nothing was proved"
    [ "$runs_leak" -eq 0 ] || report "$who: the counter-check collected after all ($runs_leak runs)"
    [ "$kib_kept" -le "$MAX_KIB" ] || \
        report "$who: WITH the collector the memory grows to $kib_kept KiB (limit $MAX_KIB)"
    [ "$kib_leak" -ge "$MIN_KIB" ] || \
        report "$who: the counter-check stays at $kib_leak KiB -- the measuring method sees nothing"
    if [ "$kib_leak" -le "$((kib_kept * 3))" ]; then
        report "$who: collected ($kib_kept KiB) and leaking ($kib_leak KiB) are too close together"
    fi
}

echo "str leak proof: $ROUNDS rounds x 8 concatenations"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
"$FIRNC" -o "$W/soak0" "$SRC" 2>"$W/e0" || { cat "$W/e0"; echo "firnc0 could not translate the endurance run"; exit 1; }
run_pair "firnc0" "$W/soak0"

# The same program through the self-hosting compiler.
if [ ! -x "$FC1" ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc1 could not be built"; exit 1; }
fi
if "$FC1" "$SRC" -o "$W/soak1" 2>"$W/e1"; then
    run_pair "firnc1" "$W/soak1"
else
    rc=$?
    if [ "$rc" -eq 3 ] || [ "$rc" -eq 4 ] || [ "$rc" -eq 5 ] || [ "$rc" -eq 6 ]; then
        echo "  firnc1: not translatable (rc=$rc) -- outside the ported core"
    else
        report "firnc1 could not translate the endurance run (rc=$rc)"
        cat "$W/e1"
    fi
fi

if [ "$ERRORS" -ne 0 ]; then
    echo "STRSOAK: FAILED"
    exit 1
fi
echo "STRSOAK: ok -- with the collector flat, without it growing"
exit 0
