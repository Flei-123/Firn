#!/usr/bin/env bash
# tools/lex_compare.sh -- the lexer written in FIRN against the one written
# in RUST, over the whole source corpus, octet for octet.
#
# WHY LIKE THIS: a lexer cannot sensibly be checked against itself.
# `firnc0 --emit=tokens` is an independent implementation in another
# language -- if both outputs agree, that is a real counter-check.
#
# BOTH streams are compared:
#   * standard output = the token stream          (lib/firnc1/lexer.fi)
#   * error output    = the diagnostics with line, column, source line and
#                       marker                    (lib/firnc1/diag.fi)
# That is why `lexdump` gets the FILE NAME as a call argument: it is in
# every diagnostic.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${DUMP:-./.lexdump}

# Rebuild when the dump binary is missing OR sources are younger
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/lexdump.fi -o "$DUMP" || exit 1
fi

# KNOWN DEVIATIONS -- each one named separately, with a reason. This list is
# NO free pass: it stands here so that the number of exceptions stays visible
# and does not grow silently.
#
#   tests/590_f64.fi  ->  the literal `1e308`. The lexer in Firn computes
#   floating point literals outside the fast path of Clinger
#   (|exponent| > 22 and the mantissa does not fit into 2^53) step by step and is
#   off by up to one ULP there. Correct would be Eisel-Lemire with
#   128-bit arithmetic; that is still missing.
BEKANNT="tests/590_f64.fi"

gleich=0
ungleich=0
unerwartet=0
bekannt=0
mit_fehler=0
uebersprungen=0
tokens=0
langsam=0
erste=""

while IFS= read -r f; do
    "$FIRNC" --emit=tokens "$f" > "$TMPD"/lexv_a.txt 2>"$TMPD"/lexv_ae.txt
    # Module fragments (`tests/modules/*.fi`) cannot be compiled
    # separately: `firnc0` already stops in the module resolution, BEFORE the
    # lexer. That is no question of the lexer -- such files are counted and
    # skipped.
    if grep -q "cannot read" "$TMPD"/lexv_ae.txt; then
        uebersprungen=$((uebersprungen+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/lexv_b.txt 2>"$TMPD"/lexv_be.txt
    # The counts are only on the error output when there were NO
    # diagnostics -- otherwise the whole stream belongs to the messages.
    if grep -q '^; tokens ' "$TMPD"/lexv_be.txt; then
        t=$(awk '{print $3}' "$TMPD"/lexv_be.txt)
        g=$(awk '{print $5}' "$TMPD"/lexv_be.txt)
        tokens=$((tokens + ${t:-0}))
        langsam=$((langsam + ${g:-0}))
        : > "$TMPD"/lexv_be.txt
    else
        mit_fehler=$((mit_fehler+1))
    fi
    if cmp -s "$TMPD"/lexv_a.txt "$TMPD"/lexv_b.txt && cmp -s "$TMPD"/lexv_ae.txt "$TMPD"/lexv_be.txt; then
        gleich=$((gleich+1))
    else
        ungleich=$((ungleich+1))
        if echo "$BEKANNT" | tr ' ' '\n' | grep -qxF "$f"; then
            bekannt=$((bekannt+1))
        else
            unerwartet=$((unerwartet+1))
            [ -z "$erste" ] && erste="$f"
        fi
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "GLEICH:        $gleich"
echo "UNGLEICH:      $ungleich   (bekannt und benannt: $bekannt)"
echo "MIT DIAGNOSEN: $mit_fehler  (Fehlerausgabe ebenfalls verglichen)"
echo "UEBERSPRUNGEN: $uebersprungen  (Modulbruchstueck, nicht einzeln uebersetzbar)"
echo "TOKEN GESAMT:  $tokens"
echo "GLEITKOMMA ausserhalb des schnellen Pfades: $langsam"
if [ "$unerwartet" -gt 0 ]; then
    echo "erste unerwartete Abweichung: $erste"
    "$FIRNC" --emit=tokens "$erste" > "$TMPD"/lexv_a.txt 2>"$TMPD"/lexv_ae.txt
    "$DUMP" "$erste" > "$TMPD"/lexv_b.txt 2>"$TMPD"/lexv_be.txt
    diff "$TMPD"/lexv_a.txt "$TMPD"/lexv_b.txt | head -12
    diff "$TMPD"/lexv_ae.txt "$TMPD"/lexv_be.txt | head -20
    exit 1
fi
exit 0
