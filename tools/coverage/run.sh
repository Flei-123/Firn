#!/usr/bin/env bash
# Proof for firncov (roadmap r59/r982, docs/COVERAGE.md).
# Usage: bash tools/coverage/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
COV="$ROOT/tools/coverage/firncov"
if ! command -v valgrind >/dev/null 2>&1; then
    echo "SKIP: valgrind missing"
    exit 0
fi
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
FAIL=0
PASS=0
check() { if [ "$2" = "0" ]; then PASS=$((PASS + 1)); else echo "  FAIL  $1"; FAIL=$((FAIL + 1)); fi; }
py() { local code; code=$(cat); python3 -I -c "$code" "$@"; }
cd tools/coverage/cases

echo "== 1. known program: exact uncovered lines, exact hit counts =="
"$COV" run -o "$T/branch.json" branch.fi > "$T/branch.txt" 2>&1
check "run branch.fi" $?
py "$T/branch.json" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
t = p["files"]["branch.fi"]
dead = sorted(int(l) for l, h in t.items() if h == 0)
assert dead == [8, 9], dead
assert t["13"] == 1 and t["14"] == 1 and t["16"] == 5 and t["17"] == 5 and t["19"] == 1, t
assert t["5"] == 4, t
assert "7" not in t and "4" not in t and "6" not in t
fns = {f["name"]: f["hits"] for f in p["functions"].values() if f["file"] == "branch.fi"}
assert set(fns) == {"used", "unused", "main"}, fns
assert fns["unused"] == 0 and fns["used"] == 5 and fns["main"] == 1, fns
PY
check "uncovered lines [8,9], loop body 5 hits, used 5 calls" $?

echo "== 2. report formats =="
"$COV" report --missing "$T/branch.json" > "$T/rep.txt"
grep -q "uncovered lines: 8-9" "$T/rep.txt"
check "text report names lines 8-9" $?
grep -q "uncovered functions: unused" "$T/rep.txt"
check "text report names function unused" $?
grep -q "branch.fi  *11/13 " "$T/rep.txt"
check "text report 11/13 lines" $?
"$COV" report --format lcov "$T/branch.json" > "$T/rep.info"
py "$T/rep.info" <<'PY'
import sys
rows = open(sys.argv[1]).read().split("\n")
assert rows[0] == "TN:" and "SF:branch.fi" in rows and "end_of_record" in rows
assert "LF:13" in rows and "LH:11" in rows and "FNF:3" in rows and "FNH:2" in rows
assert "DA:8,0" in rows and "DA:16,5" in rows and "FNDA:0,unused" in rows and "FNDA:1,main" in rows
PY
check "lcov structure and counts" $?
"$COV" report --format json "$T/branch.json" > "$T/totals.json"
py "$T/totals.json" <<'PY'
import json, sys
t = json.load(open(sys.argv[1]))
assert t["lines_hit"] == 11 and t["lines"] == 13 and t["functions_hit"] == 2 and t["functions"] == 3
PY
check "json totals" $?
"$COV" report --format annotate --annotate branch.fi --root . "$T/branch.json" > "$T/ann.txt"
py "$T/ann.txt" <<'PY'
import sys
rows = open(sys.argv[1]).read().split("\n")
assert rows[7].startswith("    #####|    8|fn unused") and rows[8].startswith("    #####|    9|")
assert rows[15].startswith("        5|   16|") and rows[6].startswith("         |    7|")
PY
check "annotate marks uncovered lines" $?

echo "== 3. merge of two runs =="
"$COV" run -o "$T/left.json" left_main.fi > /dev/null; check "run left" $?
"$COV" run -o "$T/right.json" right_main.fi > /dev/null; check "run right" $?
py "$T/left.json" "$T/right.json" <<'PY'
import json, sys
for path, dead in ((sys.argv[1], [5, 6]), (sys.argv[2], [1, 2])):
    t = json.load(open(path))["files"]["shared.fi"]
    assert [int(l) for l, h in t.items() if h == 0] == dead, (path, t)
PY
check "each run alone leaves the other function uncovered" $?
"$COV" merge -o "$T/both.json" "$T/left.json" "$T/right.json"
check "merge" $?
py "$T/both.json" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
t = p["files"]["shared.fi"]
assert all(h > 0 for h in t.values()), t
assert all(f["hits"] > 0 for f in p["functions"].values() if f["file"] == "shared.fi")
assert len(p["runs"]) == 2
PY
check "merged profile covers everything" $?

echo "== 4. suite over a directory, parallel =="
"$COV" suite -j 2 -o "$T/suite.json" left_main.fi right_main.fi > /dev/null
check "suite" $?
cmp -s <(py "$T/suite.json" <<'PY'
import json, sys
print(json.load(open(sys.argv[1]))["files"]["shared.fi"])
PY
) <(py "$T/both.json" <<'PY'
import json, sys
print(json.load(open(sys.argv[1]))["files"]["shared.fi"])
PY
)
check "suite equals merge of single runs" $?

echo "== 5. build levels =="
for level in dev-fast release-safe release-fast; do
    "$COV" run --opt "$level" -o "$T/o-$level.json" branch.fi > /dev/null
    check "run at $level" $?
    py "$T/o-$level.json" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
t = p["files"]["branch.fi"]
assert t and all(1 <= int(l) <= 20 for l in t), t
assert not any(h > 0 for l, h in t.items() if int(l) in (8, 9)), t
names = {f["name"]: f["hits"] for f in p["functions"].values()}
assert names.get("main", 0) > 0, names
assert names.get("unused", 0) == 0, names
PY
    check "$level: lines inside the file, unused never hit, main hit" $?
done

echo "== 6. #[test] programs (process per case) =="
"$COV" run --test -o "$T/tests.json" ../../testrunner/cases/mixed.fi > /dev/null
check "run --test" $?
py "$T/tests.json" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
mine = [f for f in p["functions"].values() if f["file"].endswith("testrunner/cases/mixed.fi")]
assert len(mine) >= 4 and all(f["hits"] > 0 for f in mine), mine
PY
check "every test function of mixed.fi is hit, also the crashing one" $?

echo "== 7. thresholds and filters =="
"$COV" report --fail-under 90 "$T/branch.json" > /dev/null
[ $? -eq 1 ]; check "fail-under 90 strikes at 84.6 percent" $?
"$COV" report --fail-under 80 "$T/branch.json" > /dev/null
check "fail-under 80 passes" $?
"$COV" report --include nothing "$T/branch.json" | grep -q "^total  *0/0"
check "include filter drops everything else" $?
"$COV" report --exclude branch.fi "$T/both.json" | grep -q "branch.fi"
[ $? -ne 0 ]; check "exclude filter" $?

echo "== 8. counter-checks =="
echo '{"version": 7}' > "$T/bad.json"
"$COV" report "$T/bad.json" > /dev/null 2>&1
[ $? -eq 2 ]; check "unknown profile version refused (exit 2)" $?
"$COV" run -o "$T/x.json" missing_file.fi > /dev/null 2>&1
[ $? -eq 2 ]; check "missing source refused (exit 2)" $?
printf 'fn main() -> i32 {\n    return undefined_name\n}\n' > "$T/broken.fi"
"$COV" run -o "$T/x.json" "$T/broken.fi" > /dev/null 2>&1
[ $? -eq 2 ]; check "build failure refused (exit 2)" $?
py "$T/branch.json" 2>/dev/null <<'PY'
import json, sys
t = json.load(open(sys.argv[1]))["files"]["branch.fi"]
assert [int(l) for l, h in t.items() if h == 0] == [8, 10], t
PY
[ $? -ne 0 ]; check "a wrong expectation strikes" $?
"$COV" report --format annotate --annotate nothere.fi "$T/branch.json" > /dev/null 2>&1
[ $? -eq 2 ]; check "annotate of a file outside the profile refused" $?

echo "firncov: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
