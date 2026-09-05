#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/regexp_compare.sh -- the PATTERN ENGINE against node, character
# for character.
#
# test262 says whether a case passes. It does NOT say whether two engines
# agree on the value a pattern produces, and that is exactly where a
# backtracking matcher goes wrong quietly: the same match, a different
# capture. So the same programs run through both, and the output is
# compared octet by octet. `node` is a CHECKING INSTANCE and nothing of it
# is in the production path.
#
# Without node the comparison is SKIPPED, not silently passed.
#
# Usage:  bash tools/js/regexp_compare.sh [engine]
set -uo pipefail
cd "$(dirname "$0")/../.."
ENGINE="${1:-.js-work/jsrun}"
WORK=".js-work/recmp"
mkdir -p "$WORK"

if ! command -v node > /dev/null 2>&1; then
    echo "node is not there -- the comparison is SKIPPED (not passed)."
    exit 0
fi

CASES="tools/js/cases/re_01_basic.js tools/js/cases/re_02_groups.js \
tools/js/cases/re_03_replace.js tools/js/cases/re_04_split.js"

BAD=0
for c in $CASES; do
    name=$(basename "$c")
    # node has no `print`; it gets the same three lines in front.
    {
        echo 'function print(){console.log(Array.prototype.join.call(arguments," "));}'
        cat "$c"
    } > "$WORK/$name.node.cjs"
    node "$WORK/$name.node.cjs" > "$WORK/$name.node.out" 2>&1
    python3 - "$ENGINE" "$c" > "$WORK/$name.firn.out" 2>&1 <<'PY'
import struct, subprocess, sys
engine, path = sys.argv[1], sys.argv[2]
src = open(path, "rb").read()
job = struct.pack("<II", 0, len(src)) + src
p = subprocess.run([engine], input=job, stdout=subprocess.PIPE, timeout=120)
block = p.stdout.decode("utf-8", "replace").split("\x00")[0]
lines = block.rstrip("\n").split("\n")
if lines and lines[-1] == "OK":
    lines.pop()
sys.stdout.write("\n".join(lines) + "\n")
PY
    if cmp -s "$WORK/$name.node.out" "$WORK/$name.firn.out"; then
        printf "%-24s identical (%s lines)\n" "$name" \
            "$(wc -l < "$WORK/$name.firn.out")"
    else
        printf "%-24s DIFFERENT\n" "$name"
        diff "$WORK/$name.node.out" "$WORK/$name.firn.out" | head -10
        BAD=$((BAD + 1))
    fi
done

if [ "$BAD" -ne 0 ]; then
    echo "FAILED: $BAD program(s) differ from node."
    exit 1
fi
echo "OK: the pattern engine agrees with node character for character."
