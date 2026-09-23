#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/wasm/run.sh -- THE SAME PROGRAMS, NATIVE AND AS WEBASSEMBLY
# (round WASM, step 4).
#
# Every program that test.sh runs (tests/*.fi, tests/opt/*.fi,
# examples/*.fi) is built twice per build level -- once for x86_64-linux,
# once for wasm32-browser -- and both are RUN: the native one as a
# process, the WebAssembly one under node (tools/wasm/run.mjs). Standard
# output, standard error and the exit code have to be the same, octet for
# octet. Standard input is empty for both, the working directory is the
# repository for both, exactly as test.sh runs them.
#
# Every program ends in exactly one of four columns:
#
#   SAME      all three outputs identical, in every build level
#   REFUSED   the compiler refused the WebAssembly build AT COMPILE TIME,
#             with a reason (files, sockets, threads, SIMD, inline asm).
#             That is the specified behaviour, not a failure -- but it is
#             counted and every reason is listed.
#   DIFFERENT a build level produced other output: that IS a failure,
#             listed with the first differing lines
#   NATIVE    the native build itself failed -- not this round's business,
#             listed so that nothing disappears from the count
#
# On top of that, three checks of the translation itself:
#   * the text form: `--emit=asm` fed to wat2wasm (wabt) has to give OUR
#     binary octet for octet (name section stripped) -- an independent
#     assembler agreeing with our encoder
#   * the dispatch fallback: the whole series once more with
#     FIRN_WASM_DISPATCH=1 (every function through the loop that
#     irreducible graphs need) -- same outputs required
#   * the collector: tools/wasm/gc_soak.sh
#
# Environment: LEVELS (default "release-fast dev dev-fast release-safe"),
#              JOBS (parallel programs, default 8), W (work directory).
#
# Usage: bash tools/wasm/run.sh [file.fi ...]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
export FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export W=${W:-/tmp/firn-wasm-run}
LEVELS=${LEVELS:-"release-fast dev dev-fast release-safe"}
JOBS=${JOBS:-8}
export LEVELS ROOT
rm -rf "$W"
mkdir -p "$W/res"

if [ $# -gt 0 ]; then
    PROGS="$*"
else
    PROGS=$(ls tests/*.fi tests/opt/*.fi examples/*.fi)
fi

# One program, all levels. Writes $W/res/<id>: first line the verdict.
one() {
    local f="$1" id base lvl nd wd flags
    id=$(echo "$f" | tr '/' '_')
    base="$W/$id"
    local verdict="SAME" detail=""
    for lvl in $LEVELS; do
        flags="--opt-level=$lvl"
        [ "$lvl" = "dev" ] && flags="--no-opt"
        # ROUND 72 (test.sh): these programs exist to show release-fast
        # wrapping; every other level panics on purpose.
        if [ "$lvl" != "release-fast" ] && head -5 "$f" | grep -q '^// only_mode: opt$'; then
            continue
        fi
        if ! timeout 300 "$FIRNC" $flags -o "$base.$lvl.nat" "$f" > "$base.$lvl.ncomp" 2>&1; then
            verdict="NATIVE"; detail="native build failed at $lvl: $(head -2 "$base.$lvl.ncomp" | tr '\n' ' ')"
            break
        fi
        if ! timeout 300 "$FIRNC" $flags --target=wasm32-browser -o "$base.$lvl.wasm" "$f" > "$base.$lvl.wcomp" 2>&1; then
            if grep -q "wasm32-browser: " "$base.$lvl.wcomp"; then
                verdict="REFUSED"
                # The reason CLASS, from the compiler's own words.
                local first why
                first=$(grep -m1 -o "wasm32-browser: .*" "$base.$lvl.wcomp")
                case "$first" in
                    *"no files"*)         why="files" ;;
                    *"no sockets"*)       why="sockets" ;;
                    *"processes"*)        why="processes" ;;
                    *"thread"*)           why="threads" ;;
                    *"SIMD"*|*"v128"*)    why="SIMD" ;;
                    *"inline assembler"*) why="inline assembler" ;;
                    *)                    why="other" ;;
                esac
                detail="$why -- $first"
            else
                verdict="DIFFERENT"; detail="wasm build failed at $lvl: $(head -3 "$base.$lvl.wcomp" | tr '\n' ' ')"
            fi
            break
        fi
        timeout 120 "$base.$lvl.nat" < /dev/null > "$base.$lvl.nout" 2> "$base.$lvl.nerr"
        echo $? > "$base.$lvl.nrc"
        timeout 120 node --stack-size=7800 "$ROOT/tools/wasm/run.mjs" "$base.$lvl.wasm" < /dev/null > "$base.$lvl.wout" 2> "$base.$lvl.werr"
        echo $? > "$base.$lvl.wrc"
        if ! cmp -s "$base.$lvl.nout" "$base.$lvl.wout" || ! cmp -s "$base.$lvl.nerr" "$base.$lvl.werr" \
            || ! cmp -s "$base.$lvl.nrc" "$base.$lvl.wrc"; then
            verdict="DIFFERENT"
            detail="$lvl: exit $(cat "$base.$lvl.nrc")/$(cat "$base.$lvl.wrc")"
            if ! cmp -s "$base.$lvl.nout" "$base.$lvl.wout"; then
                detail="$detail, stdout differs: $(diff "$base.$lvl.nout" "$base.$lvl.wout" | head -3 | tr '\n' ' ' | cut -c1-160)"
            fi
            if ! cmp -s "$base.$lvl.nerr" "$base.$lvl.werr"; then
                detail="$detail, stderr differs: $(diff "$base.$lvl.nerr" "$base.$lvl.werr" | head -3 | tr '\n' ' ' | cut -c1-160)"
            fi
            break
        fi
        # keep the work directory small: the programs of one level go
        rm -f "$base.$lvl.nat" "$base.$lvl.wasm"
    done
    printf '%s\t%s\t%s\n' "$verdict" "$f" "$detail" > "$W/res/$id"
}
export -f one

echo "== 1. native and WebAssembly, octet for octet ($LEVELS) =="
t0=$(date +%s)
printf '%s\n' $PROGS | xargs -P "$JOBS" -I{} bash -c 'one "$@"' _ {}
t1=$(date +%s)
cat "$W"/res/* | sort -k2 > "$W/results.tsv"
total=$(wc -l < "$W/results.tsv")
same=$(grep -c '^SAME' "$W/results.tsv")
refused=$(grep -c '^REFUSED' "$W/results.tsv")
different=$(grep -c '^DIFFERENT' "$W/results.tsv")
native=$(grep -c '^NATIVE' "$W/results.tsv")
echo "   programs:  $total   ($((t1 - t0)) s)"
echo "   SAME:      $same   (stdout, stderr and exit code identical in every level)"
echo "   REFUSED:   $refused   (refused at compile time, with a reason)"
echo "   DIFFERENT: $different"
echo "   NATIVE:    $native   (the native build failed; not counted either way)"
if [ "$refused" -gt 0 ]; then
    echo "   -- refused, by reason:"
    grep '^REFUSED' "$W/results.tsv" | cut -f3 | sed 's/ -- .*//' | sort | uniq -c | sort -rn | sed 's/^/      /'
    grep '^REFUSED' "$W/results.tsv" | cut -f2,3 | sed 's/\t/  /; s/^/      /' | cut -c1-200
fi
if [ "$different" -gt 0 ]; then
    echo "   -- DIFFERENT:"
    grep '^DIFFERENT' "$W/results.tsv" | cut -f2,3 | sed 's/^/      /' | cut -c1-400
fi
if [ "$native" -gt 0 ]; then
    grep '^NATIVE' "$W/results.tsv" | cut -f2,3 | sed 's/^/      /' | cut -c1-200
fi
fail=0
[ "$different" -gt 0 ] && fail=1

echo "== 2. the text form: wat2wasm must give our binary octet for octet =="
if command -v wat2wasm > /dev/null && command -v wasm-strip > /dev/null; then
    n=0; ok=0; bad=""
    for f in $(grep '^SAME' "$W/results.tsv" | cut -f2); do
        id=$(echo "$f" | tr '/' '_')
        "$FIRNC" --opt-level=dev-fast --target=wasm32-browser --emit=asm -o "$W/$id.wat" "$f" 2>/dev/null || continue
        "$FIRNC" --opt-level=dev-fast --target=wasm32-browser -o "$W/$id.cmp.wasm" "$f" 2>/dev/null || continue
        n=$((n + 1))
        if wat2wasm "$W/$id.wat" -o "$W/$id.w2w.wasm" 2> "$W/$id.w2w.err" && wasm-strip "$W/$id.cmp.wasm" \
            && cmp -s "$W/$id.cmp.wasm" "$W/$id.w2w.wasm"; then
            ok=$((ok + 1))
        else
            bad="$bad $f"
        fi
        rm -f "$W/$id.wat" "$W/$id.w2w.wasm" "$W/$id.cmp.wasm"
    done
    echo "   $ok of $n modules: wat2wasm(our text) == our binary"
    [ "$ok" = "$n" ] || { echo "   differing:$bad"; fail=1; }
else
    echo "   wat2wasm/wasm-strip not installed -- this check SKIPPED, not passed"
fi

echo "== 3. the dispatch fallback: every function through the loop =="
n=0; ok=0; bad=""
for f in $(grep '^SAME' "$W/results.tsv" | cut -f2); do
    id=$(echo "$f" | tr '/' '_')
    # dev-fast, except for the programs that exist to show release-fast
    # wrapping (`// only_mode: opt`, see test.sh): they have no other
    # native reference.
    lvl=dev-fast
    head -5 "$f" | grep -q '^// only_mode: opt$' && lvl=release-fast
    FIRN_WASM_DISPATCH=1 "$FIRNC" --opt-level=$lvl --target=wasm32-browser -o "$W/$id.disp.wasm" "$f" 2>/dev/null || { bad="$bad $f"; continue; }
    n=$((n + 1))
    timeout 120 node --stack-size=7800 tools/wasm/run.mjs "$W/$id.disp.wasm" < /dev/null > "$W/$id.dout" 2> "$W/$id.derr"
    rc=$?
    if [ "$rc" = "$(cat "$W/$id.$lvl.nrc" 2>/dev/null)" ] && cmp -s "$W/$id.dout" "$W/$id.$lvl.nout" \
        && cmp -s "$W/$id.derr" "$W/$id.$lvl.nerr"; then
        ok=$((ok + 1))
    else
        bad="$bad $f"
    fi
    rm -f "$W/$id.disp.wasm"
done
echo "   $ok of $n programs identical to native with FIRN_WASM_DISPATCH=1"
[ "$ok" = "$n" ] || { echo "   differing:$bad"; fail=1; }

echo "== 4. the collector without a stack scan (tools/wasm/gc_soak.sh) =="
if W="$W/soak" bash tools/wasm/gc_soak.sh > "$W/soak.log" 2>&1; then
    sed 's/^/   /' "$W/soak.log" | grep -E 'OK|PASSED|intact|CORRUPT'
else
    sed 's/^/   /' "$W/soak.log"
    fail=1
fi

echo
if [ "$fail" = 0 ]; then
    echo "WASM: $same of $((total - native)) programs run as WebAssembly octet for octet like native ($refused refused with a reason, 0 different)"
    echo "ALL WASM CHECKS PASSED"
else
    echo "WASM CHECKS FAILED"
fi
exit $fail
