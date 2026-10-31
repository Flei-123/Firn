#!/bin/bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$ROOT/lib}
C=$ROOT/tools/ownership/cases
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
fail=0
pass=0
build() {
  "$FIRNC" --no-cache --ownership="$2" "$C/$1.fi" -o "$D/$1_$2" > "$D/$1_$2.log" 2>&1 || { echo "FAIL build $1 $2"; sed -n 1,5p "$D/$1_$2.log"; fail=$((fail + 1)); return 1; }
}
run() {
  (cd "$D" && timeout 60 "./$1_$2" > "$D/$1_$2.out" 2> "$D/$1_$2.err"); echo $?
}
expect_rc() {
  build "$1" "$2" || return
  local rc
  rc=$(run "$1" "$2")
  if [ "$rc" = "$3" ]; then pass=$((pass + 1)); else echo "FAIL $1 $2: rc $rc, want $3"; fail=$((fail + 1)); fi
}
same_out() {
  local ref=""
  for m in off on check; do
    build "$1" "$m" || return
    run "$1" "$m" > /dev/null
  done
  for m in on check; do
    if cmp -s "$D/$1_off.out" "$D/$1_$m.out"; then pass=$((pass + 1)); else echo "FAIL $1: output $m differs from off"; fail=$((fail + 1)); fi
  done
}
same_out concat_chain
expect_rc free_reuse on 0
expect_rc free_reuse check 0
expect_rc poison_hit check 86
grep -q "use after free" "$D/poison_hit_check.err" && pass=$((pass + 1)) || { echo "FAIL poison_hit: no message"; fail=$((fail + 1)); }
echo "ownership: $pass passed, $fail failed"
[ "$fail" = 0 ]
