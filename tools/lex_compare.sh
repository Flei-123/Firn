#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
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
# ROUND 65: THE LIST IS EMPTY. Until then `tests/590_f64.fi` stood here,
# because the lexer in Firn computed floating point literals outside
# Clinger's fast path step by step and was up to one ULP off there. It no
# longer computes them step by step: `float_exact` in lib/firnc1/lexer.fi
# holds the value as a fraction of two big integers and rounds it correctly.
# tools/lexnum/run.sh proves that over several thousand literals.
KNOWN=""

same=0
different=0
unexpected=0
known=0
with_diag=0
skipped=0
tokens=0
slow=0
first=""

while IFS= read -r f; do
    "$FIRNC" --emit=tokens "$f" > "$TMPD"/lexv_a.txt 2>"$TMPD"/lexv_ae.txt
    # Module fragments (`tests/modules/*.fi`) cannot be compiled
    # separately: `firnc0` already stops in the module resolution, BEFORE the
    # lexer. That is no question of the lexer -- such files are counted and
    # skipped.
    if grep -q "cannot read" "$TMPD"/lexv_ae.txt; then
        skipped=$((skipped+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/lexv_b.txt 2>"$TMPD"/lexv_be.txt
    # The counts are only on the error output when there were NO
    # diagnostics -- otherwise the whole stream belongs to the messages.
    if grep -q '^; tokens ' "$TMPD"/lexv_be.txt; then
        t=$(awk '{print $3}' "$TMPD"/lexv_be.txt)
        g=$(awk '{print $5}' "$TMPD"/lexv_be.txt)
        tokens=$((tokens + ${t:-0}))
        slow=$((slow + ${g:-0}))
        : > "$TMPD"/lexv_be.txt
    else
        with_diag=$((with_diag+1))
    fi
    if cmp -s "$TMPD"/lexv_a.txt "$TMPD"/lexv_b.txt && cmp -s "$TMPD"/lexv_ae.txt "$TMPD"/lexv_be.txt; then
        same=$((same+1))
    else
        different=$((different+1))
        if echo "$KNOWN" | tr ' ' '\n' | grep -qxF "$f"; then
            known=$((known+1))
        else
            unexpected=$((unexpected+1))
            [ -z "$first" ] && first="$f"
        fi
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "SAME:          $same"
echo "DIFFERENT:     $different   (known and named: $known)"
echo "WITH DIAGNOSTICS: $with_diag  (the error output is compared as well)"
echo "SKIPPED:       $skipped  (module fragment, not compilable on its own)"
echo "TOKENS TOTAL:  $tokens"
echo "FLOATING POINT outside the fast path: $slow"
if [ "$unexpected" -gt 0 ]; then
    echo "first unexpected deviation: $first"
    "$FIRNC" --emit=tokens "$first" > "$TMPD"/lexv_a.txt 2>"$TMPD"/lexv_ae.txt
    "$DUMP" "$first" > "$TMPD"/lexv_b.txt 2>"$TMPD"/lexv_be.txt
    diff "$TMPD"/lexv_a.txt "$TMPD"/lexv_b.txt | head -12
    diff "$TMPD"/lexv_ae.txt "$TMPD"/lexv_be.txt | head -20
    exit 1
fi
exit 0
