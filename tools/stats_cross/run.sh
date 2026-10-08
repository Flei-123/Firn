#!/usr/bin/env bash
# tools/stats_cross/run.sh -- lib/std/stats.fi against Python (SKRIPT-LIBS r316).
#
# N random data sets (default 5200; STATS_CROSS_N) in 16 families -- uniform,
# decimals, wide (1e-100..1e100) and tiny/huge magnitudes, denormals,
# ill-conditioned (mean 1e9, spread 1e-3), ties, 1 and 2 elements, constants,
# sorted, byte values (the pixel-count case), cancellation -- go through
# stats_cli (tools/stats_cross/stats_cli.fi) and the answers are compared with
# math.fsum, statistics (median, quantiles, variance, stdev, fmean, mean) and,
# if installed, numpy (median, percentile, var, histogram). The deviation is
# measured in ULP and printed per statistic and family (cross.py documents the
# tolerances). The other three build levels run the SAME input and must give
# byte-identical answers.
#
#   STATS_CROSS_N=1000 STATS_CROSS_SEED=7 bash tools/stats_cross/run.sh
#   STATS_CROSS_FAST=1  -> release-fast only
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
N=${STATS_CROSS_N:-5200}
SEED=${STATS_CROSS_SEED:-20261008}
if ! command -v python3 >/dev/null 2>&1; then
    echo "  SKIP  python3 is not installed"
    exit 0
fi
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-statscross.XXXXXX")
trap 'rm -rf "$W"' EXIT
python3 -I tools/stats_cross/cross.py gen "$N" "$SEED" "$W/in.bin" "$W/classes.txt" || exit 1
ERRORS=0
FIRST=""
STAGES="release-fast:--opt-level=release-fast no-opt:--no-opt dev-fast:--opt-level=dev-fast release-safe:--opt-level=release-safe"
[ "${STATS_CROSS_FAST:-0}" = "1" ] && STAGES="release-fast:--opt-level=release-fast"
for st in $STAGES; do
    name=${st%%:*}; flag=${st#*:}
    if ! $FIRNC $flag -o "$W/cli.$name" tools/stats_cross/stats_cli.fi > "$W/cc.$name" 2>&1; then
        echo "  FAIL  stats_cli does not compile [$name]"; head -5 "$W/cc.$name"; ERRORS=$((ERRORS + 1)); continue
    fi
    if ! "$W/cli.$name" "$W/in.bin" "$W/out.$name"; then
        echo "  FAIL  stats_cli crashed [$name]"; ERRORS=$((ERRORS + 1)); continue
    fi
    if [ -z "$FIRST" ]; then
        FIRST=$name
        python3 -I tools/stats_cross/cross.py check "$W/in.bin" "$W/out.$name" "$W/classes.txt" --strict > "$W/report" 2>&1 \
            && echo "  stats cross-check ok [$name]" \
            || { echo "  FAIL  stats cross-check [$name]"; ERRORS=$((ERRORS + 1)); }
        grep -v '^  HISTOGRAM\|^  variance vs numpy' "$W/report" | tail -48
        grep -E '^  (HISTOGRAM|variance vs numpy|NaN|FORM)' "$W/report" | head -10
    elif cmp -s "$W/out.$FIRST" "$W/out.$name"; then
        echo "  [$name] identical to [$FIRST]"
    else
        echo "  FAIL  [$name] answers differ from [$FIRST]"; ERRORS=$((ERRORS + 1))
    fi
done
[ "$ERRORS" -eq 0 ] && echo "stats_cross: ok" || echo "stats_cross: $ERRORS FAILED"
exit $((ERRORS > 0))
