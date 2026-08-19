#!/usr/bin/env bash
# tools/fir_compare.sh -- the lowering in FIRN against the one in RUST.
#
# The YARDSTICK is `firnc0 --emit=fir-raw`: the intermediate representation DIRECTLY after
# the lowering, without any optimisation. What is compared is the text octet for octet,
# and it contains everything that matters: value numbers, block numbers,
# the order of the instructions, terminators.
#
# That is the sharpest comparison of the whole series. Two value numbers in
# a different order, one block too many or too few -- and the text no longer
# matches.
#
# Return values of `.firdump`:
#   0 output * 1 error * 3 not core language * 4 comptime needed *
#   5 `defer`/`errdefer` (not ported in the lowering yet)
#
# ONLY WHAT `firnc0` can compile SEPARATELY is tested: `--emit=fir-raw`
# otherwise runs over the merged module program, while the Firn way goes
# over a single file -- that would be no comparison but two
# different inputs.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${FIRDUMP:-./.firdump}

# Rebuild when the dump binary is missing OR sources are younger
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/firdump.fi -o "$DUMP" || exit 1
fi

BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
bekannt=0
nichtkern=0
comptime=0
defer_zahl=0
uebersprungen=0
instruktionen=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=typen "$f" >/dev/null 2>&1; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    if ! "$FIRNC" --emit=fir-raw "$f" > "$TMPD"/firv_a.txt 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/firv_b.txt 2>/dev/null
    rc=$?
    case "$rc" in
        3) nichtkern=$((nichtkern+1)); continue;;
        4) comptime=$((comptime+1)); continue;;
        5) defer_zahl=$((defer_zahl+1)); continue;;
    esac
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/firv_a.txt "$TMPD"/firv_b.txt; then
        gleich=$((gleich+1))
        n=$(grep -c '^  ' "$TMPD"/firv_a.txt)
        instruktionen=$((instruktionen + n))
        continue
    fi
    ungleich=$((ungleich+1))
    if echo "$BEKANNT" | tr ' ' '\n' | grep -qxF "$f"; then
        bekannt=$((bekannt+1))
    else
        [ -z "$erste" ] && erste="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "GLEICH:        $gleich"
echo "UNGLEICH:      $ungleich   (bekannt und benannt: $bekannt)"
echo "INSTRUKTIONEN: $instruktionen  (Wertnummern und Bloecke eingeschlossen)"
echo "DEFER:         $defer_zahl  (im Lowering noch nicht portiert)"
echo "NICHT KERN:    $nichtkern"
echo "COMPTIME:      $comptime"
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 uebersetzt die Datei nicht einzeln)"
if [ -n "$erste" ]; then
    echo "erste unerwartete Abweichung: $erste"
    ff=${erste%% *}
    diff <("$FIRNC" --emit=fir-raw "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -12
    exit 1
fi
exit 0
