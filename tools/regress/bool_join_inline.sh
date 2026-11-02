#!/bin/bash
set -u
D="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="${FIRNLIB:-/root/certus-ui/.flib2}"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
rc=0
for m in release-safe release-fast; do
  "$FIRNC" --opt-level="$m" -o "$T/b_$m" "$D/bool_join_inline.fi" || { echo "FAIL build $m"; rc=1; continue; }
  "$T/b_$m"; e=$?
  if [ "$e" = 0 ]; then echo "OK $m"; else echo "FAIL $m exit=$e"; rc=1; fi
done
exit $rc
