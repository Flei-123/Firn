#!/usr/bin/env bash
# tools/testkit/run.sh -- std.testkit end to end (docs/SKRIPT-LIBS.md, r311).
#
# tests/2300_std_testkit.fi runs every assertion failing inside a forked
# child. This script is the other half: the REAL consumers.
#   1. `firnc --test` over tools/testkit/cases/suite.fi: the failing cases
#      are reported as "fail" with the assertion text as the reason and the
#      position of the test function; the passing ones as "pass"; the exit
#      code is 1; a case AFTER a failing one still runs
#   2. script style: exit code 1 / 0 and the exact output format of
#      OpenPlan's Python check()
#   3. an assertion in a plain main: exit code 101, the code after it does
#      not run
#   4. counter-checks: the same suite with the failing asserts removed is
#      green (so the reds above are the assertions' doing), and a deliberate
#      wrong expectation strikes
# Usage:  bash tools/testkit/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
CASES=tools/testkit/cases
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; }
has() { if grep -qF -- "$2" "$1"; then ok; else bad "$3 -- '$2' is missing"; fi; }
eq() { if [ "$2" = "$3" ]; then ok; else bad "$1: got '$2', expected '$3'"; fi; }

echo "== 1. firnc --test over assertions =="
"$FIRNC" --test -o "$TMPD/suite" "$CASES/suite.fi" > "$TMPD/suite.json" 2> "$TMPD/suite.err"
eq "exit code with failing assertions" "$?" "1"
python3 - "$TMPD/suite.json" > "$TMPD/suite.txt" << 'PY' || bad "the report is not valid JSON"
import json, sys
d = json.load(open(sys.argv[1]))
print("total", d["total"], "passed", d["passed"], "failed", d["failed"])
for c in d["cases"]:
    print("case", c["name"], c["status"], c["line"], c.get("reason", "").split("\n")[0])
PY
has "$TMPD/suite.txt" "total 6 passed 2 failed 4" "counts"
has "$TMPD/suite.txt" "case holds pass" "a holding case passes"
has "$TMPD/suite.txt" "case near_holds pass" "assert_near and assert_eq[u8] hold"
has "$TMPD/suite.txt" "case eq_fails fail" "assert_eq_i64 fails the case"
has "$TMPD/suite.txt" "assert_eq failed: left=1 right=2" "the reason is the assertion text"
has "$TMPD/suite.txt" "case str_fails fail" "assert_eq_str fails the case"
has "$TMPD/suite.txt" "assert_eq_str failed: lengths 17 and 18" "the str reason"
has "$TMPD/suite.txt" "assert failed: arithmetic is broken" "assert_true reason"
has "$TMPD/suite.txt" "case context_fails fail" "context case fails"
has "$TMPD/suite.txt" "assert_near failed: left=1 right=2 diff=-1 eps=0.1 [second group]" "context label in the reason"
# the position is the one of the test function (no caller location exists)
for t in eq_fails str_fails true_fails context_fails; do
    line=$(sed -n "s/.*\"name\":\"$t\"[^}]*\"line\":\([0-9]*\).*/\1/p" "$TMPD/suite.json")
    text=$(sed -n "${line:-0}p" "$CASES/suite.fi")
    case "$text" in
        "fn $t"*) ok ;;
        *) bad "$t is reported at line $line ('$text'), expected its declaration" ;;
    esac
done
has "$TMPD/suite.json" '"exit":101' "the exit code of the panic path is reported"

echo "== 2. script style =="
"$FIRNC" -o "$TMPD/sf" "$CASES/script_fail.fi" && "$TMPD/sf" > "$TMPD/sf.out"
eq "script with a failing check: exit" "$?" "1"
printf '  ok    two and two\n  FAIL  three times three\n  ok    the last one\n3 checks, 1 failed\n' > "$TMPD/sf.want"
if cmp -s "$TMPD/sf.out" "$TMPD/sf.want"; then ok; else bad "output format differs:"; diff "$TMPD/sf.want" "$TMPD/sf.out" | head -5; fi
"$FIRNC" -o "$TMPD/so" "$CASES/script_ok.fi" && "$TMPD/so" > "$TMPD/so.out"
eq "script, all ok: exit" "$?" "0"
printf '  ok    one is less than two\n  ok    two is less than three\norder: 2 checks, 0 failed\n' > "$TMPD/so.want"
if cmp -s "$TMPD/so.out" "$TMPD/so.want"; then ok; else bad "finish_named output differs"; fi

echo "== 3. an assertion in main =="
"$FIRNC" -o "$TMPD/am" "$CASES/assert_in_main.fi" && "$TMPD/am" > "$TMPD/am.out" 2> "$TMPD/am.err"
eq "assert in main: exit" "$?" "101"
has "$TMPD/am.err" "assert_eq failed: left=5 right=6" "message on stderr"
if [ -s "$TMPD/am.out" ]; then bad "the line after the failing assertion ran"; else ok; fi

echo "== 4. counter-checks =="
# remove the three failing assertions: the suite must be green
grep -v 'assert_eq_i64(1, 2)\|assert_true(1 + 1 == 3\|assert_eq_str("first line' "$CASES/suite.fi" > "$TMPD/green.fi"
"$FIRNC" --test -o "$TMPD/green" "$TMPD/green.fi" > "$TMPD/green.json" 2>&1
has "$TMPD/green.json" '"name":"eq_fails","status":"pass"' "without its assertion the case is green"
# a deliberately wrong expectation has to strike
sed 's/failed 4/failed 3/' "$TMPD/suite.txt" > "$TMPD/wrong.txt"
if grep -qF "total 6 passed 2 failed 4" "$TMPD/wrong.txt"; then bad "the wrong expectation did not strike"; else ok; fi

echo
echo "testkit: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
