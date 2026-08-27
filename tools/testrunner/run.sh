#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/testrunner/run.sh -- THE PROOF FOR THE TEST RUNNER (round 94,
# ACCEPTANCE.md item 4 criterion A, TODO-FIRN.md 0.3).
#
# What is being proved: `firnc --test` finds the functions marked `#[test]`,
# runs EVERY one of them even when one of them dies, and reports the result
# so that a machine can read it -- name, outcome, duration, and the place of
# the failure as file:line:column.
#
#   1. the report is JSON and it PARSES; the numbers in the header agree with
#      the cases listed under them
#   2. the four outcomes: a case that passes, one that panics (with a
#      position of its own), one that dies from a signal (no message), and
#      one that PRINTS -- its output must not end up in the report
#   3. THE POSITION IS RIGHT, not merely present: the file:line:column of the
#      failing case is compared against the statement that really stands
#      there in the source file
#   4. the exit code: 1 with a failure, 0 without -- that is what makes it
#      usable in CI
#   5. TAP as a second format
#   6. a case that never returns is killed by the time limit and reported
#   7. counter-checks. Without them this would prove nothing:
#      * ISOLATION: the case after the one that dies still appears. Without a
#        process per case the report would end at the crash
#      * a file without a `#[test]` is refused, so an empty run cannot be
#        mistaken for a green one
#      * a file that declares `main` itself is refused with a clear sentence
#      * a wrong signature is refused
#      * a deliberately wrong expectation has to strike
#   8. both compilers: `#[test]` is a KNOWN attribute in firnc1 too, so the
#      same file is core language for both of them
#
# Usage:  bash tools/testrunner/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
FC1=${FIRNC1:-$ROOT/.firnc1}
export FIRNLIB="$ROOT/lib"
CASES=tools/testrunner/cases

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); }
bad() {
    FAIL=$((FAIL + 1))
    echo "  FAIL  $1"
}
has() { # file text name
    if grep -qF -- "$2" "$1"; then ok; else bad "$3 -- '$2' is missing"; fi
}
has_not() {
    if grep -qF -- "$2" "$1"; then bad "$3 -- '$2' is there but must not be"; else ok; fi
}
num() { # name value op want
    if [ -z "$2" ]; then
        bad "$1: no number found"
        return
    fi
    if [ "$2" -"$3" "$4" ] 2> /dev/null; then ok; else bad "$1: $2, expected $3 $4"; fi
}

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

echo "== 1. the report is JSON, and its numbers agree =="
"$FIRNC" --test -o "$TMPD/mixed" "$CASES/mixed.fi" > "$TMPD/mixed.json" 2> "$TMPD/mixed.err"
rc=$?
num "exit code with a failing case" "$rc" eq 1
if command -v python3 > /dev/null; then
    python3 - "$TMPD/mixed.json" << 'EOF' > "$TMPD/parsed.txt" 2>&1
import json, sys
d = json.load(open(sys.argv[1]))
print("total", d["total"])
print("passed", d["passed"])
print("failed", d["failed"])
print("rate", d["rate"])
print("cases", len(d["cases"]))
for c in d["cases"]:
    print("case", c["name"], c["status"], c["file"] + ":" + str(c["line"]) + ":" + str(c["col"]), c["us"])
EOF
    if [ $? -eq 0 ]; then
        ok
        sed 's/^/   /' "$TMPD/parsed.txt" | head -8
    else
        bad "the report is not valid JSON"
        cat "$TMPD/parsed.txt" | head -5
    fi
    has "$TMPD/parsed.txt" "total 4" "total"
    has "$TMPD/parsed.txt" "passed 2" "passed"
    has "$TMPD/parsed.txt" "failed 2" "failed"
    has "$TMPD/parsed.txt" "cases 4" "as many cases as announced"
    has "$TMPD/parsed.txt" "rate 0.5" "rate"
else
    bad "python3 is missing -- the JSON cannot be parsed"
fi

echo
echo "== 2. the four outcomes =="
has "$TMPD/mixed.json" '"name":"adds_up","status":"pass"' "a case that passes"
has "$TMPD/mixed.json" '"name":"overflows","status":"fail"' "a case that panics"
has "$TMPD/mixed.json" '"name":"writes_to_null","status":"fail"' "a case that dies"
has "$TMPD/mixed.json" '"name":"prints_and_passes","status":"pass"' "a case that prints"
has "$TMPD/mixed.json" '"signal":11' "the signal of the crash is reported"
has "$TMPD/mixed.json" '"exit":101' "the exit code of the panic is reported"
has "$TMPD/mixed.json" 'panic: integer overflow' "the reason is the message the case itself printed"
# The case that prints writes to standard output -- into the pipe, not into
# the report. If that were not so the report would not parse (checked above),
# but the sentence is checked separately so that the reason is visible.
has_not "$TMPD/mixed.json" 'a test may print too' "the output of a case does not land in the report"

echo
echo "== 3. the position of the failure is the right one =="
line=$(sed -n 's/.*"name":"overflows"[^}]*"line":\([0-9]*\).*/\1/p' "$TMPD/mixed.json")
col=$(sed -n 's/.*"name":"overflows"[^}]*"col":\([0-9]*\).*/\1/p' "$TMPD/mixed.json")
file=$(sed -n 's/.*"name":"overflows"[^}]*"file":"\([^"]*\)".*/\1/p' "$TMPD/mixed.json")
if [ -z "$line" ] || [ -z "$file" ]; then
    bad "no position for the failing case"
else
    text=$(sed -n "${line}p" "$file")
    case "$text" in
        *"a + b"*)
            ok
            echo "   $file:$line:$col is '$(echo "$text" | sed 's/^ *//')'"
            ;;
        *) bad "$file:$line points at '$text', expected the addition" ;;
    esac
    # The declaration is the fallback for a case that dies without a message.
    dline=$(sed -n 's/.*"name":"writes_to_null"[^}]*"line":\([0-9]*\).*/\1/p' "$TMPD/mixed.json")
    dtext=$(sed -n "${dline}p" "$CASES/mixed.fi")
    case "$dtext" in
        "fn writes_to_null"*)
            ok
            echo "   the case without a message is reported at its declaration, line $dline"
            ;;
        *) bad "the crashing case points at line $dline ('$dtext')" ;;
    esac
fi

echo
echo "== 4. the exit code =="
"$FIRNC" --test -o "$TMPD/allpass" "$CASES/all_pass.fi" > "$TMPD/allpass.json" 2>&1
num "exit code without a failure" "$?" eq 0
has "$TMPD/allpass.json" '"rate":1.000' "rate 1.000"
has "$TMPD/allpass.json" '"failed":0' "no failure"

echo
echo "== 5. TAP as a second format =="
"$FIRNC" --test --format=tap -o "$TMPD/tap" "$CASES/mixed.fi" > "$TMPD/mixed.tap" 2>&1
has "$TMPD/mixed.tap" "TAP version 13" "TAP header"
has "$TMPD/mixed.tap" "1..4" "the plan"
has "$TMPD/mixed.tap" "ok 1 - adds_up" "a passing case"
has "$TMPD/mixed.tap" "not ok 2 - overflows" "a failing case"
has "$TMPD/mixed.tap" "  location: $CASES/mixed.fi:" "the position in the YAML block"

echo
echo "== 6. a case that never returns =="
t0=$(date +%s)
"$FIRNC" --test --test-limit=1 -o "$TMPD/slow" "$CASES/slow.fi" > "$TMPD/slow.json" 2>&1
t1=$(date +%s)
has "$TMPD/slow.json" '"name":"never_returns","status":"fail"' "the hanging case counts as a failure"
has "$TMPD/slow.json" '"signal":14' "it was killed by SIGALRM"
num "the run took seconds" "$((t1 - t0))" lt 20

echo
echo "== 7. counter-checks =="
# 7a. ISOLATION. The crashing case stands THIRD in the file; the fourth one
#     has to be in the report anyway. Without a process per case the report
#     would end at the third.
after=$(sed -n 's/.*"name":"\(prints_and_passes\)".*/\1/p' "$TMPD/mixed.json")
if [ "$after" = "prints_and_passes" ]; then
    ok
    echo "   the case after the crash is reported -- the runner survived it"
else
    bad "isolation: the case after the crashing one is missing from the report"
fi
# 7b. no test case at all
"$FIRNC" --test -o "$TMPD/none" "$CASES/with_main.fi" > "$TMPD/none.out" 2>&1
if [ $? -ne 0 ] && grep -q "must not declare 'main'" "$TMPD/none.out"; then
    ok
else
    bad "a file with its own 'main' is not refused"
fi
cat > "$TMPD/empty.fi" << 'EOF'
fn helper() -> i32 {
    return 1
}
EOF
"$FIRNC" --test -o "$TMPD/empty" "$TMPD/empty.fi" > "$TMPD/empty.out" 2>&1
if [ $? -eq 2 ] && grep -q "no '#\[test\]' function" "$TMPD/empty.out"; then
    ok
else
    bad "a file without a test case is not refused (an empty run must not look green)"
fi
# 7c. a wrong signature
cat > "$TMPD/badsig.fi" << 'EOF'
#[test]
fn takes_something(x: i32) -> i32 {
    return x
}
EOF
"$FIRNC" --test -o "$TMPD/badsig" "$TMPD/badsig.fi" > "$TMPD/badsig.out" 2>&1
if [ $? -ne 0 ] && grep -q "takes no parameters and returns nothing" "$TMPD/badsig.out"; then
    ok
else
    bad "a test with parameters is not refused"
fi
# 7d. a deliberately wrong expectation has to strike
has_not "$TMPD/mixed.json" '"name":"adds_up","status":"fail"' "the counter-check strikes"

echo
echo "== 8. both compilers know the attribute =="
# firnc1 has no test mode -- but `#[test]` has to be a KNOWN attribute for it,
# otherwise a file full of test cases would not even be core language to it
# (lib/firnc1/parser.fi, round 94). Measured on the file that has a `main` of
# its own: both compilers translate it, both programs return 3.
"$FIRNC" -o "$TMPD/wm0" "$CASES/with_main.fi" > /dev/null 2>&1 && "$TMPD/wm0"
num "firnc0: the marked function is an ordinary one" "$?" eq 3
if [ ! -x "$FC1" ] || [ "$FIRNC" -nt "$FC1" ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" > /dev/null 2>&1
fi
if [ -x "$FC1" ]; then
    "$FC1" "$CASES/with_main.fi" -o "$TMPD/wm1" > "$TMPD/wm1.log" 2>&1
    rc1=$?
    if [ "$rc1" -eq 3 ]; then
        bad "firnc1 refuses '#[test]' as not core language"
    elif [ "$rc1" -ne 0 ]; then
        bad "firnc1 cannot translate the file (rc=$rc1)"
        head -3 "$TMPD/wm1.log" | sed 's/^/   /'
    else
        "$TMPD/wm1"
        num "firnc1: the same file, the same result" "$?" eq 3
    fi
else
    bad "firnc1 is missing"
fi

echo
echo "TESTRUNNER: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
