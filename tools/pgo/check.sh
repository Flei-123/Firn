#!/bin/bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
pass=0
check() {
  if [ "$1" = "$2" ]; then pass=$((pass + 1)); else echo "FAIL $3: got '$1' want '$2'"; fail=$((fail + 1)); fi
}
for src in bench/firn/branchy.fi bench/firn/statemachine.fi bench/firn_vs_c/firn/qsort.fi bench/firn_vs_c/firn/crc32.fi; do
  n=$(basename "$src" .fi)
  cp "$ROOT/$src" "$D/$n.fi"
  (cd "$D" && $FIRNC --no-cache --opt-level=release-fast "$n.fi" -o "$n.plain" 2>/dev/null)
  (cd "$D" && $FIRNC --no-cache --opt-level=release-fast --profile-gen "$n.fi" -o "$n.gen" 2>/dev/null)
  "$D/$n.plain" > "$D/$n.o1"; r1=$?
  (cd /; "$D/$n.gen" > "$D/$n.o2"); r2=$?
  check "$r2" "$r1" "$n gen exit"
  check "$(cmp -s "$D/$n.o1" "$D/$n.o2" && echo same)" same "$n gen output"
  check "$(head -c 9 "$D/$n.gen.fprof" 2>/dev/null)" "FIRNPGO 1" "$n profile written"
  log=$(cd "$D" && FIRN_PGO_LOG=1 $FIRNC --no-cache --opt-level=release-fast --profile-use="$n.gen.fprof" "$n.fi" -o "$n.use" 2>&1)
  check "$(echo "$log" | grep -c ', 0 stale')" 1 "$n profile fresh"
  "$D/$n.use" > "$D/$n.o3"; r3=$?
  check "$r3" "$r1" "$n use exit"
  check "$(cmp -s "$D/$n.o1" "$D/$n.o3" && echo same)" same "$n use output"
  printf '\nfn pgo_stale_marker() -> i32 {\n    return 7\n}\n' >> "$D/$n.fi"
  log=$(cd "$D" && FIRN_PGO_LOG=1 $FIRNC --no-cache --opt-level=release-fast --profile-use="$n.gen.fprof" "$n.fi" -o "$n.stale" 2>&1)
  check "$(echo "$log" | grep -c ' 0 source positions')" 1 "$n changed file dropped"
done
printf 'FIRNPGO 9\nEND\n' > "$D/bad.fprof"
cp "$ROOT/bench/firn/branchy.fi" "$D/v.fi"
log=$(cd "$D" && $FIRNC --no-cache --opt-level=release-fast --profile-use=bad.fprof v.fi -o v 2>&1)
check "$(echo "$log" | grep -c 'version 9 not supported')" 1 "version check"
log=$(cd "$D" && $FIRNC --no-cache --target=aarch64 --profile-gen v.fi -o v 2>&1)
check "$(echo "$log" | grep -c 'only x86-64')" 1 "gen target check"
echo "pgo checks: $pass passed, $fail failed"
[ $fail -eq 0 ]
