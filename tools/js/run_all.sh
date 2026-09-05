#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/run_all.sh -- the engine over the whole test262 subset, DIRECTORY
# BY DIRECTORY and several directories at a time.
#
# Each directory gets its own process and its own hard time limit, so that a
# single case that never terminates cannot stop the whole run. The JSON
# files are added up at the end into one balance -- and the balance counts
# every case, the ones that were never reached included.
set -uo pipefail
cd "$(dirname "$0")/../.."
ENGINE="${1:-.js-work/jsrun}"
OUT="${2:-.js-work/per}"
JOBS="${JS_JOBS:-6}"
LIMIT="${JS_DIR_TIMEOUT:-1800}"
export T262="${T262:-$(pwd)/.js-work/t262}"
mkdir -p "$OUT"

dirs=$(cd "$T262/test" && ls -d language/*/ built-ins/*/ 2>/dev/null | sed 's|/$||')
printf '%s\n' "$dirs" | xargs -P "$JOBS" -I{} bash -c '
    d="{}"
    n=$(echo "$d" | tr "/" "_")
    if [ -f "'"$OUT"'/$n.json" ]; then exit 0; fi
    timeout '"$LIMIT"' python3 tools/js/harness_run.py "'"$ENGINE"'" \
        --dir "test/$d" --json "'"$OUT"'/$n.json" > "'"$OUT"'/$n.txt" 2>&1
    printf "%-40s %s\n" "$d" "$(grep -E "^quota" "'"$OUT"'/$n.txt" | head -1)"
'

python3 - "$OUT" "$T262" <<'PY'
import glob, json, os, sys
out, t262 = sys.argv[1], sys.argv[2]
tot = passed = 0
reasons = {}
seen = set()
for f in sorted(glob.glob(os.path.join(out, "*.json"))):
    d = json.load(open(f))
    tot += d["total"]
    passed += d["passed"]
    seen.add(os.path.basename(f)[:-5])
    for k, v in d.get("reasons", {}).items():
        reasons[k] = reasons.get(k, 0) + v
# A directory whose run did not finish counts with ALL of its cases as a
# failure -- silently dropping it would flatter the quota.
missing = 0
base = os.path.join(t262, "test")
for group in ("language", "built-ins"):
    gdir = os.path.join(base, group)
    if not os.path.isdir(gdir):
        continue
    for d in sorted(os.listdir(gdir)):
        key = "%s_%s" % (group, d)
        if key in seen:
            continue
        n = sum(len([x for x in fs if x.endswith(".js")])
                for _, _, fs in os.walk(os.path.join(gdir, d)))
        missing += n
        tot += n
if missing:
    reasons["not-reached"] = missing
print()
print("runs        : %d" % tot)
print("passed      : %d" % passed)
print("failed      : %d" % (tot - passed))
print("quota       : %.2f%%" % (100.0 * passed / tot if tot else 0))
print("failures by cause:")
for k in sorted(reasons, key=lambda x: -reasons[x]):
    print("   %-22s %6d" % (k, reasons[k]))
json.dump({"total": tot, "passed": passed, "failed": tot - passed,
           "reasons": reasons},
          open(os.path.join(out, os.pardir, "run.json"), "w"))
PY
