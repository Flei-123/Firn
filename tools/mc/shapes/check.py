#!/usr/bin/env python3
"""check.py <dump-output> <minecraft-data/data/pc/1.20 dir> -- compares the collision boxes of lib/mc/mc_shapes.fi (dump.fi) with
minecraft-data blockCollisionShapes for EVERY state of the modelled families (s11: panes, bars, anvils, cauldrons, hoppers, farmland, dirt path, soul sand, mud, honey), as 1/16 voxel sets (union of boxes).
Prints 'ok <family> <n>' lines, 'MISSING' for states of the families that got no shape, 'DIFF' lines; exit 1 on any difference.
Informational: block names with a partial (non-cube) shape that mc_shapes does not model."""
import json, sys
dump, d = sys.argv[1], sys.argv[2]
cs = json.load(open(d + "/blockCollisionShapes.json"))
bl = json.load(open(d + "/blocks.json"))
shapes = cs["shapes"]

def vox(boxes):
    s = set()
    for b in boxes:
        x0, y0, z0, x1, y1, z1 = [round(c * 16) for c in b]
        for x in range(x0, x1):
            for y in range(y0, y1):
                for z in range(z0, z1):
                    s.add((x, y, z))
    return s

got = {}
for line in open(dump):
    t = line.split()
    if not t or t[0] != "S":
        continue
    st, code, nb = int(t[1]), int(t[2]), int(t[3])
    nums = list(map(int, t[4:]))
    got[st] = (code, [tuple(c / 16 for c in nums[i * 6:i * 6 + 6]) for i in range(nb)])

fams = [("fence", lambda n: n.endswith("_fence")), ("fence_gate", lambda n: n.endswith("_fence_gate")),
        ("wall", lambda n: n.endswith("_wall")), ("door", lambda n: n.endswith("_door")),
        ("trapdoor", lambda n: n.endswith("_trapdoor")), ("bed", lambda n: n.endswith("_bed")),
        ("chest", lambda n: n in ("chest", "trapped_chest", "ender_chest")),
        ("pane", lambda n: n.endswith("glass_pane") or n == "iron_bars"), ("anvil", lambda n: n.endswith("anvil")),
        ("cauldron", lambda n: n.endswith("cauldron")), ("hopper", lambda n: n == "hopper"),
        ("low", lambda n: n in ("farmland", "dirt_path", "soul_sand", "mud", "honey_block"))]
bad = 0
cnt = {}
modelled = set()
for b in bl:
    name = b["name"]
    fam = next((f for f, p in fams if p(name)), None)
    if not fam:
        continue
    modelled.add(name)
    ids = cs["blocks"].get(name)
    for st in range(b["minStateId"], b["maxStateId"] + 1):
        sid = ids[st - b["minStateId"]] if isinstance(ids, list) else ids
        exp = vox(shapes[str(sid)])
        if st not in got:
            if exp:
                bad += 1
                print("MISSING", fam, name, st)
            else:
                cnt[fam] = cnt.get(fam, 0) + 1
            continue
        code, boxes = got[st]
        have = vox(boxes) if code != 65535 else set()
        if have != exp:
            bad += 1
            if bad < 30:
                print("DIFF", fam, name, st, "code", code, "missing", len(exp - have), "extra", len(have - exp))
        else:
            cnt[fam] = cnt.get(fam, 0) + 1
for f, n in cnt.items():
    print("  ok", f, n, "states")
extra = set()
for name, ids in cs["blocks"].items():
    if name in modelled or name.endswith("_slab") or name.endswith("_stairs") or name.endswith("_carpet") or name == "snow":
        continue
    for i in set(ids if isinstance(ids, list) else [ids]):
        s = shapes[str(i)]
        if s != [] and s != [[0, 0, 0, 1, 1, 1]]:
            extra.add(name)
            break
print("  info: partial shapes NOT modelled:", len(extra), "blocks e.g.", ", ".join(sorted(extra)[:12]))
print("RESULT mc-shapes-data:", "FAIL" if bad else "ok", bad)
sys.exit(1 if bad else 0)
