#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/fmt/run.sh -- THE PROOF FOR firnfmt (round 64, point 1).
#
# Six steps, every one of them with a number at the end. Nothing is
# filtered, nothing is skipped quietly.
#
#   1. build `firnfmt` -- with `firnc0` AND with `firnc1`, so that the
#      formatter is not just a Firn program on paper.
#   2. the whole tree in a copy: every `.fi` file gets formatted, and for
#      every one of them three things have to hold
#        a) the TOKEN STREAM stays the same (`firnc0 --emit=tokens`)
#        b) the SYNTAX TREE stays the same (`firnc0 --emit=ast-canon`)
#        c) a second run changes NOTHING (idempotence)
#   3. the tree in the repository is IN canonical shape (`firnfmt -c`)
#   4. the random test: `tools/fmt/mutate.py` scrambles the blanks of a
#      source text without touching a token, and firnfmt has to produce the
#      very same text out of the scrambled version as out of the original.
#      Plus the tree comparison the round asked for: format, parse, format
#      again -- the tree has to be equal before and after.
#   5. the pinned cases in tools/fmt/cases/ (input -> expected shape)
#   6. counter-checks: a source text that does not scan has to be REFUSED,
#      and `-c` has to STRIKE on a file that is not formatted. Without
#      those two the whole proof would be worthless.
#
# Usage:  bash tools/fmt/run.sh [--fast]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC="$ROOT/compiler/target/release/firnc"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1
# How many files the random test scrambles (each with three seeds).
FUZZ=${FMT_FUZZ:-40}
[ "$FAST" -eq 1 ] && FUZZ=${FMT_FUZZ:-12}

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

FAIL=0
note() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }

# Every `.fi` file of the project, without the deliberately broken ones in
# tests/lexneg/ (they do not scan -- that is what they are there for) and
# without the symbolic links (bin/ points into lib/firnc1/).
sources() {
    find lib bin tests demos examples bench tools -name '*.fi' -not -type l \
        -not -path 'tests/lexneg/*' | sort
}

# The same, but WITH the symbolic links: the copy of the tree needs
# bin/rt.fi & friends as real files, otherwise no `import` resolves in it.
all_sources() {
    find lib bin tests demos examples bench tools -name '*.fi' \
        -not -path 'tests/lexneg/*' | sort
}

echo "== 1. build firnfmt =="
FMT="$TMPD/firnfmt"
FIRNLIB="$ROOT/lib" "$FIRNC" -o "$FMT" tools/fmt/firnfmt.fi || {
    echo "firnc0 cannot build tools/fmt/firnfmt.fi"; exit 1; }
echo "   firnc0 : $FMT  ($(wc -c < "$FMT") octets)"

# The same program through the compiler written in Firn. That is the point
# of the exercise: the formatter is the first serious Firn program outside
# the compiler, so the compiler written in Firn has to be able to build it.
FC1="$ROOT/.firnc1"
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    FIRNLIB="$ROOT/lib" "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null || exit 1
fi
FMT1="$TMPD/firnfmt1"
if FIRNLIB="$ROOT/lib" "$FC1" tools/fmt/firnfmt.fi -o "$FMT1" >"$TMPD/fc1.log" 2>&1; then
    echo "   firnc1 : $FMT1  ($(wc -c < "$FMT1") octets)"
    FMT1_OK=1
else
    echo "   firnc1 : NOT built (exit $?), see $TMPD/fc1.log"
    FMT1_OK=0
    note "firnc1 cannot build tools/fmt/firnfmt.fi"
fi

echo
echo "== 2. the whole tree: tokens, syntax tree, idempotence =="
MIRROR="$TMPD/tree"
mkdir -p "$MIRROR"
# `.fi` plus the package manifests -- without them the module search in the
# copy would not find the packages in demos/packages/.
tar -chf - $(all_sources) $(find . -name 'firn.package' -not -path './.git/*' | sed 's|^\./||') \
    | tar -xf - -C "$MIRROR"
( cd "$MIRROR" && "$FMT" -w $(find . -name '*.fi') 2>/dev/null )
mrc=$?
# 3 = single sources REFUSED (unknown character, unbalanced brackets --
# the deliberately broken negative tests). Everything else is a failure.
[ "$mrc" -eq 0 ] || [ "$mrc" -eq 3 ] || note "firnfmt -w on the copy (exit $mrc)"

# The refused files, once, by name. They are left out of the per-file
# comparison below -- there is nothing to compare -- and are checked
# separately in step 3: every one of them has to be a source that the
# COMPILER rejects too.
"$FMT" -c $(sources) > "$TMPD/shape.txt" 2> "$TMPD/refused.txt"
sed 's/^.*: //' "$TMPD/refused.txt" | sort > "$TMPD/refused_names.txt"

files=0; tokdiff=0; astdiff=0; nonidem=0; astcases=0; changed=0
first=""
while IFS= read -r f; do
    if grep -qxF "$f" "$TMPD/refused_names.txt"; then
        continue
    fi
    files=$((files + 1))
    cmp -s "$f" "$MIRROR/$f" || changed=$((changed + 1))
    # a) the token stream
    FIRNLIB="$ROOT/lib" "$FIRNC" --emit=tokens "$f" 2>/dev/null | cut -c11- > "$TMPD/t1"
    r1=${PIPESTATUS[0]}
    FIRNLIB="$MIRROR/lib" "$FIRNC" --emit=tokens "$MIRROR/$f" 2>/dev/null | cut -c11- > "$TMPD/t2"
    r2=${PIPESTATUS[0]}
    if [ "$r1" -eq 0 ] && [ "$r2" -eq 0 ]; then
        if ! cmp -s "$TMPD/t1" "$TMPD/t2"; then
            tokdiff=$((tokdiff + 1)); [ -z "$first" ] && first="$f (tokens)"
        fi
    fi
    # b) the syntax tree
    if FIRNLIB="$ROOT/lib" "$FIRNC" --emit=ast-canon "$f" > "$TMPD/a1" 2>/dev/null; then
        astcases=$((astcases + 1))
        FIRNLIB="$MIRROR/lib" "$FIRNC" --emit=ast-canon "$MIRROR/$f" > "$TMPD/a2" 2>/dev/null
        if ! cmp -s "$TMPD/a1" "$TMPD/a2"; then
            astdiff=$((astdiff + 1)); [ -z "$first" ] && first="$f (syntax tree)"
        fi
    fi
    # c) idempotence
    "$FMT" "$MIRROR/$f" > "$TMPD/two" 2>/dev/null
    if ! cmp -s "$MIRROR/$f" "$TMPD/two"; then
        nonidem=$((nonidem + 1)); [ -z "$first" ] && first="$f (idempotence)"
    fi
done < <(sources)
echo "   files formatted:        $files  (of them changed by the shape: $changed)"
echo "   refused (deliberately broken): $(wc -l < "$TMPD/refused_names.txt")"
echo "   token stream differs:   $tokdiff"
echo "   syntax tree differs:    $astdiff  (out of $astcases comparable files)"
echo "   second run differs:     $nonidem"
[ "$tokdiff" -eq 0 ] || note "the token stream changed"
[ "$astdiff" -eq 0 ] || note "the syntax tree changed"
[ "$nonidem" -eq 0 ] || note "not idempotent"
[ -z "$first" ] || echo "   first deviation: $first"

echo
echo "== 3. the tree in the repository is in canonical shape =="
outofshape=$(grep -c 'is not formatted' "$TMPD/shape.txt")
refusedn=$(wc -l < "$TMPD/refused_names.txt")
echo "   out of shape: $outofshape"
echo "   refused:      $refusedn"
if [ "$outofshape" -ne 0 ]; then
    head -10 "$TMPD/shape.txt" | sed 's/^/     /'
    note "the tree is not formatted (run: firnfmt -w)"
fi
# Every refused file has to be one that the COMPILER rejects too --
# otherwise firnfmt would be refusing valid Firn.
while IFS= read -r rf; do
    [ -n "$rf" ] || continue
    if FIRNLIB="$ROOT/lib" "$FIRNC" -o /dev/null "$rf" > /dev/null 2>&1; then
        note "firnfmt refuses a source that the compiler accepts: $rf"
    fi
done < "$TMPD/refused_names.txt"

echo
echo "== 4. random test: blanks scrambled, shape has to stay =="
# Take FUZZ files spread over the whole corpus, three seeds each.
mapfile -t ALL < <(sources)
step=$(( ${#ALL[@]} / FUZZ ))
[ "$step" -lt 1 ] && step=1
fuzz_cases=0; fuzz_bad=0; fuzz_ast=0; fuzz_astcases=0
for ((k = 0; k < ${#ALL[@]}; k += step)); do
    f="${ALL[$k]}"
    grep -qxF "$f" "$TMPD/refused_names.txt" && continue
    dir=$(dirname "$f")
    "$FMT" "$f" > "$TMPD/base.fi" 2>/dev/null || continue
    for seed in 1 2 3; do
        python3 tools/fmt/mutate.py "$f" "$dir/.fmtfuzz.fi" "$seed" || continue
        fuzz_cases=$((fuzz_cases + 1))
        if ! "$FMT" "$dir/.fmtfuzz.fi" > "$TMPD/mut.fi" 2>/dev/null; then
            fuzz_bad=$((fuzz_bad + 1))
            echo "     scrambled version does not scan: $f seed=$seed"
            rm -f "$dir/.fmtfuzz.fi"; continue
        fi
        if ! cmp -s "$TMPD/base.fi" "$TMPD/mut.fi"; then
            fuzz_bad=$((fuzz_bad + 1))
            echo "     shape depends on blanks: $f seed=$seed"
            diff "$TMPD/base.fi" "$TMPD/mut.fi" | head -6 | sed 's/^/       /'
        fi
        # format -> parse -> format again: the tree has to stay equal
        if FIRNLIB="$ROOT/lib" "$FIRNC" --emit=ast-canon "$f" > "$TMPD/f1" 2>/dev/null; then
            fuzz_astcases=$((fuzz_astcases + 1))
            cp "$TMPD/mut.fi" "$dir/.fmtfuzz.fi"
            FIRNLIB="$ROOT/lib" "$FIRNC" --emit=ast-canon "$dir/.fmtfuzz.fi" > "$TMPD/f2" 2>/dev/null
            cmp -s "$TMPD/f1" "$TMPD/f2" || {
                fuzz_ast=$((fuzz_ast + 1))
                echo "     syntax tree differs after scrambling: $f seed=$seed"
            }
        fi
        rm -f "$dir/.fmtfuzz.fi"
    done
done
echo "   scrambled cases:        $fuzz_cases"
echo "   shape differs:          $fuzz_bad"
echo "   syntax tree differs:    $fuzz_ast  (out of $fuzz_astcases comparable cases)"
[ "$fuzz_bad" -eq 0 ] || note "the shape depends on blanks"
[ "$fuzz_ast" -eq 0 ] || note "the syntax tree changed in the random test"

echo
echo "== 5. the pinned cases (tools/fmt/cases/) =="
cases=0; casebad=0
for inp in tools/fmt/cases/*.in; do
    [ -e "$inp" ] || break
    want="${inp%.in}.expected"
    cases=$((cases + 1))
    "$FMT" < "$inp" > "$TMPD/case.out" 2>/dev/null
    if ! cmp -s "$TMPD/case.out" "$want"; then
        casebad=$((casebad + 1))
        echo "     $inp"
        diff "$want" "$TMPD/case.out" | head -12 | sed 's/^/       /'
    fi
done
echo "   cases: $((cases - casebad)) / $cases"
[ "$casebad" -eq 0 ] || note "pinned cases failed"

echo
echo "== 6. counter-checks =="
# 6a. Broken input must not be REPAIRED. firnfmt is no lexer: it knows
# only three kinds of breakage (unknown character, text literal or block
# comment not closed) and refuses those outright with exit code 3. For all
# the others the demand is weaker but just as binding: the formatted text
# has to be rejected by `firnc0` with THE SAME message as the original.
refused=0; lexneg=0; kept=0
for f in tests/lexneg/*.fi; do
    [ -e "$f" ] || break
    lexneg=$((lexneg + 1))
    if ! "$FMT" "$f" > "$TMPD/neg.fi" 2>/dev/null; then
        refused=$((refused + 1))
        continue
    fi
    m1=$(FIRNLIB="$ROOT/lib" "$FIRNC" -o /dev/null "$f" 2>&1 | grep -m1 '^error:')
    m2=$(FIRNLIB="$ROOT/lib" "$FIRNC" -o /dev/null "$TMPD/neg.fi" 2>&1 | grep -m1 '^error:')
    if [ -n "$m1" ] && [ "$m1" = "$m2" ]; then
        kept=$((kept + 1))
    else
        note "the error vanished after formatting: $f"
        echo "     before: $m1"
        echo "     after : $m2"
    fi
done
echo "   broken sources refused: $refused / $lexneg   (the rest still broken: $kept)"
[ $((refused + kept)) -eq "$lexneg" ] || note "a broken source came out repaired"

# 6b. `-c` has to strike on a file that is out of shape.
printf 'fn main( ) -> i32 {\n  return    1\n}\n' > "$TMPD/ugly.fi"
"$FMT" -c "$TMPD/ugly.fi" > "$TMPD/ugly.out" 2>&1
if [ $? -eq 1 ] && grep -q 'is not formatted' "$TMPD/ugly.out"; then
    echo "   -c strikes on an unformatted file: yes"
else
    note "-c did NOT strike on an unformatted file"
fi

# 6c. and it has to be quiet on the formatted one.
"$FMT" "$TMPD/ugly.fi" > "$TMPD/pretty.fi"
if "$FMT" -c "$TMPD/pretty.fi" > /dev/null 2>&1; then
    echo "   -c quiet on the formatted file:      yes"
else
    note "-c strikes on an already formatted file"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "FMT: everything passed"
    exit 0
fi
echo "FMT: $FAIL checks failed"
exit 1
