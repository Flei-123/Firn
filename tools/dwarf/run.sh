#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/dwarf/run.sh -- THE PROOF FOR THE DEBUG INFORMATION (round 64, point 3).
#
# It drives `gdb` in batch mode over two translated Firn programs and holds
# its output against expectations. Nothing here is read off by eye: every
# claim in docs/DEBUGGER.md corresponds to one line in this script.
#
#   1. translate (`--no-opt`) and check the sections with `readelf`
#   2. `.debug_info` read back with `readelf --debug-dump=info`: subprogram,
#      formal_parameter, variable, base_type, pointer_type, array_type,
#      structure_type with members
#   3. the gdb session over docs/gdb_example.fi: breakpoint on a Firn
#      function, backtrace with the caller, `info args`, `info locals`,
#      stepping line by line, `print`
#   4. the gdb session over tools/dwarf/probe.fi: struct with members,
#      pointer with `print *p` and `p->x`, array with `print field[2]`,
#      `finish` with the return value, `ptype`
#   5. THE VALUES ARE RIGHT, not just present: the printed numbers are
#      compared against the values the program really computes
#   7. ROUND 94 -- THE LINE TABLE AGAINST THE PROGRAM'S OWN MESSAGE. An
#      overflow inside an EMBEDDED function: the panic message says where it
#      stands, `gdb` is asked what it thinks the address of that check is,
#      and the two have to agree -- at all four build levels. Plus: no line
#      number of the table points at a blank or a comment line, the panic arm
#      behind the `ret` carries the right line too, and three counter-checks
#      (stripped = no answer, a wrong expectation strikes, the `fn main` line
#      is NOT the answer).
#   8. ROUND 94 -- the four build levels: how many lines each of them still
#      covers, and that a coarser level never invents a line that `--no-opt`
#      does not have.
#   6. counter-checks. Without them the whole thing would be worthless:
#      * WITH the optimizer there must be NO variable information -- a wrong
#        value in the debugger is worse than none (docs/DEBUGGER.md)
#      * a deliberately wrong expectation has to FAIL
#      * `print` of a name that does not exist has to fail
#
# Usage:  bash tools/dwarf/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
export FIRNLIB="$ROOT/lib"

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
if ! command -v gdb > /dev/null; then
    echo "gdb is missing -- without it this proof cannot be run"
    exit 1
fi

# Checks that `file` contains the text `want`.
expect() {
    local file=$1 want=$2 what=$3
    if grep -qF -- "$want" "$file"; then
        ok
    else
        bad "$what: '$want' is missing"
    fi
}
# ... and that it does NOT contain it.
expect_not() {
    local file=$1 want=$2 what=$3
    if grep -qF -- "$want" "$file"; then
        bad "$what: '$want' is there but must not be"
    else
        ok
    fi
}

echo "== 1. translate and look at the sections =="
"$FIRNC" --no-opt -o "$TMPD/demo" docs/gdb_example.fi || { echo "translation failed"; exit 1; }
"$FIRNC" --no-opt -o "$TMPD/probe" tools/dwarf/probe.fi || { echo "translation failed"; exit 1; }
"$FIRNC" -o "$TMPD/demo_opt" docs/gdb_example.fi || exit 1
readelf -S "$TMPD/demo" > "$TMPD/sections.txt"
for sec in .debug_info .debug_abbrev .debug_line; do
    expect "$TMPD/sections.txt" "$sec" "sections"
done
echo "   $(grep -c debug "$TMPD/sections.txt") debug sections in $TMPD/demo"

echo
echo "== 2. .debug_info read back =="
readelf --debug-dump=info "$TMPD/probe" > "$TMPD/info.txt" 2>&1
for tag in DW_TAG_compile_unit DW_TAG_subprogram DW_TAG_formal_parameter \
           DW_TAG_variable DW_TAG_base_type DW_TAG_pointer_type \
           DW_TAG_array_type DW_TAG_structure_type DW_TAG_member; do
    expect "$TMPD/info.txt" "$tag" "debug_info"
done
expect "$TMPD/info.txt" "DW_AT_frame_base  : 1 byte block: 56" "frame base is rbp"
expect "$TMPD/info.txt" "DW_OP_fbreg" "variables lie in the frame"
expect "$TMPD/info.txt" "(ANSI C99)" "language"
echo "   $(grep -c DW_TAG_variable "$TMPD/info.txt") variables, $(grep -c DW_TAG_formal_parameter "$TMPD/info.txt") parameters, $(grep -c DW_TAG_subprogram "$TMPD/info.txt") functions"

echo
echo "== 3. gdb over docs/gdb_example.fi =="
gdb -batch \
    -ex "break summe" -ex run -ex bt -ex "info args" -ex "info locals" \
    -ex "next" -ex "next" -ex "next" \
    -ex "print s" -ex "print i" -ex "print n" -ex "ptype summe" \
    -ex continue "$TMPD/demo" > "$TMPD/g1.txt" 2>&1
expect "$TMPD/g1.txt" "file docs/gdb_example.fi, line 3." "breakpoint on the Firn function"
expect "$TMPD/g1.txt" "summe (n=10) at docs/gdb_example.fi:3" "frame with the parameter value"
expect "$TMPD/g1.txt" "#1  0x" "backtrace has the caller"
expect "$TMPD/g1.txt" "in main () at docs/gdb_example.fi:11" "the caller is main, line 11"
expect "$TMPD/g1.txt" "n = 10" "info args"
expect "$TMPD/g1.txt" "4	    for i in 1 as i32..n + 1 as i32 {" "stepping shows Firn source text"
expect "$TMPD/g1.txt" "5	        s = s + i" "stepping reaches the loop body"
expect "$TMPD/g1.txt" 'type = i32 (i32)' "ptype of the function"
expect "$TMPD/g1.txt" "exited with code 067" "the program runs through (55 = 067 octal)"
# The values: after the first pass s = 1 and i = 2, n stays 10.
expect "$TMPD/g1.txt" '$1 = 1' "print s = 1"
expect "$TMPD/g1.txt" '$2 = 2' "print i = 2"
expect "$TMPD/g1.txt" '$3 = 10' "print n = 10"

echo
echo "== 4. gdb over tools/dwarf/probe.fi: struct, pointer, array =="
gdb -batch \
    -ex "break shift" -ex run -ex bt -ex "info args" \
    -ex "print *p" -ex "print p->x" -ex "ptype struct Point" \
    -ex finish -ex "info locals" -ex "print p" \
    -ex "break total" -ex continue -ex next -ex next -ex next \
    -ex "print field" -ex "print field[2]" -ex "ptype field" \
    -ex "print sum" -ex continue "$TMPD/probe" > "$TMPD/g2.txt" 2>&1
expect "$TMPD/g2.txt" "shift (p=0x" "pointer parameter in the frame line"
expect "$TMPD/g2.txt" "by=3" "second parameter"
expect "$TMPD/g2.txt" '$1 = {x = 5, y = 7}' "print *p -- struct with members"
expect "$TMPD/g2.txt" '$2 = 5' "print p->x"
expect "$TMPD/g2.txt" "type = struct Point {" "ptype struct Point"
expect "$TMPD/g2.txt" "    i32 x;" "the member x with its type"
expect "$TMPD/g2.txt" "    i32 y;" "the member y with its type"
expect "$TMPD/g2.txt" "Value returned is \$3 = 18" "finish gives the return value"
expect "$TMPD/g2.txt" "p = {x = 8, y = 10}" "the struct in main was changed through the pointer"
expect "$TMPD/g2.txt" "total (n=2) at tools/dwarf/probe.fi:18" "second breakpoint"
expect "$TMPD/g2.txt" '$5 = {1, 2, 3, 4}' "print of an array"
expect "$TMPD/g2.txt" '$6 = 3' "print field[2]"
expect "$TMPD/g2.txt" "type = i32 [4]" "ptype of the array"
expect "$TMPD/g2.txt" "exited with code 046" "the program runs through (38 = 046 octal)"

echo
echo "== 5. the values are the ones the program computes =="
"$TMPD/probe"; rc_probe=$?
"$TMPD/demo"; rc_demo=$?
[ "$rc_probe" -eq 38 ] && ok || bad "probe.fi returns $rc_probe, expected 38"
[ "$rc_demo" -eq 55 ] && ok || bad "gdb_example.fi returns $rc_demo, expected 55"
echo "   probe.fi -> $rc_probe, gdb_example.fi -> $rc_demo"

echo
echo "== 6. counter-checks =="
# 6a. WITH the optimizer there must be no variable information.
readelf --debug-dump=info "$TMPD/demo_opt" > "$TMPD/info_opt.txt" 2>&1
expect_not "$TMPD/info_opt.txt" "DW_TAG_variable" "optimized: no variables"
expect_not "$TMPD/info_opt.txt" "DW_TAG_formal_parameter" "optimized: no parameters"
echo "   optimized build: $(grep -c DW_TAG_variable "$TMPD/info_opt.txt") variables (has to be 0)"

# 6b. A deliberately wrong expectation has to strike.
if grep -qF '$1 = 999' "$TMPD/g1.txt"; then
    bad "the counter-check does not strike: a wrong value was found"
else
    ok
fi

# 6c. gdb has to say NO to a name that does not exist -- otherwise the
# preceding print results would prove nothing.
gdb -batch -ex "break summe" -ex run -ex "print does_not_exist" -ex kill \
    "$TMPD/demo" > "$TMPD/g3.txt" 2>&1
expect "$TMPD/g3.txt" "No symbol" "unknown name is refused"

# 6d. A binary without debug information really has none.
cp "$TMPD/demo" "$TMPD/demo_stripped"
strip --strip-debug "$TMPD/demo_stripped" 2>/dev/null
readelf -S "$TMPD/demo_stripped" > "$TMPD/sections_stripped.txt"
expect_not "$TMPD/sections_stripped.txt" ".debug_info" "stripped: no debug info"

echo
echo "== 7. ROUND 94: the line table does not lie =="
#
# THE MEASUREMENT. `tools/dwarf/inline_probe.fi` overflows an `i32` inside a
# function that the optimizer embeds in its caller. Two independent parts of
# the compiler then say where that addition stands:
#
#   * the PANIC MESSAGE, built at lowering time out of the source position
#     (round 72, `lower.rs::overflow_msg`) -- it is printed by the program
#     itself and knows nothing about DWARF;
#   * the LINE TABLE, built by the code generator out of `fir::Loc`.
#
# They have to name the same line. Before round 94 they did not, as soon as
# the optimizer ran: the line table carried only the line of the `fn`, so
# `gdb` reported the caller's first line for code out of another function.
#
# The address that is asked about is the one of the overflow check itself
# (`jo`), found in the disassembly -- no eye, no guess.

PROBE=tools/dwarf/inline_probe.fi
PROBE_LINES=$(wc -l < "$PROBE")

# The address of the overflow check (`jo`) in the program. Read out of
# `objdump`, not out of `gdb`: the linker symbols of this compiler carry a
# dot (`_F0.add_one`, DESIGN_GOALS 4) and `gdb` reads that as a field
# access. The probe has exactly one checked addition, so the first `jo` in
# the text is the one that matters.
check_addr() {
    objdump -d "$1" 2> /dev/null | grep -m1 -E '^[[:space:]]*[0-9a-f]+:.*[[:space:]]jo[[:space:]]' \
        | awk '{print "0x" $1}' | tr -d ':'
}
# The target of that check -- the panic arm behind the `ret` (round 90).
arm_addr() {
    objdump -d "$1" 2> /dev/null | grep -m1 -E '^[[:space:]]*[0-9a-f]+:.*[[:space:]]jo[[:space:]]' \
        | awk '{print "0x" $(NF-1)}'
}
# The line gdb attributes to an address.
line_of() {
    gdb -batch -ex "info line *$2" "$1" 2>&1 | grep -m1 -oE 'Line [0-9]+' | awk '{print $2}'
}

for lvl in dev dev-fast release-safe release-fast; do
    bin="$TMPD/probe_$lvl"
    "$FIRNC" --opt-level="$lvl" -o "$bin" "$PROBE" 2> /dev/null \
        || { bad "$lvl: translation failed"; continue; }
    msg=$("$bin" 2>&1 > /dev/null)
    # a) every line of the table really exists in the file, and none of them
    #    is a blank or a comment line -- code cannot come from there.
    readelf --debug-dump=decodedline "$bin" 2> /dev/null \
        | awk '$2 ~ /^[0-9]+$/ {print $2}' | sort -un > "$TMPD/lines_$lvl.txt"
    covered=$(wc -l < "$TMPD/lines_$lvl.txt")
    bogus=0
    while read -r ln; do
        [ -z "$ln" ] && continue
        if [ "$ln" -gt "$PROBE_LINES" ]; then
            bogus=$((bogus + 1))
            continue
        fi
        text=$(sed -n "${ln}p" "$PROBE")
        case "$(echo "$text" | tr -d ' \t')" in
            "" | "//"*) bogus=$((bogus + 1)) ;;
        esac
    done < "$TMPD/lines_$lvl.txt"
    if [ "$bogus" -eq 0 ]; then
        ok
    else
        bad "$lvl: $bogus line numbers of the table point at nothing ($PROBE has $PROBE_LINES lines)"
    fi
    # b) the panic message and the line table agree about the SAME address.
    case "$msg" in
        *"$PROBE":*)
            want=$(echo "$msg" | sed -n "s|.*$PROBE:\([0-9]*\):.*|\1|p")
            addr=$(check_addr "$bin")
            if [ -z "$addr" ]; then
                bad "$lvl: no overflow check found in the disassembly"
            else
                got=$(line_of "$bin" "$addr")
                if [ "$got" = "$want" ]; then
                    ok
                    echo "   $lvl: the check at $addr is line $got, the panic message says $want, $covered lines covered"
                else
                    bad "$lvl: gdb says line ${got:-none} for $addr, the program's own message says $want"
                fi
            fi
            ;;
        *)
            # release-fast: unchecked arithmetic (SPEC 13, L9), no message.
            # What is measured here instead is the ATTRIBUTION ACROSS
            # INLINING: the body of `add_one` sits inside `main` and has to
            # keep ITS line, not the caller's.
            inlined=$(gdb -batch -ex 'disassemble /s main' "$bin" 2> /dev/null \
                | grep -cE '^23[[:space:]]')
            if [ "$inlined" -ge 1 ]; then
                ok
                echo "   $lvl: no check (unchecked arithmetic), but line 23 of the embedded callee stands in main, $covered lines covered"
            else
                bad "$lvl: the embedded body of add_one does not carry its own line in main"
            fi
            ;;
    esac
done

# c) the same thing for the PANIC ARM. It is emitted behind the `ret`
#    (round 90, the cold half) and used to inherit whatever line stood last.
bin="$TMPD/probe_release-safe"
addr=$(check_addr "$bin")
target=$(arm_addr "$bin")
armline=$(gdb -batch -ex "info line *$target" "$bin" 2>&1 | grep -m1 -oE 'Line [0-9]+' | awk '{print $2}')
if [ "$armline" = "23" ]; then
    ok
    echo "   the panic arm at $target is line $armline as well"
else
    bad "the panic arm is line ${armline:-none}, expected 23"
fi

# d) COUNTER-CHECK 1: without the debug information the measurement has to
#    collapse. Otherwise it would be proving nothing about the table.
cp "$bin" "$TMPD/probe_stripped"
strip --strip-debug "$TMPD/probe_stripped" 2> /dev/null
if gdb -batch -ex "info line *$addr" "$TMPD/probe_stripped" 2>&1 | grep -q 'No line number information'; then
    ok
else
    bad "counter-check: the stripped binary still answers with a line"
fi

# e) COUNTER-CHECK 2: a deliberately wrong expectation has to strike.
if [ "$(line_of "$bin" "$addr")" = "999" ]; then
    bad "counter-check: gdb reports line 999, which does not exist"
else
    ok
fi

# f) COUNTER-CHECK 3: the line of the `fn main` declaration is NOT the answer
#    for the embedded code. That is exactly the answer round 94 got rid of,
#    so a return to it has to fail here.
mainline=$(grep -n '^fn main' "$PROBE" | head -1 | cut -d: -f1)
if [ "$(line_of "$bin" "$addr")" = "$mainline" ]; then
    bad "the check is attributed to the 'fn main' line ($mainline) -- that is the round 94 bug"
else
    ok
fi

echo
echo "== 8. ROUND 94: the four build levels, and what each of them still knows =="
# What is measured: how many distinct source lines the table covers per
# level, and that the coarser levels are really COARSER and not WRONG (every
# line they cover is also covered by `--no-opt`).
base="$TMPD/lines_dev.txt"
for lvl in dev-fast release-safe release-fast; do
    extra=$(comm -13 "$base" "$TMPD/lines_$lvl.txt" | wc -l)
    if [ "$extra" -eq 0 ]; then
        ok
    else
        bad "$lvl: $extra lines that --no-opt does not have at all"
    fi
done
printf '   lines covered:'
for lvl in dev dev-fast release-safe release-fast; do
    printf ' %s=%s' "$lvl" "$(wc -l < "$TMPD/lines_$lvl.txt")"
done
echo

echo
echo "DWARF: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
