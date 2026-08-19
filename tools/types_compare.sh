#!/usr/bin/env bash
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

gleich=0
ungleich=0
nichtkern=0
uebersprungen=0
mit_structs=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=layout "$f" > "$TMPD"/typv_a.txt 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/typv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        nichtkern=$((nichtkern+1))
        continue
    fi
    grep -q '^  (struct' "$TMPD"/typv_a.txt && mit_structs=$((mit_structs+1))
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/typv_a.txt "$TMPD"/typv_b.txt; then
        gleich=$((gleich+1))
    else
        ungleich=$((ungleich+1))
        [ -z "$erste" ] && erste="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "GLEICH:        $gleich"
echo "UNGLEICH:      $ungleich"
echo "MIT STRUCTS:   $mit_structs  (dort steht ein echtes Layout auf dem Spiel)"
echo "NICHT KERN:    $nichtkern"
echo "UEBERSPRUNGEN: $uebersprungen"
if [ -n "$erste" ]; then
    echo "erste Abweichung: $erste"
    ff=${erste%% *}
    diff <("$FIRNC" --emit=layout "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -12
    exit 1
fi
exit 0
