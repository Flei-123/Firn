#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/escape/run.sh -- THE ESCAPE ANALYSIS, MEASURED IN BOTH COMPILERS
# (round 79, compiler/src/escape.rs and lib/firnc1/escape.fi).
#
# WHAT IS PROVEN HERE AND WHY EXACTLY THAT:
#
#   1. THE REJECTED ONES (tools/escape/reject/): programs in which the
#      address of a local really does leave its frame. Every one of them has
#      to be REFUSED -- by firnc0 AND by the self-hosted firnc1 -- and the
#      first line of the file says at which line:column and with which text.
#      A program that gets through here is a silent memory error in a
#      language that promises safety.
#
#   2. THE MESSAGE, CHARACTER FOR CHARACTER. The output of the two compilers
#      is compared with `cmp`: the whole block, including the arrow line, the
#      source line, the marker, `= note:` and `= help:`. Two compilers of one
#      language that reject the same program with two different messages are
#      two languages. That is also why firnc1 does not write its own
#      renderer: it goes through lib/firnc1/diag.fi, the twin of
#      compiler/src/diag.rs.
#
#   3. THE COUNTER-CHECK (tools/escape/accept/), and it is the half that
#      matters more: correct programs WITH POINTERS that have to keep
#      compiling. A checker that refuses everything catches every error.
#      Every single false alarm here counts as a failure of the round. The
#      cases are the patterns that really stand in this repository --
#      `&(*s).b` out of an accessor, a load out of the heap, the out
#      parameter idiom, the stride measurement of lib/std/rc.fi, a field
#      read next to a field that holds an address (lib/js/regexp.fi), and
#      the deliberate way out `#[allow_escape]`.
#
# Usage:  bash tools/escape/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FIRNC1=${FIRNC1:-./.firnc1}
WORK=.escape-work
mkdir -p "$WORK"
rm -f "$WORK"/*

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
# LESSON of round 46, the same trap for the fifth time: never reuse a binary
# just because it is there. An outdated .firnc1 measures yesterday's state.
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

pass=0
fail=0
same=0

note() { echo "FAIL  $1"; fail=$((fail + 1)); }

# ---------------------------------------------------------------- rejected
for f in tools/escape/reject/*.fi; do
    name=$(basename "$f" .fi)
    hdr=$(head -1 "$f")
    case "$hdr" in
        "// expect_error: "*) ;;
        *) note "$name: no '// expect_error:' line at the top"; continue;;
    esac
    exp=${hdr#// expect_error: }
    pos=${exp%% *}
    msg=${exp#* }

    "$FIRNC" "$f" -o "$WORK/$name.bin" > "$WORK/$name.c0" 2>&1
    rc0=$?
    "$FIRNC1" "$f" -o "$WORK/$name.1.bin" > "$WORK/$name.c1" 2>&1
    rc1=$?

    if [ "$rc0" -eq 0 ]; then
        note "$name: firnc0 let it through (exit 0) -- a dangling pointer got past"
        continue
    fi
    if [ "$rc1" -eq 0 ]; then
        note "$name: firnc1 let it through (exit 0) -- a dangling pointer got past"
        continue
    fi
    if grep -qE 'panicked at|RUST_BACKTRACE' "$WORK/$name.c0"; then
        note "$name: a Rust panic instead of a clean message"
        continue
    fi
    if ! grep -qF ":$pos" "$WORK/$name.c0"; then
        note "$name: position '$pos' is missing from the message"
        sed 's/^/        /' "$WORK/$name.c0" | head -4
        continue
    fi
    if ! grep -qF "$msg" "$WORK/$name.c0"; then
        note "$name: expected text is missing from the message"
        sed 's/^/        /' "$WORK/$name.c0" | head -4
        continue
    fi
    # The three places a reader needs: where it is taken, where it dies,
    # where it gets out. The last one is the position above.
    if ! grep -qF "= note: " "$WORK/$name.c0"; then
        note "$name: the message has no note saying where the local dies"
        continue
    fi
    if ! grep -qF "= help: " "$WORK/$name.c0"; then
        note "$name: the message has no help"
        continue
    fi
    if ! cmp -s "$WORK/$name.c0" "$WORK/$name.c1"; then
        note "$name: the two compilers say something different"
        diff "$WORK/$name.c0" "$WORK/$name.c1" | head -8 | sed 's/^/        /'
        continue
    fi
    same=$((same + 1))
    pass=$((pass + 1))
    echo "PASS  reject/$name  ($pos, both compilers, identical message)"
done

# ------------------------------------------------------------- accepted
for f in tools/escape/accept/*.fi; do
    name=$(basename "$f" .fi)
    hdr=$(head -1 "$f")
    case "$hdr" in
        "// expect_ok"*) ;;
        *) note "$name: no '// expect_ok' line at the top"; continue;;
    esac
    "$FIRNC" "$f" -o "$WORK/$name.bin" > "$WORK/$name.c0" 2>&1
    rc0=$?
    "$FIRNC1" "$f" -o "$WORK/$name.1.bin" > "$WORK/$name.c1" 2>&1
    rc1=$?
    if [ "$rc0" -ne 0 ]; then
        note "$name: FALSE ALARM in firnc0 -- a correct program was refused"
        sed 's/^/        /' "$WORK/$name.c0" | head -8
        continue
    fi
    if [ "$rc1" -ne 0 ]; then
        note "$name: FALSE ALARM in firnc1 -- a correct program was refused"
        sed 's/^/        /' "$WORK/$name.c1" | head -8
        continue
    fi
    pass=$((pass + 1))
    echo "PASS  accept/$name  (builds in both compilers)"
done

nrej=$(ls tools/escape/reject/*.fi | wc -l)
nacc=$(ls tools/escape/accept/*.fi | wc -l)
echo "--------------------------------------------------------------------"
echo "  cases:              $((nrej + nacc))  ($nrej rejected, $nacc counter-checks)"
echo "  messages identical: $same / $nrej"
echo "PASS: $pass   FAIL: $fail"
[ "$fail" -eq 0 ]
