#!/usr/bin/env bash
# tools/textlibs_cross/run.sh -- gate for lib/text/{diff,merge3,fuzzy}.fi and lib/auth/otp.fi.
#
#   bash tools/textlibs_cross/run.sh                 (full: release-fast full size, other stages 1/3 size)
#   TEXTLIBS_SCALE=1 bash tools/textlibs_cross/run.sh   (every stage at full size, slower)
#
# 1. CROSS-CHECKS against reference tools, in all four build stages of test.sh:
#      text.diff   vs GNU diff -u / GNU patch / difflib   (diff_cross.py, Myers and patience)
#      auth.otp    vs RFC 4226/6238 vectors, Python hmac/hashlib/base64/urllib   (otp_cross.py)
#      text.merge3 vs `git merge-file`, byte for byte, 8 variants   (merge_cross.py)
#      text.fuzzy  vs a brute-force reference over all placements   (fuzzy_cross.py)
# 2. FUZZ, built with --opt-level=release-safe (every index and overflow checked): damaged unified diffs
#    through parse + apply, random three-way merges with invariants. A crash or a hang (timeout) fails.
# 3. COUNTER-CHECKS: the merge and the fuzzy cross-check must REPORT mismatches when the reference is
#    sabotaged (another marker size, another gap cost) -- else a harness that checks nothing would pass.
# Needs python3, git, GNU diff and patch. Exit 0 = no difference, no failure.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC="$ROOT/compiler/target/release/firnc"
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
SCALE=${TEXTLIBS_SCALE:-3}
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-textlibs.XXXXXX")
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
for need in python3 git diff patch; do
    command -v "$need" > /dev/null || { echo "  SKIP  $need missing"; exit 0; }
done

TOOLS="difftool otptool fuzzytool mergetool"
FULL_DIFF=3000
FULL_OTP=3000
FULL_FUZZY=3000
FULL_MERGE=3000
for stage in "release-fast:--opt-level=release-fast:1" "no-opt:--no-opt:$SCALE" "dev-fast:--opt-level=dev-fast:$SCALE" "release-safe:--opt-level=release-safe:$SCALE"; do
    name=${stage%%:*}
    rest=${stage#*:}
    opt=${rest%%:*}
    div=${rest##*:}
    ok=1
    for t in $TOOLS; do
        if ! $FIRNC $opt -o "$W/$t.$name" "tools/textlibs_cross/$t.fi" 2> "$W/err"; then
            report "$name: tools/textlibs_cross/$t.fi does not compile"
            sed 's/^/        /' "$W/err" | head -6
            ok=0
        fi
    done
    [ $ok = 1 ] || continue
    mkdir -p "$W/d.$name" "$W/m.$name"
    out=$(python3 tools/textlibs_cross/diff_cross.py "$W/difftool.$name" "$W/d.$name" $((FULL_DIFF / div)) 1 0 3 2>&1) || report "$name: diff_cross myers"
    echo "$out" | tail -2 | sed "s/^/  [$name] /"
    out=$(python3 tools/textlibs_cross/diff_cross.py "$W/difftool.$name" "$W/d.$name" $((FULL_DIFF / div)) 2 1 3 2>&1) || report "$name: diff_cross patience"
    echo "$out" | tail -2 | sed "s/^/  [$name] /"
    out=$(python3 tools/textlibs_cross/otp_cross.py "$W/otptool.$name" $((FULL_OTP / div)) 1 2>&1) || report "$name: otp_cross"
    echo "$out" | tail -2 | sed "s/^/  [$name] /"
    out=$(python3 tools/textlibs_cross/fuzzy_cross.py "$W/fuzzytool.$name" $((FULL_FUZZY / div)) 1 2>&1) || report "$name: fuzzy_cross"
    echo "$out" | tail -2 | sed "s/^/  [$name] /"
    out=$(python3 tools/textlibs_cross/merge_cross.py "$W/mergetool.$name" "$W/m.$name" $((FULL_MERGE / div)) 1 2>&1) || report "$name: merge_cross"
    echo "$out" | tail -1 | sed "s/^/  [$name] /"
done

# 2. fuzz, release-safe
if [ -x "$W/mergetool.release-safe" ]; then
    out=$(timeout 600 "$W/mergetool.release-safe" fuzz 1 4000 2>&1) || report "merge fuzz: $out"
    echo "  [fuzz] merge: $out" | tail -1
fi
if [ -x "$W/difftool.release-safe" ]; then
    python3 - "$W" <<'PY'
import random, sys
w = sys.argv[1]
r = random.Random(5)
old = ["line %d" % (i % 17) if r.random() < 0.7 else "x%d" % r.randrange(5) for i in range(120)]
new = list(old)
for _ in range(15):
    new.insert(r.randrange(len(new) + 1), "new %d" % r.randrange(99))
for _ in range(10):
    del new[r.randrange(len(new))]
open(w + "/fuzz.old", "w").write("\n".join(old) + "\n")
open(w + "/fuzz.new", "w").write("\n".join(new) + "\n")
PY
    diff -u "$W/fuzz.old" "$W/fuzz.new" > "$W/fuzz.patch"
    out=$(timeout 600 "$W/difftool.release-safe" fuzz 1 20000 "$W/fuzz.patch" "$W/fuzz.old" 2>&1) || report "patch fuzz: $out"
    echo "  [fuzz] patch: $out" | tail -1
fi

# 3. counter-checks
if [ -x "$W/mergetool.release-fast" ]; then
    mkdir -p "$W/m.sab"
    if python3 tools/textlibs_cross/merge_cross.py "$W/mergetool.release-fast" "$W/m.sab" 300 3 sabotage > "$W/sab.out" 2>&1; then
        report "counter-check: merge_cross did not notice a wrong marker size"
    else
        echo "  [counter] merge_cross caught the sabotage: $(tail -1 "$W/sab.out")"
    fi
fi
if [ -x "$W/fuzzytool.release-fast" ]; then
    if FUZZY_CROSS_SABOTAGE=1 python3 tools/textlibs_cross/fuzzy_cross.py "$W/fuzzytool.release-fast" 800 3 > "$W/sab2.out" 2>&1; then
        report "counter-check: fuzzy_cross did not notice a wrong gap cost"
    else
        echo "  [counter] fuzzy_cross caught the sabotage: $(tail -1 "$W/sab2.out")"
    fi
fi
if [ $ERRORS = 0 ]; then
    echo "textlibs_cross: OK"
else
    echo "textlibs_cross: $ERRORS failure(s)"
fi
exit $((ERRORS > 0))
