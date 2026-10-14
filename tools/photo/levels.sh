#!/usr/bin/env bash
# Run one test file at the 4 opt levels + aarch64/qemu (release-fast). Usage: tools/photo/levels.sh tests/N_x.fi
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
D="$(mktemp -d)"; trap 'rm -rf "$D"' EXIT
T="$1"; rc=0
for L in no-opt dev-fast release-safe release-fast; do
  if [ $L = no-opt ]; then F=--no-opt; else F=--opt-level=$L; fi
  FIRNLIB="$ROOT/lib" "$FIRNC" $F -o "$D/t_$L" "$T" && out=$("$D/t_$L" 2>&1 | tail -1) || out="FAIL($?)"
  echo "$L: $out"; [ "$out" = ok ] || rc=1
done
FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level=release-fast --target=aarch64-linux -o "$D/t_a64" "$T" && out=$(qemu-aarch64 "$D/t_a64" 2>&1 | tail -1) || out="FAIL($?)"
echo "aarch64: $out"; [ "$out" = ok ] || rc=1
exit $rc
