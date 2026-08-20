#!/usr/bin/env bash
# tools/fixpoint.sh -- THE FIXPOINT: Firn carries itself.
#
# Three stages, the same source:
#
#   stage 1   firnc0 (Rust)  compiles  bin/firnc1.fi  ->  .firnc1
#   stage 2   .firnc1        compiles  bin/firnc1.fi  ->  .firnc2
#   stage 3   .firnc2        compiles  bin/firnc1.fi  ->  .firnc3
#
# STAGE 2 AND STAGE 3 HAVE TO BE CHARACTER-IDENTICAL. That is the real
# proof: `.firnc2` is produced by a compiler that came from Rust,
# `.firnc3` by one that came from Firn. If their outputs are equal, the
# result no longer hangs on the Rust compiler -- the translator is a fixpoint
# of itself.
#
# WHY STAGE 1 IS NOT COMPARED ALONG: `firnc0` has a register allocation,
# `lib/firnc1/codegen.fi` does not. The assembly texts of stage 1 and stage 2
# cannot be equal at all and do not have to be -- what is compared is
# what stays stable from stage 2 on.
#
# Finally: the self-compiled compiler (.firnc2) runs over the
# WHOLE test corpus and has to behave exactly like stage 1 while doing so. A
# compiler that can only compile itself would be no compiler.
set -uo pipefail
cd "$(dirname "$0")/.."
# A temp directory of its own per run: two simultaneous runs (e.g. the main
# repo and a worktree) otherwise used THE SAME /tmp files and
# overwrote each other's comparison output -- which looked like a
# real difference (round 41).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
SOURCE=bin/firnc1.fi

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi

# --- stage 1 ---------------------------------------------------------------
# Rebuild when .firnc1 is missing OR a source file is younger -- an
# outdated .firnc1 otherwise measures yesterday's state (round 35).
if [ ! -x ./.firnc1 ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer ./.firnc1 -print -quit)" ]; then
    rm -f ./.firnc1
    "$FIRNC" "$SOURCE" -o ./.firnc1 || { echo "stage 1 failed"; exit 1; }
fi

# --- stage 2 ---------------------------------------------------------------
rm -f .firnc2 .firnc2.s .firnc2.o
t0=$(date +%s%N)
./.firnc1 "$SOURCE" -o ./.firnc2
rc2=$?
t1=$(date +%s%N)
if [ "$rc2" -ne 0 ]; then
    echo "STAGE 2 FAILED (rc=$rc2)"
    echo "  3 = not core language * 4 = comptime * 5 = defer * 6 = code generator"
    exit 1
fi
if [ ! -x ./.firnc2 ]; then
    echo "STAGE 2: no executable file"
    exit 1
fi

# --- stage 3 ---------------------------------------------------------------
rm -f .firnc3 .firnc3.s .firnc3.o
t2=$(date +%s%N)
./.firnc2 "$SOURCE" -o ./.firnc3
rc3=$?
t3=$(date +%s%N)
if [ "$rc3" -ne 0 ]; then
    echo "STAGE 3 FAILED (rc=$rc3)"
    exit 1
fi

echo "STAGE 2: $(( (t1 - t0) / 1000000 )) ms   $(wc -c < .firnc2) octets"
echo "STAGE 3: $(( (t3 - t2) / 1000000 )) ms   $(wc -c < .firnc3) octets"

# --- the comparison --------------------------------------------------------
if ! cmp -s .firnc2.s .firnc3.s; then
    echo "NO FIXPOINT: the assembly texts of stage 2 and 3 differ"
    diff <(head -400 .firnc2.s) <(head -400 .firnc3.s) | head -20
    exit 1
fi
if ! cmp -s .firnc2 .firnc3; then
    echo "NO FIXPOINT: the binaries differ (with the same .s)"
    exit 1
fi
lines=$(wc -l < .firnc2.s)
echo "FIXPOINT:  stage 2 == stage 3, character-identical ($lines lines of assembly)"

# --- the self-compiled compiler over the whole corpus ----------------------
FIRNC1=./.firnc2 bash tools/self_compare.sh > "$TMPD"/fixpoint_corpus.txt 2>&1
krc=$?
sed 's/^/  /' "$TMPD"/fixpoint_corpus.txt
if [ "$krc" -ne 0 ]; then
    echo "STAGE 2 does NOT behave like stage 1 on the corpus"
    exit 1
fi
echo "CORPUS:    .firnc2 behaves like firnc0"
exit 0
