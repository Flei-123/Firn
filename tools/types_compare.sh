#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/types_compare.sh -- memory layout and calling convention:
# `lib/firnc1/types.fi` against `compiler/src/types.rs` + `abi.rs`.
#
# WHY THIS IS CHECKED SEPARATELY: layout and ABI are the places where
# a compiler goes wrong SILENTLY. One field offset off, one aggregate in
# registers instead of in memory -- the program runs, only wrongly. Putting two
# independent implementations against each other finds more here than any
# invented test case.
#
# What is compared is `firnc0 --emit=layout` against `bin/layoutdump.fi`:
# per struct the size, the alignment and every field offset, per function the
# System V class of every argument and of the return value including `sret`.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${LAYOUTDUMP:-./.layoutdump}

# Rebuild when the dump binary is missing OR sources are younger
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/layoutdump.fi -o "$DUMP" || exit 1
fi

same=0
different=0
noncore=0
skipped=0
with_structs=0
first=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=layout "$f" > "$TMPD"/typv_a.txt 2>/dev/null; then
        skipped=$((skipped+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/typv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        noncore=$((noncore+1))
        continue
    fi
    grep -q '^  (struct' "$TMPD"/typv_a.txt && with_structs=$((with_structs+1))
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/typv_a.txt "$TMPD"/typv_b.txt; then
        same=$((same+1))
    else
        different=$((different+1))
        [ -z "$first" ] && first="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "SAME:          $same"
echo "DIFFERENT:     $different"
echo "WITH STRUCTS:  $with_structs  (a real layout is at stake there)"
echo "NOT CORE:      $noncore"
echo "SKIPPED:       $skipped"
if [ -n "$first" ]; then
    echo "first deviation: $first"
    ff=${first%% *}
    diff <("$FIRNC" --emit=layout "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -12
    exit 1
fi
exit 0
