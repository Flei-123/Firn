#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Proves that NOTHING was changed in the css-parsing-tests data.
#
# Three things are checked:
#   1. Exactly the 12 expected files lie in testdata/css-parsing-tests/ --
#      no more, no fewer.
#   2. Every file has exactly the sha256 sum from tools/css/testdata.sha256.
#      These sums come from the upstream commit
#      203ce36bffd617db7f118c551e32794561fb273d of
#      https://github.com/SimonSapin/css-parsing-tests
#   3. The number of cases is counted (expectation: 305), so that a change
#      which happened to keep the sum would show up as well.
#
# With --against-upstream the files of the fixed commit are loaded from
# GitHub once more and compared directly (needs the network; without it the
# switch is a clean error, not a silent success).
#
# Usage:  bash tools/css/verify_testdata.sh [--against-upstream]
# Return: 0 = everything unchanged, 1 = a deviation was found.
set -euo pipefail
cd "$(dirname "$0")/../.."

COMMIT="203ce36bffd617db7f118c551e32794561fb273d"
DATA="testdata/css-parsing-tests"
SUMS="tools/css/testdata.sha256"
EXPECTED_FILES=12
EXPECTED_CASES=305
AGAINST_UPSTREAM=0
[ "${1:-}" = "--against-upstream" ] && AGAINST_UPSTREAM=1

errs=0

echo "== check the test data: $DATA =="
echo "   reference: css-parsing-tests @ $COMMIT"

# --- 1. the set of files ---------------------------------------------------
present=$(cd "$DATA" && ls -1 2>/dev/null | sort)
expected=$(awk '!/^#/ && NF==2 {print $2}' "$SUMS" | sort)
count=$(printf '%s\n' "$present" | grep -c . || true)

if [ "$present" != "$expected" ]; then
    echo "   ERROR: the set of files differs."
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$present") \
        | sed 's/^/          /' || true
    errs=1
fi
if [ "$count" -ne "$EXPECTED_FILES" ]; then
    echo "   ERROR: $count files instead of $EXPECTED_FILES"
    errs=1
else
    echo "   files   : $count (expected $EXPECTED_FILES)"
fi

# --- 2. sha256 against the fixed set ---------------------------------------
if (cd "$DATA" && grep -v '^#' "../../$SUMS" | grep . | sha256sum -c --status -); then
    echo "   sha256  : all $EXPECTED_FILES sums match"
else
    echo "   ERROR: at least one sum differs:"
    (cd "$DATA" && grep -v '^#' "../../$SUMS" | grep . | sha256sum -c - 2>&1 \
        | grep -v ': OK$' | sed 's/^/          /') || true
    errs=1
fi

# --- 3. count the cases ----------------------------------------------------
cases=$(python3 - "$DATA" <<'PY'
import glob, json, os, sys
n = 0
for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.json"))):
    d = json.load(open(path, encoding="utf-8"))
    n += len(d) // 2
print(n)
PY
)
if [ "$cases" -ne "$EXPECTED_CASES" ]; then
    echo "   ERROR: $cases test cases instead of $EXPECTED_CASES"
    errs=1
else
    echo "   cases   : $cases (expected $EXPECTED_CASES)"
fi

# --- 4. optional: directly against upstream --------------------------------
if [ "$AGAINST_UPSTREAM" -eq 1 ]; then
    echo
    echo "== direct comparison with GitHub (commit $COMMIT) =="
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    for f in $(printf '%s\n' "$expected"); do
        url="https://raw.githubusercontent.com/SimonSapin/css-parsing-tests/$COMMIT/$f"
        if ! curl -sSfL --max-time 60 -o "$tmp/$f" "$url"; then
            echo "   ERROR: $f could not be loaded ($url)"
            errs=1
            continue
        fi
        if cmp -s "$tmp/$f" "$DATA/$f"; then
            echo "   $f: identical"
        else
            echo "   $f: DIFFERS from upstream"
            errs=1
        fi
    done
fi

echo
if [ "$errs" -eq 0 ]; then
    echo "OK: test data unchanged (12 files, 305 cases, sha256 as upstream)."
    exit 0
fi
echo "FAILED: the test data differs from the recorded state."
exit 1
