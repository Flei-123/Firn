#!/usr/bin/env bash
# compare.sh <dump_a> <dump_b> [label_a=A] [label_b=B]
#
# Compares two canonical mcclient dumps (demos/mcclient/DESIGN.md) and reports
#   * full file equality (byte exact, includes the checksum line)
#   * stability of the stable part (header + C/S/H lines) and of the L (light) lines SEPARATELY
#     so that a flaky L is visible and never hidden.
# On a difference the first differing lines are printed.
# Exit code: 0 = byte identical, 1 = different, 2 = usage / missing file.
set -uo pipefail

A="${1:?usage: compare.sh <dump_a> <dump_b> [label_a] [label_b]}"
B="${2:?usage: compare.sh <dump_a> <dump_b> [label_a] [label_b]}"
LA="${3:-A}"
LB="${4:-B}"

for f in "$A" "$B"; do
  [ -f "$f" ] || { echo "compare: missing file $f" >&2; exit 2; }
done

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

# split: stable lines (everything except L and checksum) and L lines
for k in a b; do
  src="$A"; [ "$k" = b ] && src="$B"
  grep -v -E '^(L |checksum )' "$src" > "$T/$k.stable" || true
  grep -E '^L ' "$src" > "$T/$k.light" || true
done

full=1; stable=1; light=1
cmp -s "$A" "$B" && full=0
cmp -s "$T/a.stable" "$T/b.stable" && stable=0
cmp -s "$T/a.light" "$T/b.light" && light=0

ca=$(grep '^checksum' "$A" | tail -1 | cut -d' ' -f3)
cb=$(grep '^checksum' "$B" | tail -1 | cut -d' ' -f3)

verdict() { [ "$1" -eq 0 ] && echo "EQUAL" || echo "DIFFERENT"; }
echo "compare $LA vs $LB:"
echo "  full file (byte exact) : $(verdict $full)"
echo "  header+C+S+H lines     : $(verdict $stable)"
echo "  L (light) lines        : $(verdict $light)"
echo "  checksum $LA=${ca:-none} $LB=${cb:-none}"

if [ "$stable" -ne 0 ]; then
  echo "  first differences in C/S/H part (< $LA, > $LB):"
  diff "$T/a.stable" "$T/b.stable" | head -12 | sed 's/^/    /'
fi
if [ "$light" -ne 0 ]; then
  nl=$(diff "$T/a.light" "$T/b.light" | grep -c '^<')
  echo "  L lines differing: $nl of $(wc -l < "$T/a.light")"
  diff "$T/a.light" "$T/b.light" | head -8 | sed 's/^/    /'
fi
if [ "$full" -ne 0 ] && [ "$stable" -eq 0 ] && [ "$light" -eq 0 ]; then
  echo "  only the tail differs (checksum / trailing bytes):"
  diff "$A" "$B" | head -6 | sed 's/^/    /'
fi
[ "$full" -eq 0 ]
