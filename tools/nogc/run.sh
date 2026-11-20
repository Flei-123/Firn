#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -u
cd "$(dirname "$0")/../.."
ROOT=$PWD
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
pass=0
fail=0

check() {
  if [ "$2" = ok ]; then pass=$((pass + 1)); else fail=$((fail + 1)); echo "FAIL $1: $3"; fi
}

for src in tools/nogc/cases/*.fi; do
  name=$(basename "$src" .fi)
  want=$(sed -n 's|^// expect_out: ||p' "$src" | head -1)
  if ! "$FIRNC" --no-gc --no-cache "$src" -o "$D/$name" >"$D/$name.log" 2>&1; then
    check "$name" bad "compile: $(head -3 "$D/$name.log" | tr '\n' ' ')"
    continue
  fi
  got=$("$D/$name" | tr '\n' ' ' | sed 's/ $//')
  [ "$got" = "$want" ] && check "$name out" ok || check "$name out" bad "got '$got' want '$want'"
  syms=$(nm "$D/$name" | grep -c 'gc_init\|__gc_')
  [ "$syms" = 0 ] && check "$name no-gc-symbols" ok || check "$name no-gc-symbols" bad "$syms gc symbols"
  if "$FIRNC" --no-cache "$src" -o "$D/$name.gc" >/dev/null 2>&1; then
    gcgot=$("$D/$name.gc" | tr '\n' ' ' | sed 's/ $//')
    [ "$gcgot" = "$got" ] && check "$name same-as-gc" ok || check "$name same-as-gc" bad "gc '$gcgot'"
  fi
done

for src in tools/nogc/neg/*.fi; do
  name=$(basename "$src" .fi)
  out=$("$FIRNC" --no-gc --no-cache "$src" -o "$D/neg_$name" 2>&1)
  rc=$?
  if [ $rc -ne 0 ] && echo "$out" | grep -q "which '--no-gc' leaves out"; then
    check "neg $name" ok
  else
    check "neg $name" bad "rc=$rc $(echo "$out" | head -2 | tr '\n' ' ')"
  fi
done

for mode in off on check; do
  out=$("$FIRNC" --no-gc --ownership=$mode --no-cache tools/nogc/cases/hello.fi -o "$D/own_$mode" 2>&1)
  rc=$?
  if [ $rc -ne 0 ] && echo "$out" | grep -q "requires --ownership=strict"; then
    check "ownership=$mode refused" ok
  else
    check "ownership=$mode refused" bad "rc=$rc"
  fi
done

out=$("$FIRNC" --no-gc --no-cache tools/nogc/neg/gc_class.fi -o "$D/pos" 2>&1)
echo "$out" | grep -q 'gc_class.fi:1:1' && check "neg position" ok || check "neg position" bad "$(echo "$out" | head -2 | tr '\n' ' ')"

echo "nogc: $pass passed, $fail failed"
[ "$fail" = 0 ]
