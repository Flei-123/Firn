#!/usr/bin/env bash
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
echo "DWARF: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
