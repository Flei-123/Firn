#!/usr/bin/env bash
# tools/mc/light/run.sh -- cross-check of lib/mc/mc_light.fi (incremental light) against the full recompute:
# tests/2831 (random edit sequences, 6 seeds, in 3 build stages: incremental == full oracle, dirty sections cover every
# changed padded section, hand-computed values), a counter-check (a damaged removal rule must be detected) and the speed
# metrics (full recompute, single edits). Usage: bash tools/mc/light/run.sh [firnc-binary]
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    if ! $FIRNC $stage -o "$W/t.$name" tests/2831_mc_light_fuzz.fi 2>"$W/err"; then
        echo "  FAIL  $name: does not compile"; head -5 "$W/err"; ERR=1; continue
    fi
    "$W/t.$name"; rc=$?
    if [ $rc -eq 0 ]; then echo "  ok    $name: fuzz incremental == full recompute"; else echo "  FAIL  $name: exit $rc"; ERR=1; fi
done
# counter-check: removal pass skips cells one below the removed value -> stale light remains -> the oracle must see it
mkdir -p "$W/badlib" && cp -r lib/. "$W/badlib/"
sed -i 's/if ln < lp \&\& ln > b {/if ln < lp \&\& ln > b + 1 as u8 {/' "$W/badlib/mc/mc_light.fi"
if cmp -s lib/mc/mc_light.fi "$W/badlib/mc/mc_light.fi"; then
    echo "  FAIL  counter-check: the damage did not apply"; ERR=1
elif FIRNLIB="$W/badlib" $FIRNC -o "$W/t.bad" tests/2831_mc_light_fuzz.fi 2>/dev/null; then
    if "$W/t.bad"; then echo "  FAIL  counter-check: a damaged removal rule passed"; ERR=1
    else echo "  ok    counter-check: a damaged removal rule is detected"; fi
else
    echo "  FAIL  counter-check build"; ERR=1
fi
if $FIRNC -o "$W/bench" tools/mc/light/bench.fi 2>"$W/err"; then
    "$W/bench" | sed 's/^/  /'
else
    echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; ERR=1
fi
[ $ERR -eq 0 ] && echo "RESULT mc-light: ok" || echo "RESULT mc-light: FAIL"
