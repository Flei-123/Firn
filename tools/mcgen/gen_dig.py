#!/usr/bin/env python3
"""gen_dig.py <data_dir> <version> <out.fi> -- block material / tool speed / harvest table of one version.

Source: minecraft-data blocks.json (material, harvestTools, block order = registry order of gen_registry.py)
and materials.json (item id -> mining speed per material). Reader: lib/mc/mc_dig.fi.

BLOB (u28 numbers as in the registry, 4 octets of 7 bits each):
  "FDG1" nblocks nmat matdir_off blocks_off
  matdir_off: nmat offsets of the material records
  material record: n, n * (item_id, speed_milli)   (speed of that item on blocks of the material)
  blocks_off: nblocks * (material index, harvest_off)   (harvest_off 0 = any tool harvests)
  harvest list: n, n * item_id   (only these items drop the block)

1.20.5+ (protocol >= 766): the tool speed no longer comes from minecraft-data's materials (its 1.21 "materials" are
tag names that miss e.g. iron ore, and copper tools carry a placeholder speed) but from the items' default
`minecraft:tool` component and the block tags - public data of misode/mcmeta (`<version>-summary` item_components,
`<version>-data` tags; cached in $MCMETA_CACHE, default /tmp/mcmeta-cache). Each block gets a synthetic material = the
vector of its tool speeds; the harvest list = tools whose first matching `correct_for_drops` rule is true (blocks that need
a correct tool at all still come from minecraft-data harvestTools). Same blob format, same reader.
"""
import json, os, sys, urllib.request
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gen_registry as gr


def build(data_dir, version):
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    vt = json.load(open(os.path.join(HERE, "versions.json")))["versions"]
    key = vt[version]["data"] if version in vt else version
    P = paths[key]
    blocks = json.load(open(os.path.join(data_dir, P["blocks"], "blocks.json")))
    mats = json.load(open(os.path.join(data_dir, P["materials"], "materials.json")))
    for b in blocks:
        if "minStateId" not in b:
            b["minStateId"] = b["id"] * 16
    blocks = sorted(blocks, key=lambda b: b["minStateId"])
    names = sorted(set(b.get("material") or "default" for b in blocks) | set(mats.keys()))
    midx = {n: i for i, n in enumerate(names)}
    num = gr.num
    recs = []
    for n in names:
        ent = sorted((int(k), int(round(float(v) * 1000))) for k, v in mats.get(n, {}).items())
        r = bytearray(num(len(ent)))
        for it, sp in ent:
            r += num(it) + num(sp)
        recs.append(bytes(r))
    nb, nm = len(blocks), len(names)
    matdir_off = 4 + 4 * 4
    mats_off = matdir_off + 4 * nm
    blocks_off = mats_off + sum(len(r) for r in recs)
    harv_off = blocks_off + nb * 8
    harv = bytearray()
    brec = bytearray()
    for b in blocks:
        ht = b.get("harvestTools")
        off = 0
        if ht:
            off = harv_off + len(harv)
            ids = sorted(int(k) for k, v in ht.items() if v)
            harv += num(len(ids)) + b"".join(num(i) for i in ids)
        brec += num(midx[b.get("material") or "default"]) + num(off)
    out = bytearray(b"FDG1")
    for v in (nb, nm, matdir_off, blocks_off):
        out += num(v)
    pos = mats_off
    for r in recs:
        out += num(pos)
        pos += len(r)
    for r in recs:
        out += r
    assert len(out) == blocks_off
    out += brec + harv
    return bytes(out), nb, nm, key


MCMETA = "https://raw.githubusercontent.com/misode/mcmeta/%s/%s"
SPEED_MAX_MILLI = 268435455  # u28; "instantly mines" rules carry 3.4e38


def fetch_cached(ref, path):
    cache = os.environ.get("MCMETA_CACHE", "/tmp/mcmeta-cache")
    f = os.path.join(cache, ref, path.replace("/", "__"))
    if not os.path.exists(f):
        os.makedirs(os.path.dirname(f), exist_ok=True)
        req = urllib.request.Request(MCMETA % (ref, path), headers={"User-Agent": "firn-gen-dig"})
        with urllib.request.urlopen(req, timeout=60) as r:
            data = r.read()
        open(f, "wb").write(data)
    return json.load(open(f))


def tag_blocks(version, tag, memo):
    """set of block names ('minecraft:stone') of block tag `tag` ('minecraft:mineable/pickaxe'), tags resolved recursively"""
    if tag in memo:
        return memo[tag]
    memo[tag] = set()
    ns, _, name = tag.partition(":")
    doc = fetch_cached(version + "-data", "data/%s/tags/block/%s.json" % (ns, name))
    out = set()
    for v in doc["values"]:
        v = v["id"] if isinstance(v, dict) else v
        if v.startswith("#"):
            out |= tag_blocks(version, v[1:], memo)
        else:
            out.add(v)
    memo[tag] = out
    return out


def rule_blocks(version, spec, memo):
    specs = spec if isinstance(spec, list) else [spec]
    out = set()
    for sp in specs:
        out |= tag_blocks(version, sp[1:], memo) if sp.startswith("#") else {sp}
    return out


def build_tools(data_dir, version):
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    vt = json.load(open(os.path.join(HERE, "versions.json")))["versions"]
    key = vt[version]["data"] if version in vt else version
    P = paths[key]
    blocks = json.load(open(os.path.join(data_dir, P["blocks"], "blocks.json")))
    items = json.load(open(os.path.join(data_dir, P["items"], "items.json")))
    item_id = {x["name"]: x["id"] for x in items}
    for b in blocks:
        if "minStateId" not in b:
            b["minStateId"] = b["id"] * 16
    blocks = sorted(blocks, key=lambda b: b["minStateId"])
    comps = fetch_cached(version + "-summary", "item_components/data.min.json")
    memo = {}
    tools = []  # (item id, default speed, [(blockset, speed or None, correct or None)])
    for name, c in comps.items():
        t = c.get("minecraft:tool")
        if not t or name not in item_id:
            continue
        rules = [(rule_blocks(version, r["blocks"], memo), r.get("speed"), r.get("correct_for_drops")) for r in t.get("rules", [])]
        tools.append((item_id[name], float(t.get("default_mining_speed", 1.0)), rules))
    tools.sort()
    vectors = {}   # tuple of (item, speed_milli) -> material index
    mat_list = []
    brec_mat = []
    harv_lists = []
    for b in blocks:
        bn = "minecraft:" + b["name"]
        vec = []
        harv = []
        for it, dflt, rules in tools:
            sp = dflt
            for bs, rsp, _ in rules:
                if rsp is not None and bn in bs:
                    sp = float(rsp)
                    break
            ok = False
            for bs, _, corr in rules:
                if corr is not None and bn in bs:
                    ok = bool(corr)
                    break
            if sp != 1.0:
                vec.append((it, min(SPEED_MAX_MILLI, int(round(sp * 1000)))))
            if ok:
                harv.append(it)
        vec = tuple(vec)
        if vec not in vectors:
            vectors[vec] = len(mat_list)
            mat_list.append(vec)
        brec_mat.append(vectors[vec])
        harv_lists.append(sorted(harv) if b.get("harvestTools") else None)
    num = gr.num
    recs = []
    for vec in mat_list:
        r = bytearray(num(len(vec)))
        for it, sp in vec:
            r += num(it) + num(sp)
        recs.append(bytes(r))
    nb, nm = len(blocks), len(mat_list)
    matdir_off = 4 + 4 * 4
    mats_off = matdir_off + 4 * nm
    blocks_off = mats_off + sum(len(r) for r in recs)
    harv_off = blocks_off + nb * 8
    harv = bytearray()
    brec = bytearray()
    for i in range(nb):
        off = 0
        if harv_lists[i] is not None:
            off = harv_off + len(harv)
            harv += num(len(harv_lists[i])) + b"".join(num(x) for x in harv_lists[i])
        brec += num(brec_mat[i]) + num(off)
    out = bytearray(b"FDG1")
    for v in (nb, nm, matdir_off, blocks_off):
        out += num(v)
    pos = mats_off
    for r in recs:
        out += num(pos)
        pos += len(r)
    for r in recs:
        out += r
    assert len(out) == blocks_off
    out += brec + harv
    return bytes(out), nb, nm, key


if __name__ == "__main__":
    if len(sys.argv) != 5:
        raise SystemExit(__doc__)
    data_dir, version, pver, out_path = sys.argv[1:5]
    if int(pver) >= 766:
        blob, nb, nm, key = build_tools(data_dir, version)
    else:
        blob, nb, nm, key = build(data_dir, version)
    L = ["// SPDX-License-Identifier: MPL-2.0", "// GENERATED by tools/mcgen/gen_dig.py - do not edit", "//",
         "// lib/mc/v%s/mc_dig_v%s.fi -- block material, tool speeds and harvest tools of Minecraft Java %s (protocol %s):" % (pver, pver, version, pver),
         "// %d blocks, %d materials (%d octets). Source: minecraft-data data/%s%s. The reader is lib/mc/mc_dig.fi." % (nb, nm, len(blob), key, " + misode/mcmeta tool components and block tags" if int(pver) >= 766 else ""),
         "", "import mc.mc_dig", "", 'const DIG_BLOB: str = "%s"' % gr.lit_string(blob), "",
         "fn dig_make() -> mc_dig.DigData {", "    return mc_dig.dig_open(DIG_BLOB.p as u64)", "}"]
    open(out_path, "w").write("\n".join(L) + "\n")
    sys.stderr.write("gen_dig: %s: %d blocks %d materials blob %d octets\n" % (version, nb, nm, len(blob)))
