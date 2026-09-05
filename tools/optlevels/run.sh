#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/optlevels/run.sh -- THE FOUR BUILD LEVELS HAVE TO AGREE (round 90).
#
# WHY THIS EXISTS.
#
# Round 90 was opened by a bug report from the osum kernel: `firnc
# --opt-level=release-safe` produced WRONG CODE, and only that level. The
# cause was an instruction that writes a register it does not name --
# x86's one-operand `mul` puts its product in `rdx:rax` -- while the
# register allocator handed `rdx` out as the home of a live value
# (`regalloc.rs::inst_clobbers`, `tests/1900_mul_clobbers_rdx.fi`).
#
# The report said only `release-safe` was affected. It was worse: `dev-fast`
# -- THE DEFAULT LEVEL -- was broken too, on 25 of the suite's own programs,
# and had been since the merge of round 87. Nobody saw it because the levels
# were never played off AGAINST EACH OTHER: every section of the suite
# checked one level against a written-down expectation, and a program whose
# expectation is `exit 0` looks the same whether it is right or whether it
# never got far enough to be wrong.
#
# So this section asks a different question, the one that would have caught
# it on the day: DO THE FOUR LEVELS SAY THE SAME THING? A build level is an
# optimisation choice. `dev`, `dev-fast` and `release-safe` must produce
# programs that behave identically down to the octet; `release-fast` must
# too, for every program that does not deliberately go out of range (it is
# the one level that wraps instead of checking, SPEC section 13 item L9).
#
# WHAT IS RUN
#
#   1. Every program in the list below, through `firnc0`, in all four
#      levels. Same exit code, same standard output, and equal to the
#      expectation in line 1 of the file.
#   2. The same programs through `firnc1` (the self-hosted compiler), in all
#      four levels. `firnc1` has no register allocation and is therefore
#      structurally immune to this whole class -- which is exactly why it is
#      the control group, and why it will notice the day it grows one.
#   3. COUNTER-CHECK A: a program that goes out of range MUST differ between
#      `release-fast` and the three checked levels. Without this the whole
#      comparison could be measuring four levels that all quietly do
#      nothing.
#   4. COUNTER-CHECK B: `FIRN_RA_ROUGH=1` switches the allocator back to the
#      coarse (more conservative, always safe) crossing question. The
#      results must be identical to the exact one. If the exact analysis
#      ever again claims something the coarse one does not, this is where
#      it shows.
#
# Usage:  bash tools/optlevels/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

FAIL=0
note() { echo "  $*"; }
bad()  { FAIL=$((FAIL + 1)); echo "  FAIL  $*"; }

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi
# Never reuse an old `.firnc1` (the lesson of round 46).
if [ ! -x "$FC1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$FC1" -print -quit)" ]; then
    rm -f "$FC1"
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc0 cannot build firnc1"; exit 1; }
fi

LEVELS="dev dev-fast release-safe release-fast"
FIRST_LEVEL="dev"

# The programs. Chosen for the instructions that write registers they do not
# name -- checked `*` and `/`, saturating arithmetic, `select`/`cmov`,
# `copymem`, atomics -- plus the library-heavy ones that dragged the whole
# corpus down when the model was wrong (text, collector, crypto, JSON,
# DEFLATE, the hash map).
PROGS="
tests/1900_mul_clobbers_rdx.fi
tests/890_frame_bitmap.fi
tests/1401_core_number.fi
tests/801_std_num_core.fi
tests/300_str16_surrogate.fi
tests/307_bignum.fi
tests/500_gc_grundlagen.fi
tests/520_gc_weak.fi
tests/1610_hash_map.fi
tests/1611_deflate.fi
tests/1612_json.fi
tests/1613_crypto.fi
tests/332_adressierung.fi
tests/1403_core_page_allocator.fi
"

# ---------------------------------------------------------------- 1 + 2 -----
run_all_levels() {          # $1 = compiler, $2 = tag
    local cc="$1" tag="$2"
    local f base lvl bin out rc crc ref_out ref_rc hdr exp n=0 skipped=0
    for f in $PROGS; do
        [ -f "$f" ] || { bad "$tag: $f does not exist"; continue; }
        base=$(basename "$f" .fi)
        ref_out=""; ref_rc=""
        for lvl in $LEVELS; do
            bin="$TMPD/$tag.$base.$lvl"
            set +e
            "$cc" --opt-level="$lvl" -o "$bin" "$f" >"$TMPD/c.err" 2>&1
            crc=$?
            set -e
            # firnc1's own "I cannot do this" codes (tools/self_compare.sh):
            # 3 = not core language, 4 = comptime, 5 = defer, 6 = this FIR.
            # They are a KNOWN limit of the self-hosted compiler, not a
            # difference between build levels -- but the program is then
            # skipped for firnc1 in EVERY level, never in only some.
            if [ "$tag" = firnc1 ] && [ "$crc" -ge 3 ] && [ "$crc" -le 6 ]; then
                skipped=$((skipped + 1))
                continue 2
            fi
            if [ "$crc" -ne 0 ]; then
                bad "$tag $f [$lvl]: compilation failed -- $(head -2 "$TMPD/c.err" | tr '\n' ' ')"
                continue 2
            fi
            set +e
            out=$(timeout 600 "$bin" 2>/dev/null)
            rc=$?
            set -e
            if [ -z "$ref_rc" ]; then
                ref_rc=$rc; ref_out=$out
            elif [ "$rc" != "$ref_rc" ]; then
                bad "$tag $f: exit code $rc at [$lvl], $ref_rc at [$FIRST_LEVEL]"
                continue 2
            elif [ "$out" != "$ref_out" ]; then
                bad "$tag $f: the output at [$lvl] differs from [$FIRST_LEVEL]"
                continue 2
            fi
        done
        # ... and all four have to be RIGHT, not merely equal.
        hdr=$(head -1 "$f")
        case "$hdr" in
            *expect_exit:*)
                exp=${hdr#*expect_exit: }
                [ "$ref_rc" = "$exp" ] || bad "$tag $f: exit code $ref_rc in every level, expected $exp" ;;
            *expect_out:*)
                exp=${hdr#*expect_out: }
                [ "$ref_out" = "$exp" ] || bad "$tag $f: output '$ref_out' in every level, expected '$exp'" ;;
        esac
        n=$((n + 1))
    done
    note "$tag: $n programs x 4 levels, $skipped outside the self-hosted subset"
}

echo "1. firnc0 -- every program in all four levels"
run_all_levels "$FIRNC" firnc0

echo "2. firnc1 (self-hosted, no register allocation) -- the control group"
run_all_levels "$FC1" firnc1

# ------------------------------------------------------------ 3. counter -----
echo "3. counter-check: the levels are really different where they must be"
cat > "$TMPD/overflow.fi" <<'EOF'
fn main() -> i32 {
    var a: u32 = 4000000000
    var b: u32 = 4000000000
    let c: u32 = a * b
    if c == 0 {
        return 3
    }
    return 7
}
EOF
OVF_DEV=""; OVF_DEVFAST=""; OVF_SAFE=""; OVF_FAST=""
for lvl in $LEVELS; do
    if ! "$FIRNC" --opt-level="$lvl" -o "$TMPD/ovf.$lvl" "$TMPD/overflow.fi" >/dev/null 2>&1; then
        bad "counter-check: $lvl does not compile the overflowing program"
        continue
    fi
    set +e
    "$TMPD/ovf.$lvl" >/dev/null 2>&1
    rc=$?
    set -e
    case "$lvl" in
        dev)          OVF_DEV=$rc ;;
        dev-fast)     OVF_DEVFAST=$rc ;;
        release-safe) OVF_SAFE=$rc ;;
        release-fast) OVF_FAST=$rc ;;
    esac
done
# 101 = the panic of the checked levels, 7 = the wrapped result.
[ "$OVF_DEV" = "101" ]     || bad "counter-check: dev does not abort on overflow (rc=$OVF_DEV)"
[ "$OVF_DEVFAST" = "101" ] || bad "counter-check: dev-fast does not abort on overflow (rc=$OVF_DEVFAST)"
[ "$OVF_SAFE" = "101" ]    || bad "counter-check: release-safe does not abort on overflow (rc=$OVF_SAFE)"
[ "$OVF_FAST" = "7" ]      || bad "counter-check: release-fast does not wrap (rc=$OVF_FAST)"
note "overflow: dev=$OVF_DEV dev-fast=$OVF_DEVFAST release-safe=$OVF_SAFE release-fast=$OVF_FAST"

# ------------------------------------------------------- 4. rough == exact ---
echo "4. counter-check: the exact crossing question agrees with the coarse one"
for f in tests/1900_mul_clobbers_rdx.fi tests/1613_crypto.fi tests/1611_deflate.fi; do
    base=$(basename "$f" .fi)
    for lvl in release-safe release-fast; do
        "$FIRNC" --opt-level="$lvl" -o "$TMPD/x.$base.$lvl" "$f" >/dev/null 2>&1
        set +e
        a_out=$(timeout 600 "$TMPD/x.$base.$lvl" 2>/dev/null); a_rc=$?
        set -e
        FIRN_RA_ROUGH=1 "$FIRNC" --opt-level="$lvl" -o "$TMPD/r.$base.$lvl" "$f" >/dev/null 2>&1
        set +e
        b_out=$(timeout 600 "$TMPD/r.$base.$lvl" 2>/dev/null); b_rc=$?
        set -e
        if [ "$a_rc" != "$b_rc" ] || [ "$a_out" != "$b_out" ]; then
            bad "$f [$lvl]: exact ($a_rc) and FIRN_RA_ROUGH=1 ($b_rc) disagree"
        fi
    done
done
note "exact and coarse crossings agree on three programs x two levels"

echo
if [ "$FAIL" -gt 0 ]; then
    echo "optlevels: $FAIL FAILURES"
    exit 1
fi
echo "optlevels: ok"
