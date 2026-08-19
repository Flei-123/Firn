#!/usr/bin/env bash
# tools/sema_compare.sh -- the type checker in FIRN against the one in RUST.
#
# The YARDSTICK is `firnc0 --emit=typen`: the canonical syntax tree with the TYPE at
# every expression. `firnc0` promises that after the check every expression
# has a concrete type -- exactly this promise is compared here.
#
# Return values of `.semadump`:
#   0  output produced
#   1  error
#   3  not core language (enum/match, error unions, generics, gc, attributes,
#      comptime, the intrinsics for constant run time)
#   4  a constant needs evaluation at compile time (comptime.rs)
#
# SKIPPED are files that `firnc0` ITSELF cannot check separately
# -- almost all of them import a module whose names are unknown on their own.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${SEMADUMP:-./.semadump}

if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/semadump.fi -o "$DUMP" || exit 1
fi

# KNOWN DEVIATION: tests/590_f64.fi, the literal `1e308`. No type error
# but the floating point rounding case from round 20 -- the value is already
# wrong in the token.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
bekannt=0
nichtkern=0
comptime=0
uebersprungen=0
ausdruecke=0
erste=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=typen "$f" > "$TMPD"/semv_a.txt 2>/dev/null; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/semv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        nichtkern=$((nichtkern+1))
        continue
    fi
    if [ "$rc" -eq 4 ]; then
        comptime=$((comptime+1))
        continue
    fi
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/semv_a.txt "$TMPD"/semv_b.txt; then
        gleich=$((gleich+1))
        # Every " :" is a typed expression.
        n=$(grep -o ' :' "$TMPD"/semv_a.txt | wc -l)
        ausdruecke=$((ausdruecke + n))
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
echo "AUSDRUECKE:    $ausdruecke  (jeder mit demselben Typ wie in firnc0)"
echo "NICHT KERN:    $nichtkern"
echo "COMPTIME:      $comptime  (konstante Auswertung zur Uebersetzungszeit, nicht portiert)"
echo "UEBERSPRUNGEN: $uebersprungen  (firnc0 prueft die Datei nicht einzeln)"
if [ -n "$erste" ]; then
    echo "erste unerwartete Abweichung: $erste"
    ff=${erste%% *}
    diff <("$FIRNC" --emit=typen "$ff" 2>/dev/null) <("$DUMP" "$ff" 2>/dev/null) | head -6
    exit 1
fi
exit 0
