#!/usr/bin/env bash
# tools/prune_compare.sh -- FIR dead function pruning: firnc1 against firnc0 (r731).
#
# YARDSTICK: `FIRN_PRUNE_FIR=1 FIRN_KEEP_ALL=1 firnc --no-opt --keep-asm` (stage 0). With
# the assembly level pruning switched off, the .s file holds exactly the
# functions the FIR pruning (`compiler/src/prune.rs` + `lower_reach`) kept.
# `FIRN_PRUNE_FIR=1 .firnc1 file.fi` (stage 1, `lib/firnc1/prune.fi`) writes
# its assembly to stdout; its function labels are compared name for name.
#
# Two sets per side: ALL (pruning off) and KEPT (pruning on). Lowering
# differences that exist without pruning (closure numbering, see
# tools/fir_compare.sh) are not the pruning's fault, so only the names in
# BOTH ALL sets are compared: KEPT0 ∩ common  against  KEPT1 ∩ common.
#
# LAZY LOWERING (r734): second check per file. `FIRN_PRUNE_FIR=1` makes stage 1
# lower only the reachable functions (`prune.fi lower_reach`);
# `FIRN_PRUNE_FIR=1 FIRN_EAGER_LOWER=1` lowers everything and prunes after (the
# r731 path). Both assemblies must agree byte for byte, except for the static
# text constants of dead functions (blocks `.Lstatic_text.N`, which the lazy
# path never creates -- they are removed and the labels renumbered first).
#
# Usage: tools/prune_compare.sh [files.fi ...]      (default: tests/*.fi)
#   PRUNE_CMP_MAX=<n>   look at the first n usable files only
# Environment: FIRNC (stage 0), FIRNC1 (stage 1, default ./.firnc1, rebuilt
# when a source is younger).
# Exit: 0 = no unexpected deviation, 1 = deviation found (the first is printed).
set -uo pipefail
cd "$(dirname "$0")/.."
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT
export FIRNLIB="$(pwd)/lib"
FIRNC=${FIRNC:-compiler/target/release/firnc}
FC1=${FIRNC1:-./.firnc1}
MAX=${PRUNE_CMP_MAX:-1000000}

if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    "$FIRNC" bin/firnc1.fi -o "$TMPD/firnc1.new" || { echo "stage 1 failed"; exit 1; }
    mv -f "$TMPD/firnc1.new" "$FC1" || exit 1
fi

if [ "$#" -gt 0 ]; then
    FILES=("$@")
else
    mapfile -t FILES < <(find tests -maxdepth 1 -name '*.fi' -not -type l | sort)
fi

lazy_same=0; lazy_diff=0; lazy_first=""
same=0; different=0; skipped=0; basediff=0; looked=0; dropped_total=0; all_total=0
first=""

# Function labels of an assembly text, `_F0.`/`_F1.` prefix removed; the
# entry point `main` is unprefixed in stage 0.
labels0() { grep -E '^(_F0\.)?[A-Za-z_][^ ]*:$' "$1" | grep -E '^(_F0\.|main:)' | sed -E 's/^_F0\.//;s/:$//' | sort -u; }
labels1() { grep -E '^_F1\.[^ ]*:$' "$1" | sed -E 's/^_F1\.//;s/:$//' | sort -u; }

# Assembly without the static text blocks, labels renumbered.
norm_asm() {
    perl -0pe 's/\.section \.rodata\n\s*\.p2align 0\n\.Lstatic_text\.\d+:\n\s*\.byte [^\n]*\n//g; s/\.Lstatic_text\.\d+/.Lstatic_text.#/g' "$1"
}

for f in "${FILES[@]}"; do
    [ "$looked" -ge "$MAX" ] && break
    if head -n 3 "$f" | grep -q '^// firnc1_skip'; then skipped=$((skipped+1)); continue; fi
    # hosted executables only (the kernel profile / freestanding code is not pruned)
    if grep -qE '^profile (kernel|freestanding)|--profile=kernel|expect_compile_error|expect_error' "$f"; then
        skipped=$((skipped+1)); continue
    fi
    rm -f "$TMPD"/x0a* "$TMPD"/x0b*
    if ! "$FIRNC" --no-opt --keep-asm "$f" -o "$TMPD/x0a" >/dev/null 2>&1; then skipped=$((skipped+1)); continue; fi
    if ! FIRN_PRUNE_FIR=1 FIRN_KEEP_ALL=1 "$FIRNC" --no-opt --keep-asm "$f" -o "$TMPD/x0b" >/dev/null 2>&1; then
        skipped=$((skipped+1)); continue
    fi
    "$FC1" "$f" > "$TMPD/a1.s" 2>/dev/null; rc1=$?
    FIRN_PRUNE_FIR=1 FIRN_EAGER_LOWER=1 "$FC1" "$f" > "$TMPD/b1.s" 2>/dev/null; rc2=$?
    FIRN_PRUNE_FIR=1 "$FC1" "$f" > "$TMPD/l1.s" 2>/dev/null; rc3=$?
    if [ "$rc1" -ne 0 ] || [ "$rc2" -ne 0 ]; then skipped=$((skipped+1)); continue; fi
    [ -f "$TMPD/x0a.s" ] && [ -f "$TMPD/x0b.s" ] || { skipped=$((skipped+1)); continue; }
    looked=$((looked+1))
    labels0 "$TMPD/x0a.s" > "$TMPD/all0"
    labels0 "$TMPD/x0b.s" > "$TMPD/kept0"
    labels1 "$TMPD/a1.s" > "$TMPD/all1"
    labels1 "$TMPD/b1.s" > "$TMPD/kept1"
    comm -12 "$TMPD/all0" "$TMPD/all1" > "$TMPD/common"
    [ "$(wc -l < "$TMPD/common")" -ne "$(wc -l < "$TMPD/all0")" ] || [ "$(wc -l < "$TMPD/common")" -ne "$(wc -l < "$TMPD/all1")" ] && basediff=$((basediff+1))
    comm -12 "$TMPD/kept0" "$TMPD/common" > "$TMPD/k0c"
    comm -12 "$TMPD/kept1" "$TMPD/common" > "$TMPD/k1c"
    all_total=$((all_total + $(wc -l < "$TMPD/common")))
    dropped_total=$((dropped_total + $(wc -l < "$TMPD/common") - $(wc -l < "$TMPD/k1c")))
    if [ "$rc3" -eq "$rc2" ] && norm_asm "$TMPD/b1.s" > "$TMPD/b1.n" && norm_asm "$TMPD/l1.s" > "$TMPD/l1.n" && cmp -s "$TMPD/b1.n" "$TMPD/l1.n"; then
        lazy_same=$((lazy_same+1))
    else
        lazy_diff=$((lazy_diff+1))
        if [ -z "$lazy_first" ]; then
            lazy_first="$f"
            diff "$TMPD/b1.n" "$TMPD/l1.n" | head -12 > "$TMPD/lazydiff"
        fi
    fi
    if cmp -s "$TMPD/k0c" "$TMPD/k1c"; then
        same=$((same+1))
    else
        different=$((different+1))
        if [ -z "$first" ]; then
            first="$f"
            diff "$TMPD/k0c" "$TMPD/k1c" | head -12 > "$TMPD/firstdiff"
        fi
    fi
done

echo "SAME:          $same   (kept function names identical to stage 0)"
echo "DIFFERENT:     $different"
echo "SKIPPED:       $skipped   (not hosted / not compilable in both / firnc1_skip)"
echo "BASE-DIFF:     $basediff   (files whose unpruned name sets already differ; info only)"
echo "LAZY SAME:     $lazy_same   (lazy lowering == eager lowering + prune, asm byte-identical)"
echo "LAZY DIFFERENT: $lazy_diff"
echo "FUNCTIONS:     $all_total common, $dropped_total dropped by stage 1"
if [ -n "$lazy_first" ]; then
    echo "first lazy deviation: $lazy_first   (< eager+prune, > lazy)"
    cat "$TMPD/lazydiff"
fi
if [ -n "$first" ] || [ -n "$lazy_first" ]; then
  if [ -n "$first" ]; then
    echo "first deviation: $first   (< stage 0 only, > stage 1 only)"
    cat "$TMPD/firstdiff"
  fi
    exit 1
fi
exit 0
