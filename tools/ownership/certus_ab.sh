#!/bin/bash
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIRNC="${FIRNC:-/root/firnc-gc/compiler/target/release/firnc}"
CERTUS="${CERTUS:-/root/certus-ui}"
OUT="${OUT:?set OUT=<result dir>}"
REPS="${REPS:-1}"
REP_START="${REP_START:-0}"
MODES="${MODES:-off on check}"
BASE_FIRNC="${BASE_FIRNC:-}"
STATS="${STATS:-0}"
ONLY="${ONLY:-}"
mkdir -p "$OUT/bin"
args=()
if [ -n "$BASE_FIRNC" ]; then
  [ -x "$OUT/bin/js_base" ] || (cd "$CERTUS" && FIRNC="$BASE_FIRNC" bash tools/work/mk.sh release-safe "$OUT/bin/js_base" lib/js/run_main.fi) || exit 1
  args+=(--bin "base=$OUT/bin/js_base")
fi
for m in $MODES; do
  [ -x "$OUT/bin/js_$m" ] || (cd "$CERTUS" && FIRNC="$FIRNC" bash tools/work/mk.sh release-safe "$OUT/bin/js_$m" lib/js/run_main.fi --ownership="$m") || exit 1
  args+=(--bin "$m=$OUT/bin/js_$m")
done
[ "$STATS" = 1 ] && args+=(--stats)
[ -n "$ONLY" ] && args+=(--only "$ONLY")
exec python3 -I "$HERE/certus_ab.py" "${args[@]}" --out "$OUT" --reps "$REPS" --rep-start "$REP_START"
