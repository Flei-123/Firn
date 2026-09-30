#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/apptree.sh -- THE MEASUREMENT BEHIND docs/APP-TREE.md.
#
# Builds tools/fui/apptree_main.fi three times and runs each build
# three times (15 rounds each), then prints the median of every value:
#
#   cap128   the library as it is (scene.SCENE_MAX = 128)
#   big      a COPY of lib/ with SCENE_MAX = A11Y_MAX = 1024, so the
#            synthetic trees can grow to 1009 nodes
#   cull     that copy plus the culling EXPERIMENT: before drawing, every
#            subtree gets its ink bounds (its rectangle and its children's,
#            40 points of margin, transformed nodes and nodes with their own
#            painter never culled), and draw_node skips a subtree whose
#            bounds miss the current clip. The pictures must stay the same
#            octets (pix= is a checksum of the canvas); the script says
#            WRONG otherwise.
#
# Nothing in lib/ is changed; the copies live in $W (default
# /tmp/fui-apptree). The numbers depend on the machine and its load --
# docs/APP-TREE.md names the conditions of the run it quotes.
#
#     tools/fui/apptree.sh            all three
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
s/nodes: \[node_blank(); 128\]/nodes: [node_blank(); 1024]/' \
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

# ---- the culling experiment on top of it
cp -r "$W/lib1024" "$W/libcull"
python3 - "$W/libcull/fui/scene.fi" <<'EOF'
import sys
p = sys.argv[1]
s = open(p).read()
def rep(old, new):
    global s
    if old not in s:
        sys.exit("apptree.sh: scene.fi changed, the culling patch no longer fits: " + old[:40])
    s = s.replace(old, new, 1)
rep('import fui.viewport\n', 'import fui.viewport\nimport fui.painter\nimport paint.canvas\n')
rep('''    du: u64,
    has_dr: u32,
}''', '''    du: u64,
    has_dr: u32,
    bx0: f64,
    by0: f64,
    bx1: f64,
    by1: f64,
    bxf: u32,
}''')
rep('''        dr: draw_none, du: 0, has_dr: 0 as u32,
    }''', '''        dr: draw_none, du: 0, has_dr: 0 as u32,
        bx0: 0.0, by0: 0.0, bx1: 0.0, by1: 0.0, bxf: 0 as u32,
    }''')
rep('fn draw_node(sc: *mut Scene, c: *mut render.Ctx, i: usize) {', '''fn cull_bounds(sc: *mut Scene) {
    var k: usize = 0
    while k < (*sc).n {
        let nd: *mut Node = &(*sc).nodes[k]
        (*nd).bx0 = (*nd).x - 40.0
        (*nd).by0 = (*nd).y - 40.0
        (*nd).bx1 = (*nd).x + (*nd).w + 40.0
        (*nd).by1 = (*nd).y + (*nd).h + 40.0
        (*nd).bxf = 0 as u32
        if (*nd).has_dr != 0 as u32 || sheet.decl_has(&(*nd).d,
            sheet.EX_TRANSLATE | sheet.EX_SCALE | sheet.EX_ROTATE, true) {
            (*nd).bxf = 1 as u32
        }
        k = k + 1
    }
    var i: usize = (*sc).n
    while i > 0 {
        i = i - 1
        let nd: *mut Node = &(*sc).nodes[i]
        let p: usize = (*nd).parent
        if p != SCENE_NONE && p < (*sc).n {
            let pn: *mut Node = &(*sc).nodes[p]
            if (*nd).bx0 < (*pn).bx0 { (*pn).bx0 = (*nd).bx0 }
            if (*nd).by0 < (*pn).by0 { (*pn).by0 = (*nd).by0 }
            if (*nd).bx1 > (*pn).bx1 { (*pn).bx1 = (*nd).bx1 }
            if (*nd).by1 > (*pn).by1 { (*pn).by1 = (*nd).by1 }
            if (*nd).bxf != 0 as u32 { (*pn).bxf = 1 as u32 }
        }
    }
}

fn draw_node(sc: *mut Scene, c: *mut render.Ctx, i: usize) {''')
rep('''    if rw <= 0.0 || rh <= 0.0 {
        return
    }
    let kind: u32 = (*nd).kind''', '''    if rw <= 0.0 || rh <= 0.0 {
        return
    }
    if (*nd).bxf == 0 as u32 {
        let cv: *mut canvas.Canvas = painter.painter_canvas(render.ctx_painter(c))
        if (*nd).bx1 <= canvas.clip_x0(cv) || (*nd).bx0 >= canvas.clip_x1(cv)
        || (*nd).by1 <= canvas.clip_y0(cv) || (*nd).by0 >= canvas.clip_y1(cv) {
            return
        }
    }
    let kind: u32 = (*nd).kind''')
rep('''    (*sc).xfof = 0
    draw_node(sc, c, (*sc).root)''', '''    (*sc).xfof = 0
    cull_bounds(sc)
    draw_node(sc, c, (*sc).root)''')
open(p, "w").write(s)
EOF
FIRNLIB="$W/libcull" "$FIRNC" --opt-level=release-fast -o "$W/cull" \
    tools/fui/apptree_main.fi

# ---- three runs of each, medians
cd "$W"
python3 - <<'EOF'
import re, statistics, subprocess, sys
builds = [("cap128", "./cap128", "128"), ("big", "./big", "1024"),
          ("cull", "./cull", "1024")]
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
