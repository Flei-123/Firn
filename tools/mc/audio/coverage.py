#!/usr/bin/env python3
"""tools/mc/audio/coverage.py <snd_table.fi> <recipes.json>... [--top N] [--check] -- which sound events of a version's id table
(lib/mc/vNNN/mc_snd_vNNN.fi, generated, names only) have NO synth sound (id or alias in the recipe sets), ranked by how often a player
hears them. The ranking is a documented HEURISTIC (tier by name pattern), not measured data.

  tier 1  heard constantly in normal play: block step/break/place/hit/fall of every sound type, player, item pickup, eating, doors, chests,
          fire, water, weather, UI, arrows, experience, explosions
  tier 2  common mobs (zombie, skeleton, creeper, spider, enderman, cow, pig, sheep, chicken, horse, villager, wolf, cat, bat, slime, ...)
  tier 3  everything else (rare blocks, rare mobs, music discs, goat horns, ...)
--check: exit 1 when a tier 1 event is missing (or more than --allow2 N tier 2 events, default 0) (used by tools/mc/audio/run.sh with the committed baseline of known gaps via --allow N).
"""
import json
import re
import sys

T1 = [r"^block\.[a-z_]+\.(step|break|place|hit|fall)$", r"^entity\.player\.", r"^entity\.generic\.", r"^entity\.item\.", r"^item\.(armor|shield|bucket|trident|bottle|flintandsteel|firecharge)",
      r"^block\.(chest|barrel|ender_chest|shulker_box|wooden_door|iron_door|wooden_trapdoor|iron_trapdoor|fence_gate|lever|stone_button|wooden_button|stone_pressure_plate|wooden_pressure_plate|fire|water|lava|furnace|crafting|anvil|enchantment_table|bell|note_block|piston|tripwire|dispenser|dropper|portal|glass|sand|gravel|grass|stone|wood|wool|snow|ladder|scaffolding|bubble_column|beacon|brewing|bed|campfire|composter|fence)",
      r"^entity\.(arrow|experience_orb|lightning_bolt|tnt|firework_rocket|fishing_bobber|splash_potion|snowball|egg|ender_pearl|item_frame|painting|leash_knot|boat|minecart|dragon_fireball|player)\.",
      r"^(ui|weather|ambient|enchant|music)\.", r"^block\.(ambient|dispenser)"]
T2 = [r"^entity\.(zombie|skeleton|creeper|spider|enderman|cow|pig|sheep|chicken|horse|donkey|villager|wolf|cat|bat|slime|witch|squid|fish|cod|salmon|bee|fox|rabbit|llama|parrot|iron_golem|snow_golem|husk|drowned|phantom|pillager|zombie_villager|cave_spider|silverfish|blaze|ghast|magma_cube|wither_skeleton|piglin|hoglin|strider|turtle|dolphin|panda|polar_bear|ocelot|mule|guardian|ender_dragon|wither)\.(ambient|hurt|death|step|say|idle|hit|attack|primed)"]


def names_of(path):
    s = open(path).read()
    m = re.search(r'const SND_BLOB: str = "(.*)"\n', s, re.S)
    return [x for x in m.group(1).split("\\x00") if x]


def have_of(paths):
    have = set()
    for p in paths:
        d = json.load(open(p))
        for s in d["sounds"]:
            have.add(s["id"])
            for a in s.get("aliases", []):
                have.add(a.replace("minecraft:", ""))
    return have


MATS13 = ["stone", "deepslate", "wood", "gravel", "grass", "sand", "snow", "glass", "wool", "metal", "netherrack", "slime_block", "bone_block"]
FALLBACK = re.compile(r"^block\.[a-z0-9_]+\.(step|break|place|hit|fall)$")


def via_fallback(n, have):
    """mc_audio.mca_find: a block sound of a type the bank has no recipe for ("block.cherry_wood.step") plays the material event of
    mca_type_material (always one of the 13 types, default stone), so it IS covered when all 13 material events of that action exist."""
    m = FALLBACK.match(n)
    return bool(m) and all("block.%s.%s" % (t, m.group(1)) in have for t in MATS13)


def tier(n):
    for r in T1:
        if re.search(r, n):
            return 1
    for r in T2:
        if re.search(r, n):
            return 2
    return 3


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    top = 40
    allow = None
    if "--top" in sys.argv:
        top = int(sys.argv[sys.argv.index("--top") + 1])
        args = [a for a in args if a != str(top)]
    allow2 = 0
    if "--allow2" in sys.argv:
        allow2 = int(sys.argv[sys.argv.index("--allow2") + 1])
        args = [a for a in args if a != str(allow2)]
    if "--allow" in sys.argv:
        allow = int(sys.argv[sys.argv.index("--allow") + 1])
        args = [a for a in args if a != str(allow)]
    names = names_of(args[0])
    have = have_of(args[1:])
    miss = {1: [], 2: [], 3: []}
    cnt = {1: 0, 2: 0, 3: 0}
    nfb = {1: 0, 2: 0, 3: 0}
    for n in names:
        t = tier(n)
        cnt[t] += 1
        if n not in have:
            if via_fallback(n, have):
                nfb[t] += 1
            else:
                miss[t].append(n)
    for t in (1, 2, 3):
        print("tier %d: %d events, %d with a synth sound (%d of them via the block-type fallback), %d missing"
              % (t, cnt[t], cnt[t] - len(miss[t]), nfb[t], len(miss[t])))
    for t in (1, 2):
        print("missing tier %d (first %d):" % (t, top))
        for n in miss[t][:top]:
            print("  " + n)
    extra = sorted(h for h in have if h not in set(names) and not h.startswith(("music.ambient", "ambient.wind", "ambient.cave", "ambient.drip")))
    if extra:
        print("synth events NOT in the id table (aliases / own names): %d e.g. %s" % (len(extra), ", ".join(extra[:6])))
    if "--check" in sys.argv:
        lim = allow if allow is not None else 0
        if len(miss[1]) > lim:
            print("FAIL tier 1 gaps %d > allowed %d" % (len(miss[1]), lim))
            sys.exit(1)
        if len(miss[2]) > allow2:
            print("FAIL tier 2 gaps %d > allowed %d" % (len(miss[2]), allow2))
            sys.exit(1)
    print("coverage ok")


main()
