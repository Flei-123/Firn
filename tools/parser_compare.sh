#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/parser_compare.sh -- the parser written in FIRN against the one
# written in RUST, over the whole source corpus.
#
# The YARDSTICK is `firnc0 --emit=ast-canon`: a language-neutral, parenthesised
# form of the syntax tree (compiler/src/ast_canon.rs). Two independent parsers
# produce the same text exactly when they have built the same tree.
#
# Return values of `.astdump`:
#   0  output produced
#   1  syntax error
#   3  the file uses an extension the core parser does not know
#      (`enum`/`match`, error unions, generics, `gc class`, attributes,
#      `comptime`) -- such files are COUNTED, not passed over.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FIRNC=compiler/target/release/firnc
DUMP=${ASTDUMP:-./.astdump}

# Rebuild when the dump binary is missing OR sources are younger
if [ ! -x "$DUMP" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$DUMP" -print -quit)" ]; then
    rm -f "$DUMP"
    "$FIRNC" bin/astdump.fi -o "$DUMP" || exit 1
fi

# KNOWN DEVIATION -- named separately:
#   tests/590_f64.fi  ->  the literal `1e308`. That is NO parser error
#   but the known floating point rounding case from round 20
#   (tools/lex_compare.sh); the value is already wrong in the token.
#   tests/911_css_parser.fi  ->  a GENERIC CALL whose type argument is a
#   user defined name (`gc_null[Cv]()`). `.astdump` runs the parser WITHOUT
#   the generic table (`par_gen_set` is not called there), and instead of
#   reporting "not core language" (return value 3) it reports a syntax
#   error (return value 1). Minimal reproduction:
#
#       fn main() -> i32 {
#           let a: i32 = gc_null[Cv]()
#           return a
#       }
#
#   The same file with `u32` instead of `Cv` goes through. This is NOT a
#   consequence of the reformatting of round 64: the version out of the base
#   commit a2a2ed4 fails in exactly the same way. It only became visible
#   because this script had been calling `--emit=ast-kanon` since the
#   English migration -- an option that no longer exists, so it compared
#   NOTHING and reported zeros (round 64).
KNOWN="tests/590_f64.fi tests/911_css_parser.fi"

same=0
different=0
known=0
noncore=0
skipped=0
first=""

while IFS= read -r f; do
    if ! "$FIRNC" --emit=ast-canon "$f" > "$TMPD"/parv_a.txt 2>/dev/null; then
        # firnc0 does not get through itself (module fragment, negative test).
        skipped=$((skipped+1))
        continue
    fi
    "$DUMP" "$f" > "$TMPD"/parv_b.txt 2>/dev/null
    rc=$?
    if [ "$rc" -eq 3 ]; then
        noncore=$((noncore+1))
        continue
    fi
    if [ "$rc" -eq 0 ] && cmp -s "$TMPD"/parv_a.txt "$TMPD"/parv_b.txt; then
        same=$((same+1))
        continue
    fi
    different=$((different+1))
    if echo "$KNOWN" | tr ' ' '\n' | grep -qxF "$f"; then
        known=$((known+1))
    else
        [ -z "$first" ] && first="$f (rc=$rc)"
    fi
done < <(find tests lib bin bench -name '*.fi' -not -type l | sort)

echo "SAME:          $same"
echo "DIFFERENT:     $different   (known and named: $known)"
echo "NOT CORE:      $noncore  (enum/match, error unions, generics, gc, attributes, comptime)"
echo "SKIPPED:       $skipped  (firnc0 does not get through itself)"
if [ -n "$first" ]; then
    echo "first unexpected deviation: $first"
    ff=${first%% *}
    "$FIRNC" --emit=ast-canon "$ff" > "$TMPD"/parv_a.txt 2>/dev/null
    "$DUMP" "$ff" > "$TMPD"/parv_b.txt 2>/dev/null
    diff "$TMPD"/parv_a.txt "$TMPD"/parv_b.txt | head -10
    exit 1
fi
exit 0
