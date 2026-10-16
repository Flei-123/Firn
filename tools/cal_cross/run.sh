#!/usr/bin/env bash
# tools/cal_cross/run.sh -- cal.rrule against dateutil.rrule (rule + rruleset cases).
#   CAL_CASES=1500 CAL_SEEDS="1 2 3 4" bash tools/cal_cross/run.sh      (python3 + dateutil needed)
#   CAL_STAGES="--opt-level=dev-fast --opt-level=release-safe"  limits the compiler stages (--no-opt is slow)
#   CAL_FUZZ=40000 CAL_FUZZ_SEEDS="1 2 3"  sets the fuzz size (tests/3604 in release-safe)
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
CASES=${CAL_CASES:-1500}
SEEDS=${CAL_SEEDS:-"1 2 3 4"}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-cal-cross.XXXXXX")
trap 'rm -rf "$W"' EXIT
export FIRNLIB="$ROOT/lib"
for s in $SEEDS; do python3 tools/cal_cross/gen.py "$s" "$CASES" "$W/c.$s" > "$W/g.$s" & done
wait
cat "$W"/g.* | sed 's/^/  /'
ERR=0; TOT=0; DIF=0
for stage in ${CAL_STAGES:-"--opt-level=release-fast --no-opt --opt-level=dev-fast --opt-level=release-safe"}; do
  $FIRNC $stage -o "$W/chk" tools/cal_cross/check.fi || { echo "  FAIL compile $stage"; ERR=1; continue; }
  for s in $SEEDS; do
    out=$("$W/chk" "$W/c.$s" 2>&1); rc=$?
    echo "  $stage seed $s: $(echo "$out" | tail -1)"
    [ $rc -ne 0 ] && { ERR=1; echo "$out" | head -8; }
    TOT=$((TOT + $(echo "$out" | tail -1 | sed -n 's/^cal cross-check: \([0-9]*\) cases.*/\1/p')))
    DIF=$((DIF + $(echo "$out" | tail -1 | sed -n 's/.* \([0-9]*\) differences$/\1/p')))
  done
done
echo "cal_cross: $TOT comparisons, $DIF differences"
# fuzz on damaged calendars / cards / rules (tests/3604), release-safe so overflow and bounds panics count
$FIRNC --opt-level=release-safe -o "$W/fuzz" tests/3604_cal_fuzz.fi || { echo "  FAIL compile fuzz"; ERR=1; }
for s in ${CAL_FUZZ_SEEDS:-1 2 3}; do
  out=$(timeout 900 "$W/fuzz" ${CAL_FUZZ:-40000} "$s" 2>&1 | tail -1); rc=$?
  echo "  fuzz seed $s: $out"
  case "$out" in *"unstable writes 0, rule failures 0") ;; *) ERR=1;; esac
done
[ $ERR -eq 0 ] && echo "ALL OK" || { echo "FAILED"; exit 1; }
