#!/usr/bin/env bash
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
LEVELS="${LEVELS:-release-safe dev-fast}"
D="$(mktemp -d)"
trap 'rm -rf "$D"' EXIT
fail=0
total=0
for f in "$ROOT"/tests/[0-9]*.fi "$ROOT"/examples/*.fi "$ROOT"/bin/firnc1.fi; do
  for lvl in $LEVELS; do
    FIRN_CODEGEN_THREADS=1 FIRN_NO_CACHE=1 "$FIRNC" "$f" --emit=asm --opt-level="$lvl" -o "$D/a.s" >/dev/null 2>&1 || continue
    FIRN_CODEGEN_THREADS=8 FIRN_CODEGEN_MIN_FUNCS=1 FIRN_NO_CACHE=1 "$FIRNC" "$f" --emit=asm --opt-level="$lvl" -o "$D/b.s" >/dev/null 2>&1 || { echo "FAIL parallel run: $f $lvl"; fail=$((fail + 1)); continue; }
    total=$((total + 1))
    cmp -s "$D/a.s" "$D/b.s" || { echo "FAIL differs: $f $lvl"; fail=$((fail + 1)); }
  done
done
echo "parcodegen: $total compared, $fail failed"
[ "$fail" -eq 0 ]
