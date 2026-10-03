#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/apptree.sh -- THE MEASUREMENT BEHIND docs/APP-TREE.md.
#
# Builds tools/fui/apptree_main.fi twice and runs each build three
# times (15 rounds each), then prints the median of every value:
#
#   cap128   the library as it is (scene.SCENE_MAX = 128)
#   big      a COPY of lib/ with SCENE_MAX = A11Y_MAX = 1024, so the
#            synthetic trees can grow to 1009 nodes
#
# Every run also paints each synthetic tree without culling (draw_nocull_us,
# the library culls since r101) and measures it again with the measure memo
# (measure_memo_us, r100). The pictures must stay the same octets (pix= is a
# checksum of the canvas); the script says WRONG otherwise.
#
# Nothing in lib/ is changed; the copies live in $W (default
# /tmp/fui-apptree). The numbers depend on the machine and its load --
# docs/APP-TREE.md names the conditions of the run it quotes.
#
#     tools/fui/apptree.sh            both builds
#     tools/fui/apptree.sh --quick    cap128 only, one run
set -e
cd "$(dirname "$0")/../.."
ROOT="$(pwd)"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
W="${W:-/tmp/fui-apptree}"
rm -rf "$W"
mkdir -p "$W"

FIRNLIB="$ROOT/lib" "$FIRNC" --opt-level=release-fast -o "$W/cap128" \
    tools/fui/apptree_main.fi
if [ "$1" = "--quick" ]; then
    "$W/cap128" 128 5
    exit $?
fi

# ---- the copy with room for 1024 nodes
cp -r "$ROOT/lib" "$W/lib1024"
sed -i 's/^const SCENE_MAX: usize = 128$/const SCENE_MAX: usize = 1024/
s/nodes: \[Node; 128\],/nodes: [Node; 1024],/
s/nodes: \[node_blank(); 128\]/nodes: [node_blank(); 1024]/
s/s: \[MemoSlot; 128\],/s: [MemoSlot; 1024],/
s/\[memo_slot_blank(); 128\]/[memo_slot_blank(); 1024]/' \
    "$W/lib1024/fui/scene.fi"
sed -i 's/^const A11Y_MAX: usize = 128$/const A11Y_MAX: usize = 1024/
s/a: \[Ann; 128\],/a: [Ann; 1024],/
s/ord: \[usize; 128\],/ord: [usize; 1024],/
s/A11y { a: \[ann_blank(); 128\], ord: \[0; 128\], nord: 0 }/A11y { a: [ann_blank(); 1024], ord: [0; 1024], nord: 0 }/' \
    "$W/lib1024/fui/a11y.fi"
if grep -q '128\]' "$W/lib1024/fui/scene.fi" "$W/lib1024/fui/a11y.fi"; then
    echo "apptree.sh: the 1024 copy still has a 128-array -- the patch no longer fits"
    exit 1
fi
FIRNLIB="$W/lib1024" "$FIRNC" --opt-level=release-fast -o "$W/big" \
    tools/fui/apptree_main.fi

# ---- three runs of each, medians
# (culling is in the library since r101: every run paints with and without
# it, draw_us / draw_nocull_us, and fails when the octets differ)
cd "$W"
python3 - <<'EOF'
import re, statistics, subprocess, sys
builds = [("cap128", "./cap128", "128"), ("big", "./big", "1024")]
runs = {}
for rep in range(3):
    for name, exe, mx in builds:
        r = subprocess.run([exe, mx, "15"], capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit("apptree.sh: %s exited %d\n%s" % (name, r.returncode, r.stdout))
        for line in r.stdout.splitlines():
            kv = dict(re.findall(r"(\w+)=([\d.]+)", line))
            if line.startswith("apptree bytes"):
                runs.setdefault((name, "bytes"), [line])
            elif line.startswith("apptree tree="):
                key = (name, line.split()[1][5:] + "/" + kv["nodes"])
                runs.setdefault(key, []).append(kv)
wrong = 0
pix = {}
for (name, tree), vals in sorted(runs.items()):
    if tree == "bytes":
        print(name, vals[0])
        continue
    sums = sorted(set(v["pix"] for v in vals))
    pix.setdefault(tree, set()).update(sums)
    med = {k: statistics.median(float(v[k]) for v in vals)
           for k in vals[0] if k != "pix"}
    print("%-7s %-13s" % (name, tree), " ".join(
        "%s=%g" % (k, med[k]) for k in med if k not in ("nodes",)),
        "pix=" + ",".join(sums))
for tree, sums in pix.items():
    if len(sums) != 1:
        print("apptree.sh: WRONG -- %s painted different octets: %s" % (tree, sorted(sums)))
        wrong += 1
sys.exit(1 if wrong else 0)
EOF
