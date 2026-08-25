#!/usr/bin/env bash
# tools/reject/run.sh -- WHAT THE TWO COMPILERS REFUSE, COMPARED (round 96).
#
# THE GAP THIS CLOSES. The repository is full of tests that compare what the
# two compilers PRODUCE: the same tokens (lex_compare.sh), the same tree
# (parser_compare.sh), the same types (sema_compare.sh), the same FIR
# (fir_compare.sh), the same binary down to the octet (fixpoint.sh). Not one
# of them compared what the two compilers REFUSE -- and a compiler is as much
# the programs it turns away as the programs it translates.
#
# What that cost: `firnc1` accepted
#
#     const A: u64 = 1
#     const A: u64 = 2
#
# without a word, took the FIRST declaration and dropped the second. The
# merge of rounds K4 and K5 left `const KDATA_SIZE` twice in
# demos/kernel/kstate.fi, 0x30000 from K4 and 0x20000 from K5. `firnc0`
# refused to build it. `firnc1` built it with the smaller value, and the
# kernel put its tables for open files, descriptors, contexts and pipes at
# 0x20000..0x24FFF -- outside the data area it had just been given. It ran,
# and while it ran it overwrote memory that belonged to somebody else. That
# it was noticed at all was luck: `firnc0` happens to be tested along.
#
# ONE LIST, NOT TWO. The corpus is `tests/neg/*.fi` -- exactly the list
# section 4 of test.sh already walks, not a second one of its own. The lesson
# of round 90 was that two lists asking the same question drift apart; a new
# faulty program therefore lands here automatically, and nothing has to be
# kept in step by hand.
#
# WHAT IS DEMANDED, in three steps of decreasing strength:
#
#   1. `firnc0` refuses it. (Section 4 of test.sh checks the message; here
#      it is only the precondition -- a corpus entry that is not faulty
#      proves nothing.)
#   2. `firnc1` refuses it TOO. This is the invariant that matters: whatever
#      the wording, no program that stage 0 turns away may be translated by
#      stage 1. Where it does not hold, the file stands in SWALLOWED below,
#      by name and with the reason.
#   3. Where `firnc1` SAYS something, it says the same thing as `firnc0`,
#      character for character -- the message, the arrow line, the source
#      line, the marker. Compared with `cmp`. Where it does not hold, the
#      file stands in WORDING below, by name and with the reason.
#
# `firnc1` is silent for most of its errors (it counts them and stops); that
# is an old state of affairs and not this round's doing. Those files are
# counted apart and named in the output as what they are -- refused, but
# without a sentence to compare.
#
# THE TWO LISTS MAY ONLY SHRINK. A file that is named here and no longer
# needs naming is reported as an ERROR. That way the exceptions cannot
# quietly become the rule.
#
# Usage:  bash tools/reject/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FIRNC1=${FIRNC1:-./.firnc1}
WORK=.reject-work
mkdir -p "$WORK"
rm -f "$WORK"/*

# ---------------------------------------------------------------------------
# `firnc1` translates these although `firnc0` refuses them. Every line is a
# hole, not an exemption -- the check behind the message does not exist in
# stage 1 at all. None of them is a duplicate declaration; those were the
# subject of round 96 and are all closed.
SWALLOWED="
tests/neg/877_closure_nogc.fi a '#[no_gc]' function calls a closure -- nogc.fi does not follow closures
tests/neg/887_closure_write_capture.fi a capture is written to -- fnval.fi does not know read-only captures
tests/neg/assign_let.fi 'let' against 'var': stage 1 does not carry the mutability of a local
tests/neg/assign_op_let.fi the same, with '+='
tests/neg/step_let.fi the same, with '++'
tests/neg/atomcas_ty.fi the argument types of __atomic_cas are not checked
tests/neg/atomic_ty.fi the argument types of __atomic_add are not checked
tests/neg/core_break_outside.fi 'break' outside a loop is not noticed
tests/neg/core_export.fi 'export' of a name that is not there is not noticed
tests/neg/f32_kernel_needs_allow_fp.fi profile rule: f32 without '#[allow_fp]' in the kernel
tests/neg/free_float_without_allow_fp.fi profile rule: f64 without '#[allow_fp]' in the kernel
tests/neg/free_interrupt_call.fi profile rule: an '#[interrupt]' function called by hand
tests/neg/free_interrupt_parameter.fi profile rule: '#[interrupt]' with a parameter
tests/neg/free_interrupt_ret.fi profile rule: '#[interrupt]' with a return value
tests/neg/free_unknown_profile.fi an unknown name after 'profile'
tests/neg/literal_default_overflow.fi a literal without context that does not fit into i32
tests/neg/no_return.fi a path through the function without a 'return'
"

# `firnc1` refuses them, but says it in its own words. Both are older
# deviations of their own subsystem and have nothing to do with duplicate
# declarations: neither passes its message through diag.fi, so there is no
# arrow line, no source line and no marker.
WORDING="
tests/neg/comptime_file_absolute.fi time.fi prints its comptime messages without a position
tests/neg/comptime_file_parent.fi the same
tests/neg/free_std_import_in_kernel.fi firnc1.fi has no source map for imports (complain_std_module)
tests/neg/kernel_std_io.fi the same
"

named() { # $1 = list, $2 = file
    echo "$1" | awk '{print $1}' | grep -qxF "$2"
}

# ---------------------------------------------------------------------------
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
# LESSON of round 46: never reuse a binary just because it is there. An
# outdated .firnc1 measures yesterday's state.
rebuild=0
[ -x "$FIRNC1" ] || rebuild=1
if [ -x "$FIRNC1" ]; then
    [ "$FIRNC" -nt "$FIRNC1" ] && rebuild=1
    while IFS= read -r q; do
        [ "$q" -nt "$FIRNC1" ] && { rebuild=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$rebuild" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FIRNC1" || { echo "FAIL  building $FIRNC1"; exit 1; }
fi

fail=0
same=0
silent=0
noncore=0
swallowed=0
wording=0
total=0

note() { echo "FAIL  $1"; fail=$((fail + 1)); }

for f in tests/neg/*.fi; do
    name=$(basename "$f" .fi)
    total=$((total + 1))

    "$FIRNC" "$f" -o "$WORK/$name.b0" > "$WORK/$name.c0" 2>&1
    rc0=$?
    "$FIRNC1" "$f" -o "$WORK/$name.b1" > "$WORK/$name.c1" 2>&1
    rc1=$?

    # --- 1. the corpus entry has to BE faulty ------------------------------
    if [ "$rc0" -eq 0 ]; then
        note "$name: firnc0 translates it -- it does not belong in tests/neg/"
        continue
    fi
    if grep -qE 'panicked at|RUST_BACKTRACE' "$WORK/$name.c0"; then
        note "$name: a Rust panic instead of a clean message"
        continue
    fi

    # --- the return values of firnc1 ---------------------------------------
    #   0 = translated * 1 = error * 2 = I/O * 3 = not core language *
    #   4 = comptime * 5 = defer * 6 = code generator * 7 = as/ld
    if [ "$rc1" -ge 128 ]; then
        # A signal. Whatever the program does wrong, the compiler must not
        # fall over on it -- that is not a rejection, that is a crash.
        note "$name: firnc1 died of signal $((rc1 - 128))"
        continue
    fi
    if [ "$rc1" -ge 3 ]; then
        # Refused, but because stage 1 does not carry the feature at all --
        # the program was never looked at. Not comparable, and no cover for
        # a missing check either: such a file cannot be on SWALLOWED.
        if named "$SWALLOWED" "$f"; then
            note "$name: stands in SWALLOWED although firnc1 stops at rc=$rc1 -- remove the line"
            continue
        fi
        noncore=$((noncore + 1))
        continue
    fi

    # --- 2. firnc1 has to refuse it ----------------------------------------
    if [ "$rc1" -eq 0 ]; then
        if named "$SWALLOWED" "$f"; then
            swallowed=$((swallowed + 1))
            continue
        fi
        note "$name: firnc0 refuses it, firnc1 TRANSLATES it -- a hole with no name"
        sed 's/^/        /' "$WORK/$name.c0" | head -4
        continue
    fi
    if named "$SWALLOWED" "$f"; then
        note "$name: stands in SWALLOWED although firnc1 refuses it -- remove the line"
        continue
    fi

    # --- 3. and where it says something, it says the same thing ------------
    if [ ! -s "$WORK/$name.c1" ]; then
        if named "$WORDING" "$f"; then
            note "$name: stands in WORDING although firnc1 says nothing -- remove the line"
            continue
        fi
        silent=$((silent + 1))
        continue
    fi
    if cmp -s "$WORK/$name.c0" "$WORK/$name.c1"; then
        if named "$WORDING" "$f"; then
            note "$name: stands in WORDING although the two agree -- remove the line"
            continue
        fi
        same=$((same + 1))
        continue
    fi
    if named "$WORDING" "$f"; then
        wording=$((wording + 1))
        continue
    fi
    note "$name: both refuse it, and they say different things"
    diff "$WORK/$name.c0" "$WORK/$name.c1" | head -10 | sed 's/^/        /'
done

# --- a named file that is no longer in the corpus --------------------------
for l in "$SWALLOWED" "$WORDING"; do
    while IFS= read -r line; do
        [ -z "$line" ] && continue
        q=$(echo "$line" | awk '{print $1}')
        [ -z "$q" ] && continue
        if [ ! -f "$q" ]; then
            note "$q is named here but is not in the corpus any more"
        fi
    done < <(echo "$l")
done

echo "CORPUS:        $total faulty programs (tests/neg/*.fi, the list of section 4)"
echo "SAME:          $same  refused by both, message identical character for character"
echo "SILENT:        $silent  refused by both, firnc1 without a sentence of its own"
echo "NOT CORE:      $noncore  firnc1 stops before the program (rc>=3), not comparable"
echo "SWALLOWED:     $swallowed  firnc0 refuses, firnc1 translates -- named holes"
echo "WORDING:       $wording  both refuse, firnc1 in its own words -- named"
echo "REFUSED BY BOTH: $((total - swallowed)) of $total"
if [ "$fail" -gt 0 ]; then
    echo "FAILURES:      $fail"
    exit 1
fi
exit 0
