#!/usr/bin/env bash
# tools/testrunner/suite.sh -- gate for the suite runner (roadmap r26).
#
# The runner (tools/testrunner, Rust, std only) compiles and runs the test
# programs of a tree (tests/, tests/opt/, examples/, tests/neg/) in the four
# build levels and reports machine readable: json, jsonl, junit or text.
# This gate runs it over tools/testrunner/suite_cases, a tree whose result is
# known case by case, and checks:
#   1. json: counts, keys, every case present, exit code 1
#   2. the position of a failure is the statement that really failed
#   3. jsonl: every line parses, one summary line at the end
#   4. junit: valid XML, counts, failure text carries file:line:col
#   5. skips (only_mode / not_mode), unit files via `firnc --test`, --levels
#   6. order and result do not depend on --jobs
#   7. counter-checks: all green exits 0, empty selection and bad options
#      exit 2, a hanging program is killed by --timeout
# Usage: bash tools/testrunner/suite.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
CASES=tools/testrunner/suite_cases

D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); }
bad() {
    FAIL=$((FAIL + 1))
    echo "  FAIL  $1"
}
check() { # label command...
    local label=$1
    shift
    if "$@" > "$D/check.out" 2>&1; then ok; else bad "$label"; head -5 "$D/check.out" | sed 's/^/        /'; fi
}

if [ ! -x "$FIRNC" ]; then
    echo "compiler missing: $FIRNC"
    exit 1
fi
CARGO_TARGET_DIR="$D/target" cargo build --release --manifest-path tools/testrunner/Cargo.toml > "$D/build.log" 2>&1 || {
    cat "$D/build.log"
    exit 1
}
TR="$D/target/release/testrunner"
mkdir -p "$D/tmp"
export TMPDIR="$D/tmp"
run() { "$TR" --root "$CASES" --compiler "$FIRNC" "$@"; }

echo "== 1. json =="
run --format json --out "$D/all.json" --quiet 2> /dev/null
rc=$?
if [ "$rc" -eq 1 ]; then ok; else bad "exit code with failures is $rc, expected 1"; fi
cat > "$D/check_json.py" << 'PY'
import json, sys
d = json.load(open(sys.argv[1]))
want = {"total": 32, "passed": 15, "failed": 17, "skipped": 4}
for k, v in want.items():
    assert d[k] == v, (k, d[k], v)
assert len(d["cases"]) == 36, len(d["cases"])
assert abs(d["rate"] - 15 / 32) < 1e-3, d["rate"]
keys = {"name", "mode", "level", "kind", "status", "message", "duration_ms", "duration_us", "file", "line", "col"}
for c in d["cases"]:
    assert keys <= set(c), c
    assert c["status"] in ("pass", "fail", "skip"), c
kinds = {c["kind"] for c in d["cases"]}
assert kinds == {"run", "negative", "unit"}, kinds
levels = {c["level"] for c in d["cases"]}
assert levels == {"opt", "noopt", "devfast", "safe", "default"}, levels
PY
check "json parses and the numbers agree" python3 -I "$D/check_json.py" "$D/all.json"

echo "== 2. positions =="
cat > "$D/check_pos.py" << 'PY'
import json, sys
root = sys.argv[2]
d = json.load(open(sys.argv[1]))
def src(c):
    return open(root + "/" + c["file"]).read().split("\n")[c["line"] - 1]
def find(name, level):
    return [c for c in d["cases"] if c["name"] == name and c["level"] == level][0]
c = find("tests/007_panic_location.fi", "devfast")
assert c["status"] == "fail" and "a + b" in src(c) and c["col"] == 9, c
c = find("tests/008_unit_cases.fi::overflows", "devfast")
assert c["status"] == "fail" and "a + b" in src(c), c
assert c["kind"] == "unit", c
c = find("tests/008_unit_cases.fi::adds_up", "devfast")
assert c["status"] == "pass", c
c = find("tests/009_compile_error.fi", "safe")
assert c["status"] == "fail" and "missing_name" in src(c), c
c = find("tests/003_exit_wrong.fi", "opt")
assert c["status"] == "fail" and c["line"] == 1 and "expect_exit" in src(c), c
c = find("tests/007_panic_location.fi", "opt")
assert c["status"] == "pass", c
PY
check "failure positions point at the failing statement" python3 -I "$D/check_pos.py" "$D/all.json" "$CASES"

sed 's/"file":"tests\/007_panic_location.fi","line":5/"file":"tests\/007_panic_location.fi","line":4/g' "$D/all.json" > "$D/shifted.json"
if python3 -I "$D/check_pos.py" "$D/shifted.json" "$CASES" > /dev/null 2>&1; then bad "counter-check: a shifted line must strike"; else ok; fi

echo "== 3. jsonl =="
run --format jsonl --quiet > "$D/all.jsonl" 2> /dev/null
cat > "$D/check_jsonl.py" << 'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
assert len(rows) == 37, len(rows)
assert "summary" in rows[-1] and rows[-1]["summary"]["failed"] == 17, rows[-1]
assert all("summary" not in r and "name" in r and "status" in r for r in rows[:-1])
PY
check "jsonl: one object per line, summary last" python3 -I "$D/check_jsonl.py" "$D/all.jsonl"

echo "== 4. junit =="
run --format junit --quiet > "$D/all.xml" 2> /dev/null
cat > "$D/check_xml.py" << 'PY'
import sys
import xml.etree.ElementTree as ET
r = ET.parse(sys.argv[1]).getroot()
assert r.tag == "testsuites"
assert r.get("tests") == "36" and r.get("failures") == "17" and r.get("skipped") == "4", r.attrib
suites = {s.get("name"): s for s in r.findall("testsuite")}
assert set(suites) == {"opt", "noopt", "devfast", "safe", "default"}, set(suites)
assert sum(int(s.get("tests")) for s in suites.values()) == 36
fails = r.findall(".//failure")
assert len(fails) == 17
panic = [f for f in fails if "007_panic_location.fi:5:9" in (f.text or "")]
assert len(panic) == 3, len(panic)
assert len(r.findall(".//skipped")) == 4
for tc in r.iter("testcase"):
    assert float(tc.get("time")) >= 0 and tc.get("name") and tc.get("line")
PY
check "junit is valid XML with the right counts" python3 -I "$D/check_xml.py" "$D/all.xml"

echo "== 5. skips, unit files, levels =="
run --format json --levels opt --quiet --out "$D/opt.json" 2> /dev/null
cat > "$D/check_opt.py" << 'PY'
import json, sys
d = json.load(open(sys.argv[1]))
assert {c["level"] for c in d["cases"]} == {"opt", "default"}, {c["level"] for c in d["cases"]}
assert not [c for c in d["cases"] if c["kind"] == "unit"]
sk = [c for c in d["cases"] if c["status"] == "skip"]
assert [c["name"] for c in sk] == ["tests/006_not_opt.fi"], sk
assert sk[0]["message"] == "not_mode: opt"
PY
check "--levels opt: only opt, no unit file, not_mode skipped" python3 -I "$D/check_opt.py" "$D/opt.json"
run --format json --levels safe --quiet --out "$D/safe.json" 2> /dev/null
check "--levels safe: only_mode: opt is skipped" python3 -I -c "
import json
d = json.load(open('$D/safe.json'))
sk = [c['name'] for c in d['cases'] if c['status'] == 'skip']
assert sk == ['tests/005_only_opt.fi'], sk
"

echo "== 6. --jobs does not change the result =="
run --format jsonl --quiet --jobs 1 > "$D/j1.jsonl" 2> /dev/null
run --format jsonl --quiet --jobs 8 > "$D/j8.jsonl" 2> /dev/null
cat > "$D/same.py" << 'PY'
import json, sys
def load(p):
    return [(r["name"], r.get("level"), r.get("status")) for r in map(json.loads, open(p)) if "name" in r]
a, b = load(sys.argv[1]), load(sys.argv[2])
assert a == b and len(a) == 36, (len(a), len(b))
PY
check "same cases in the same order with 1 and 8 jobs" python3 -I "$D/same.py" "$D/j1.jsonl" "$D/j8.jsonl"

echo "== 7. counter-checks =="
run --filter 001_ --quiet > "$D/green.txt" 2>&1
rc=$?
if [ "$rc" -eq 0 ]; then ok; else bad "an all green selection must exit 0 (got $rc)"; fi
if grep -q '^4 of 4 passed' "$D/green.txt"; then ok; else bad "text summary of the green selection"; cat "$D/green.txt"; fi
run --filter no_such_case_anywhere > /dev/null 2>&1
rc=$?
if [ "$rc" -eq 2 ]; then ok; else bad "an empty selection must exit 2 (got $rc)"; fi
run --levels bogus > /dev/null 2>&1
rc=$?
if [ "$rc" -eq 2 ]; then ok; else bad "an unknown level must exit 2 (got $rc)"; fi
run --format yaml > /dev/null 2>&1
rc=$?
if [ "$rc" -eq 2 ]; then ok; else bad "an unknown format must exit 2 (got $rc)"; fi
"$TR" --root "$CASES" --compiler "$D/no_such_compiler" > /dev/null 2>&1
rc=$?
if [ "$rc" -eq 2 ]; then ok; else bad "a missing compiler must exit 2 (got $rc)"; fi

mkdir -p "$D/hang/tests"
cat > "$D/hang/tests/001_hang.fi" << 'FI'
// expect_exit: 0
fn main() -> i32 {
    var i: i64 = 0
    while i >= 0 {
        i = i + 1
        if i > 1000000000 {
            i = 0
        }
    }
    return 0 as i32
}
FI
t0=$(date +%s)
"$TR" --root "$D/hang" --compiler "$FIRNC" --levels devfast --timeout 2 --format json --out "$D/hang.json" 2> /dev/null
rc=$?
t1=$(date +%s)
if [ "$rc" -eq 1 ] && grep -q 'timed out after 2 s' "$D/hang.json"; then ok; else bad "a hanging program must fail with a timeout (rc=$rc)"; fi
if [ $((t1 - t0)) -lt 30 ]; then ok; else bad "the hang was not cut off by --timeout ($((t1 - t0)) s)"; fi

if [ -n "$(ls -A "$D/tmp")" ]; then bad "scratch directory left behind"; else ok; fi

echo
echo "SUITE RUNNER: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
