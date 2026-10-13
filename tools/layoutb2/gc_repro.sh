#!/usr/bin/env bash
# tools/layoutb2/gc_repro.sh -- DETERMINISTIC reproducer for the sporadic
# "engine exited with -11" / "reflow differs" of tools/layoutb2 (roadmap r461).
#
# It is NOT part of test.sh: it fails as long as the collector bug is open.
#
# What it does: builds lib/layout/b2_main.fi with one extra thing -- the first
# argument is passed to gc_set_limit() (collection threshold in octets) -- and
# feeds the first 17 documents of the harness chunk 300..319 (the 17th is
# css/css-flexbox/image-as-flexitem-size-003.html) to it. For some thresholds
# (1800, 2000, 2100, 2300, 2600, 2700 in one sweep on 13.10.2026; the set varies
# a little from run to run -- the collector reads the clock) a collection runs at a
# point where a `Sim` (selector part) that is still referenced from a `Cpd`
# has been freed and reused: SIGSEGV in sel.match_simple. Dropping ANY single
# earlier document makes it vanish (heap state), a single document never fails.
# Without the threshold the same thing happens by timing on a loaded machine.
set -euo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W" lib/layout/b2_gcrepro.fi' EXIT
python3 - "$W" <<'PY'
import sys
w = sys.argv[1]
s = open("lib/layout/b2_main.fi").read()
anchor = "    let reflow: bool = wants_reflow(start)\n"
code = """    if rt.arg_count(start) > 1 {
        let ap: u64 = rt.arg_ptr(start, 1)
        var lim: u64 = 0
        var q: usize = 0
        while rt.ld8(ap, q) >= 48 && rt.ld8(ap, q) <= 57 {
            lim = lim * 10 + (rt.ld8(ap, q) - 48) as u64
            q = q + 1
        }
        if lim > 0 {
            gc_set_limit(lim)
        }
    }
"""
assert anchor in s
open("lib/layout/b2_gcrepro.fi", "w").write(s.replace(anchor, code + anchor, 1))
sys.path.insert(0, "tools/layoutb2")
import harness as h
ua = open(h.UA, encoding="utf-8").read()
tests = h.collect()[300:317]
open(w + "/chunk.bin", "wb").write(b"".join(
    h.encode_job(t["text"], ua, "\n".join(h.linked_sheets(t["path"], t["text"])))
    for t in tests))
print("last document:", tests[-1]["name"])
PY
"$FIRNC" -o "$W/engine" lib/layout/b2_gcrepro.fi
bad=0
for L in $(seq 1500 100 3000); do
    if "$W/engine" "$L" < "$W/chunk.bin" > /dev/null 2>&1; then
        echo "  limit $L: ok"
    else
        echo "  limit $L: CRASH (rc $?)"; bad=$((bad + 1))
    fi
done
echo "crashing thresholds: $bad of 16 (some are expected while the collector bug is open)"
[ "$bad" -eq 0 ]
