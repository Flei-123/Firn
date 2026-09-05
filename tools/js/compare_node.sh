#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/js/compare_node.sh -- the CROSS CHECK against a foreign engine.
#
# The same small programs run through node and through this engine; the
# output is compared CHARACTER FOR CHARACTER. `print` is the only thing the
# two have to agree on, so node gets a two line preamble that defines it.
#
# node is a CHECKING INSTANCE, nothing else -- no line of it is in the
# engine. If node is missing the check reports that and fails; it does not
# quietly succeed.
set -euo pipefail
cd "$(dirname "$0")/../.."
ENGINE="${1:-.js-work/jsrun}"
CASES="tools/js/cases"
WORK="${2:-.js-work/cmp}"
mkdir -p "$WORK"

if ! command -v node >/dev/null 2>&1; then
    echo "FAILED: node is not installed -- the cross check cannot run."
    exit 1
fi
echo "reference: $(node --version)"

same=0
diffs=0
for f in "$CASES"/*.js; do
    b=$(basename "$f")
    { echo 'const print = (...a) => console.log(a.map(x => typeof x === "symbol" ? x.toString() : String(x)).join(" "));'; cat "$f"; } > "$WORK/$b.node.cjs"
    node "$WORK/$b.node.cjs" > "$WORK/$b.node.out" 2>&1 || true
    python3 - "$ENGINE" "$f" > "$WORK/$b.firn.out" <<'PY'
import struct, subprocess, sys
src = open(sys.argv[2], "rb").read()
blob = struct.pack("<II", 0, len(src)) + src
r = subprocess.run([sys.argv[1]], input=blob, stdout=subprocess.PIPE)
block = r.stdout.decode("utf-8", "replace").split("\x00\n")[0]
lines = block.rstrip("\n").split("\n")
if lines and lines[-1] == "OK":
    lines = lines[:-1]
sys.stdout.write("\n".join(lines) + ("\n" if lines else ""))
PY
    if cmp -s "$WORK/$b.node.out" "$WORK/$b.firn.out"; then
        echo "   same     $b"
        same=$((same + 1))
    else
        echo "   DIFFERS  $b"
        diff "$WORK/$b.node.out" "$WORK/$b.firn.out" | head -14 | sed 's/^/            /' || true
        diffs=$((diffs + 1))
    fi
done
echo
echo "cross check: $same identical, $diffs differing"
[ "$diffs" -eq 0 ]
