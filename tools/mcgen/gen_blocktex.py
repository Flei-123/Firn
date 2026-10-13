#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""gen_blocktex.py -- minecraft-data block names -> resource-pack texture NAME candidates (lib/mc/mc_blocktex.fi).

usage: gen_blocktex.py <minecraft-data-data-dir> <libdir>

Emits ONE version independent module (union of the block names of every flattened version in versions.json):
  * BT_TILES: one record per distinct texture slot = comma separated candidate FILE NAMES (without .png) under
    assets/minecraft/textures/block/. The first candidate a pack provides wins.
  * BT_BLOCKS: sorted records `name side top bottom` (indices into BT_TILES); binary searched at run time.
  * pack_format_for_protocol(): resource pack_format per protocol, from versions.json 'resource_pack_format'.
Only NAMES are emitted (the file names of the public pack format; nothing of any game file). The candidate rules
are heuristics of the public naming convention (<name>_top / _side / _bottom, material of stairs/slabs/walls ...);
a wrong guess only means the default tile is shown, fix it in the rules below, never in the generated file.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUFFIXES = ["_stairs", "_slab", "_fence_gate", "_fence", "_wall", "_button", "_pressure_plate", "_trapdoor"]

# explicit (side, top, bottom) candidate lists
SPECIAL = {
    "grass_block": (["grass_block_side"], ["grass_block_top"], ["dirt"]),
    "podzol": (["podzol_side"], ["podzol_top"], ["dirt"]),
    "mycelium": (["mycelium_side"], ["mycelium_top"], ["dirt"]),
    "dirt_path": (["dirt_path_side"], ["dirt_path_top"], ["dirt"]),
    "farmland": (["dirt"], ["farmland"], ["dirt"]),
    "crafting_table": (["crafting_table_side"], ["crafting_table_top"], ["oak_planks"]),
    "furnace": (["furnace_side"], ["furnace_top"], ["furnace_top"]),
    "blast_furnace": (["blast_furnace_side"], ["blast_furnace_top"], ["blast_furnace_top"]),
    "smoker": (["smoker_side"], ["smoker_top"], ["smoker_bottom", "smoker_top"]),
    "dispenser": (["furnace_side"], ["furnace_top"], ["furnace_top"]),
    "dropper": (["furnace_side"], ["furnace_top"], ["furnace_top"]),
    "bookshelf": (["bookshelf"], ["oak_planks"], ["oak_planks"]),
    "snow": (["snow"], ["snow"], ["snow"]),
    "snow_block": (["snow"], ["snow"], ["snow"]),
    "water": (["water_still"], ["water_still"], ["water_still"]),
    "bubble_column": (["water_still"], ["water_still"], ["water_still"]),
    "lava": (["lava_still"], ["lava_still"], ["lava_still"]),
    "magma_block": (["magma"], ["magma"], ["magma"]),
    "smooth_stone": (["smooth_stone"], ["smooth_stone"], ["smooth_stone"]),
    "petrified_oak": (["oak_planks"], ["oak_planks"], ["oak_planks"]),
    "quartz": (["quartz_block_side"], ["quartz_block_top"], ["quartz_block_bottom"]),
    "quartz_block": (["quartz_block_side"], ["quartz_block_top"], ["quartz_block_bottom"]),
    "smooth_quartz": (["quartz_block_bottom"], ["quartz_block_bottom"], ["quartz_block_bottom"]),
    "purpur": (["purpur_block"], ["purpur_block"], ["purpur_block"]),
    "copper": (["copper_block"], ["copper_block"], ["copper_block"]),
    "glass_pane": (["glass"], ["glass"], ["glass"]),
}
for _sand in ("sandstone", "red_sandstone"):
    SPECIAL[_sand] = ([_sand], [_sand + "_top"], [_sand + "_bottom"])
    SPECIAL["cut_" + _sand] = (["cut_" + _sand], [_sand + "_top"], [_sand + "_top"])
    SPECIAL["chiseled_" + _sand] = (["chiseled_" + _sand], [_sand + "_top"], [_sand + "_top"])
    SPECIAL["smooth_" + _sand] = ([_sand + "_top"], [_sand + "_top"], [_sand + "_top"])


# blocks of the public pack convention that have separate top / side / bottom files (everything else = one texture)
MULTI = set("""tnt pumpkin carved_pumpkin jack_o_lantern melon hay_block bone_block basalt polished_basalt barrel target cactus
honey_block beehive bee_nest jukebox note_block cartography_table fletching_table smithing_table loom composter respawn_anchor
lodestone dried_kelp_block red_mushroom_block brown_mushroom_block mushroom_stem command_block chain_command_block
repeating_command_block piston sticky_piston observer crafting_table chiseled_bookshelf stonecutter grindstone ancient_debris
crying_obsidian sculk_catalyst crimson_nylium warped_nylium muddy_mangrove_roots reinforced_deepslate sniffer_egg
chiseled_quartz_block chiseled_stone_bricks purpur_pillar coal_block""".split())


def material(n):
    """name of the material block a stairs/slab/wall/... is made of (or None)."""
    for s in SUFFIXES:
        if n.endswith(s) and len(n) > len(s):
            return n[: -len(s)]
    return None


def uniq(seq):
    out = []
    for x in seq:
        if x not in out:
            out.append(x)
    return out


def resolve(n, depth=0):
    """-> (side, top, bottom) candidate lists for the block called n."""
    if n.startswith("waxed_"):
        n = n[6:]
    if n.startswith("potted_"):
        n = n[7:]
    if n.startswith("wall_") and not n.endswith("_wall"):
        n = n[5:]
    if n in SPECIAL:
        return SPECIAL[n]
    if n.endswith(("_log", "_stem", "_pillar")) or n in ("bamboo_block", "stripped_bamboo_block"):
        return ([n], [n + "_top"], [n + "_top"])
    if n.endswith("_wood"):
        b = n[:-5] + "_log"
        return ([b], [b], [b])
    if n.endswith("_hyphae"):
        b = n[:-7] + "_stem"
        return ([b], [b], [b])
    if n.endswith("_pane") and len(n) > 5:
        b = n[:-5]
        return ([b], [b], [b])
    m = material(n)
    if m is not None and depth == 0:
        s, t, b = resolve(m, 1)
        extra = [m, m + "s", m + "_planks", m + "_block"]
        return (uniq(s + extra), uniq(t + extra), uniq(b + extra))
    if n in MULTI:
        return (uniq([n + "_side", n]), uniq([n + "_top", n]), uniq([n + "_bottom", n + "_top", n]))
    one = uniq([n, n + "_side"])  # one texture for all faces: one atlas tile instead of three
    return (one, one, one)


def load_versions(data_dir):
    vt = json.load(open(os.path.join(HERE, "versions.json")))["versions"]
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    return vt, paths


def lit_string(blob):
    parts = []
    for c in blob:
        b = ord(c)
        assert b < 0x80
        if c == '"':
            parts.append('\\"')
        elif c == "\\":
            parts.append("\\\\")
        elif 0x20 <= b < 0x7F and c != "{":
            parts.append(c)
        else:
            parts.append("\\x%02x" % b)
    return "".join(parts)


def generate(data_dir, libdir):
    vt, paths = load_versions(data_dir)
    names = {}
    used = []
    for v, meta in vt.items():
        if "FLAG_FLATTENING" not in meta.get("flags", []):
            continue  # pre-flattening names do not match the modern pack file names
        used.append(v)
        blocks = json.load(open(os.path.join(data_dir, paths[meta.get("data", v)]["blocks"], "blocks.json")))
        for b in blocks:
            names[b["name"]] = True
    tiles = []
    tile_ix = {}

    def tile_of(cands):
        key = ",".join(cands)
        if key not in tile_ix:
            tile_ix[key] = len(tiles)
            tiles.append(key)
        return tile_ix[key]

    recs = []
    for n in sorted(names):
        s, t, b = resolve(n)
        recs.append("%s %d %d %d" % (n, tile_of(s), tile_of(t), tile_of(b)))
    for r in recs:
        assert all(0x20 <= ord(c) < 0x7F for c in r)
    pfd = {meta["protocol"]: meta["resource_pack_format"] for meta in vt.values() if "resource_pack_format" in meta}
    for k, v in json.load(open(os.path.join(HERE, "versions.json"))).get("resource_pack_format_extra", {}).items():
        if k.isdigit():
            assert int(k) not in pfd, "protocol %s is a full version, remove it from resource_pack_format_extra" % k
            pfd[int(k)] = v
    pf = sorted(pfd.items())
    out = ["// SPDX-License-Identifier: MPL-2.0",
           "// GENERATED by tools/mcgen/gen_blocktex.py - do not edit",
           "//",
           "// lib/mc/mc_blocktex.fi -- block name -> resource pack texture NAME candidates (%d blocks, %d texture slots, versions: %s)." % (len(recs), len(tiles), ", ".join(used)),
           "// Names only. Source: minecraft-data block names + the public pack naming convention (rules in gen_blocktex.py).",
           "// pack_format table: tools/mcgen/versions.json 'resource_pack_format' (source in docs/mc/resourcepacks.md).",
           "",
           "export { BT_TILE_COUNT, BT_TILES, BT_BLOCK_COUNT, BT_BLOCKS, pack_format_for_protocol }",
           "",
           "const BT_TILE_COUNT: usize = %d" % len(tiles),
           "const BT_BLOCK_COUNT: usize = %d" % len(recs),
           "const BT_TILES: str = \"%s\"" % lit_string("\0".join(tiles) + "\0"),
           "const BT_BLOCKS: str = \"%s\"" % lit_string("\0".join(recs) + "\0"),
           "",
           "// Expected resource pack_format of a game protocol; -1 = not in the table (unverified / unknown).",
           "fn pack_format_for_protocol(protocol: i32) -> i32 {"]
    for proto, fmt in pf:
        out.append("    if protocol == %d { return %d }" % (proto, fmt))
    out += ["    return -1", "}", ""]
    with open(os.path.join(libdir, "mc_blocktex.fi"), "w") as fh:
        fh.write("\n".join(out))
    print("gen_blocktex: %d blocks, %d texture slots, %d pack_format rows" % (len(recs), len(tiles), len(pf)))


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    generate(sys.argv[1], sys.argv[2])


if __name__ == "__main__":
    main()
