#!/usr/bin/env bash
# tools/ct/run.sh -- THE PROOF FOR CONSTANT TIME (round 96, SPEC 9).
#
# ACCEPTANCE.md item 6 asks two things of this round and this script answers
# both of them with a number:
#
#   "a program that puts a secret value into a conditional jump or into a
#    field index MUST be refused by the compiler -- with line and column"
#   "the assembly produced for a comparison of two secret values must not
#    contain a conditional jump; check that on the ASSEMBLY TEXT"
#
# Sections:
#
#   1. THE ASSEMBLY. `tools/ct/probe.fi` at all FOUR build levels. Every
#      `#[constant_time]` function in it is loop free on purpose, so the
#      claim can be checked in a text: not one conditional jump that is
#      control flow of the program (tools/ct/jumps.py splits those from the
#      bounds/overflow checks, which are decided by public data and always
#      go the same way).
#   2. THE COUNTER-CHECKS. Without them section 1 would be worth nothing:
#      the public twins of the same functions HAVE TO carry a conditional
#      jump, the extracted bodies have to be non-empty, and `select` has to
#      really become a `cmov`.
#   3. THE REFUSALS. Every tests/neg/ct_secret_*.fi has to be refused with
#      exactly its expected message AND its expected line:column -- plus a
#      counter-check with a deliberately wrong expectation, which has to
#      strike.
#   4. `secure_zero` on a buffer of secrets survives all four levels
#      (SPEC 9.3, C3), and the probe RUNS and gives the same answer at every
#      level.
#   5. BOTH COMPILERS. `firnc1` (in Firn) has to refuse the same programs
#      with the octet-identical message. Where it cannot, that is counted
#      and named, never filtered away.
#
# Usage:  bash tools/ct/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
FC1="${FIRNC1:-$ROOT/.firnc1}"
export FIRNLIB="$ROOT/lib"

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

PASS=0
FAIL=0
ok()   { PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"; }
bad()  { FAIL=$((FAIL + 1)); printf '  FAIL  %s\n' "$1"; }

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 is missing: $FIRNC"
    exit 1
fi

LEVELS="release-fast dev dev-fast release-safe"
flag_of() {
    case "$1" in
        release-fast) echo "" ;;
        dev)          echo "--no-opt" ;;
        *)            echo "--opt-level=$1" ;;
    esac
}

echo "== 1. the assembly of the constant-time functions =="
CT_FNS="ct_eq ct_choose ct_mask ct_eq4"
for lvl in $LEVELS; do
    f=$(flag_of "$lvl")
    if ! "$FIRNC" $f --emit=asm -o "$TMPD/probe_$lvl.s" tools/ct/probe.fi >"$TMPD/build.log" 2>&1; then
        bad "$lvl: the probe does not compile"
        sed 's/^/        /' "$TMPD/build.log" | head -5
        continue
    fi
    python3 tools/ct/jumps.py "$TMPD/probe_$lvl.s" $CT_FNS > "$TMPD/j_$lvl.txt" 2>&1
    if grep -q MISSING "$TMPD/j_$lvl.txt"; then
        bad "$lvl: a constant-time function is not in the assembly (inlined away?)"
        cat "$TMPD/j_$lvl.txt" | sed 's/^/        /'
        continue
    fi
    while read -r line; do
        name=${line%% *}
        lines=$(echo "$line" | sed 's/.*lines=\([0-9]*\).*/\1/')
        control=$(echo "$line" | sed 's/.*control=\([0-9]*\).*/\1/')
        jcc=$(echo "$line" | sed 's/.* jcc=\([0-9]*\).*/\1/')
        # The body has to be a body. "no jump in an empty text" is not a
        # measurement, it is the trap this project has a rule against.
        if [ "$lines" -lt 10 ]; then
            bad "$lvl/$name: body of $lines lines -- too short to mean anything"
            continue
        fi
        if [ "$control" -ne 0 ]; then
            bad "$lvl/$name: $control conditional jump(s) of control flow ($line)"
            continue
        fi
        ok "$lvl/$name: no branch (lines=$lines jcc=$jcc control=0)"
    done < "$TMPD/j_$lvl.txt"
done

echo "== 2. the counter-checks =="
for lvl in $LEVELS; do
    [ -f "$TMPD/probe_$lvl.s" ] || continue
    python3 tools/ct/jumps.py "$TMPD/probe_$lvl.s" public_eq public_choose > "$TMPD/p_$lvl.txt" 2>&1
    n=$(awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^control=/) { split($i, a, "="); s += a[2] } } END { print s + 0 }' "$TMPD/p_$lvl.txt")
    if [ "$n" -ge 2 ]; then
        ok "$lvl: the public twins DO branch ($n conditional jumps) -- the check can strike"
    else
        bad "$lvl: the public twins carry $n branches; then section 1 measures nothing"
    fi
    c=$(python3 tools/ct/jumps.py "$TMPD/probe_$lvl.s" ct_choose | sed 's/.*cmov=\([0-9]*\).*/\1/')
    if [ "$c" -ge 1 ]; then
        ok "$lvl: 'select' on a secret condition really is a cmov ($c)"
    else
        bad "$lvl: 'select' produced no cmov"
    fi
done

echo "== 3. the refusals, with line and column =="
NEG=$(ls tests/neg/ct_*.fi)
for f in $NEG; do
    hdr=$(head -1 "$f")
    exp=${hdr#*expect_error: }
    pos=${exp%% *}
    msg=${exp#* }
    "$FIRNC" -o "$TMPD/neg.bin" "$f" >"$TMPD/neg.out" 2>&1
    rc=$?
    if [ "$rc" -eq 0 ]; then
        bad "$f: compiled without an error"
        continue
    fi
    if grep -qE "panicked at|RUST_BACKTRACE" "$TMPD/neg.out"; then
        bad "$f: a Rust panic instead of a clean message"
        continue
    fi
    if ! grep -qF ":$pos" "$TMPD/neg.out"; then
        bad "$f: the position $pos is missing"
        sed 's/^/        /' "$TMPD/neg.out" | head -4
        continue
    fi
    if ! grep -qF "$msg" "$TMPD/neg.out"; then
        bad "$f: the text is missing: $msg"
        sed 's/^/        /' "$TMPD/neg.out" | head -4
        continue
    fi
    ok "$f refused at $pos"
done
# The counter-check for section 3: a wrong expectation HAS TO strike.
cp tests/neg/ct_secret_if.fi "$TMPD/wrong.fi"
"$FIRNC" -o "$TMPD/neg.bin" "$TMPD/wrong.fi" >"$TMPD/neg.out" 2>&1
if grep -qF ":99:99" "$TMPD/neg.out"; then
    bad "counter-check: the message claims a position 99:99 that cannot be right"
else
    ok "counter-check: a wrong expectation (99:99) does not match"
fi

echo "== 4. secure_zero, and the same answer at every level =="
for lvl in $LEVELS; do
    f=$(flag_of "$lvl")
    if "$FIRNC" $f --emit=asm -o "$TMPD/z_$lvl.s" tests/435_ct_secret_eq.fi >/dev/null 2>&1 &&
        grep -q "rep stosb" "$TMPD/z_$lvl.s"; then
        ok "$lvl: secure_zero on a buffer of secrets survives (rep stosb)"
    else
        bad "$lvl: secure_zero was optimized away or the build failed"
    fi
    rm -f "$TMPD/probe.bin"
    if "$FIRNC" $f -o "$TMPD/probe.bin" tools/ct/probe.fi >/dev/null 2>&1; then
        "$TMPD/probe.bin"
        got=$?
        want=$(head -1 tools/ct/probe.fi | sed 's/.*expect_exit: //')
        if [ "$got" -eq "$want" ]; then
            ok "$lvl: the probe runs and answers $got"
        else
            bad "$lvl: the probe answers $got, expected $want"
        fi
    else
        bad "$lvl: the probe does not build"
    fi
done

echo "== 5. what firnc1 (in Firn) does with the same files =="
# WHAT THIS MEASURES: `firnc0` has SPEC 9.1 since this round, `lib/firnc1`
# does not -- exactly as it did not have the SIMD words of round 82. The
# question a proof has to ask is therefore not "does it say the same thing"
# but "does it say something WRONG": a compiler that quietly drops the
# marking would build a program that leaks. Three outcomes are counted, and
# only one of them is a failure.
if [ ! -x "$FC1" ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" >/dev/null 2>&1
fi
if [ ! -x "$FC1" ]; then
    bad "firnc1 could not be built -- section 5 measures nothing"
else
    same=0
    noncore=0
    other=0
    accepted=0
    for f in $NEG; do
        hdr=$(head -1 "$f")
        exp=${hdr#*expect_error: }
        pos=${exp%% *}
        msg=${exp#* }
        rm -f "$TMPD/n1.bin"
        "$FC1" "$f" -o "$TMPD/n1.bin" >"$TMPD/n1.out" 2>&1
        rc=$?
        if [ "$rc" -eq 0 ]; then
            accepted=$((accepted + 1))
            bad "firnc1 ACCEPTED $f -- firnc0 refuses it"
        elif [ "$rc" -eq 3 ]; then
            noncore=$((noncore + 1))
        elif grep -qF ":$pos" "$TMPD/n1.out" && grep -qF "$msg" "$TMPD/n1.out"; then
            same=$((same + 1))
        else
            other=$((other + 1))
        fi
    done
    total=$(echo "$NEG" | wc -w)
    printf '  firnc1 over %d refused files: %d identical message, %d "not core language", %d refused otherwise, %d ACCEPTED\n' \
        "$total" "$same" "$noncore" "$other" "$accepted"
    if [ "$accepted" -eq 0 ]; then
        ok "firnc1 accepts none of the files firnc0 refuses"
    fi
    # And the other way round: a program that USES secrets has to come out of
    # firnc1 as "not core language" (3) -- never as a binary.
    for f in tests/434_ct_secret_basics.fi tests/435_ct_secret_eq.fi tests/436_ct_secret_select.fi tests/437_ct_secret_flow.fi tools/ct/probe.fi; do
        rm -f "$TMPD/p1.bin"
        "$FC1" "$f" -o "$TMPD/p1.bin" >/dev/null 2>&1
        rc=$?
        if [ "$rc" -eq 3 ]; then
            ok "firnc1: $f counted as 'not core language', not built"
        else
            bad "firnc1: $f gave $rc -- expected 3 (not core language)"
        fi
    done
fi

echo "== 6. our own AES, asked the question (SPEC 9.1, C4) =="
# lib/std/crypto/aes.fi note A3 says the S-box lookup leaks through the
# cache. tools/ct/aes_probe.fi is that very step with the state declared as
# what it is -- and it MUST NOT COMPILE.
"$FIRNC" -o "$TMPD/aes.bin" tools/ct/aes_probe.fi >"$TMPD/aes.out" 2>&1
if [ $? -eq 0 ]; then
    bad "tools/ct/aes_probe.fi compiled -- the S-box lookup was NOT caught"
else
    if grep -qF "aes_probe.fi:25:29" "$TMPD/aes.out" &&
        grep -qF "a secret value cannot be converted to the public type usize" "$TMPD/aes.out"; then
        ok "SubBytes: the conversion of a secret into an index is refused (25:29)"
    else
        bad "SubBytes: the refusal at 25:29 is missing"
        sed 's/^/        /' "$TMPD/aes.out" | head -6
    fi
    if grep -qF "aes_probe.fi:31:17" "$TMPD/aes.out" &&
        grep -qF "an index must not be a secret value" "$TMPD/aes.out"; then
        ok "the table lookup with a secret index is refused (31:17)"
    else
        bad "the refusal at 31:17 is missing"
        sed 's/^/        /' "$TMPD/aes.out" | head -6
    fi
fi
# And the way out really works: the same answer for all 256 inputs, and what
# it costs is printed rather than hidden.
rm -f "$TMPD/aesct.bin"
if "$FIRNC" -o "$TMPD/aesct.bin" tools/ct/aes_ct.fi >"$TMPD/aesct.log" 2>&1; then
    out=$("$TMPD/aesct.bin")
    if echo "$out" | grep -q "^SAME 256$"; then
        ok "the constant-time way gives the same answer for all 256 inputs"
    else
        bad "the constant-time way disagrees with the table: $out"
    fi
    echo "$out" | grep "^table " | sed 's/^/  cost  /'
else
    bad "tools/ct/aes_ct.fi does not build"
    sed 's/^/        /' "$TMPD/aesct.log" | head -5
fi

echo
echo "CT: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
