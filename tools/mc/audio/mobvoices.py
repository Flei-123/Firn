#!/usr/bin/env python3
"""tools/mc/audio/mobvoices.py <snd_table.fi> <recipes.json> -- the ten common mobs (zombie skeleton creeper spider cow pig sheep chicken
villager enderman): every ambient / hurt / death / step event of the version's id table must have a synth sound, ambient / hurt / death an
OWN recipe (not an alias) with >= 2 variants, and the steps of zombie / skeleton / spider / cow / pig / sheep / chicken their own recipe
with pairwise different seeds (pig, sheep and chicken used to alias the cow step). Exit 1 on any violation."""
import json
import re
import sys

MOBS = "zombie skeleton creeper spider cow pig sheep chicken villager enderman".split()
OWN_STEP = "zombie skeleton spider cow pig sheep chicken".split()


def names_of(path):
    s = open(path).read()
    m = re.search(r'const SND_BLOB: str = "(.*)"\n', s, re.S)
    return {x for x in m.group(1).split("\\x00") if x}


def main():
    table = names_of(sys.argv[1])
    doc = json.load(open(sys.argv[2]))
    by_id = {s["id"]: s for s in doc["sounds"]}
    alias = {}
    for s in doc["sounds"]:
        for a in s.get("aliases", []):
            alias[a.replace("minecraft:", "")] = s
    bad, n = [], 0
    for mob in MOBS:
        for act in ("ambient", "hurt", "death", "step"):
            ev = "entity.%s.%s" % (mob, act)
            if ev not in table:
                continue
            n += 1
            own = by_id.get(ev)
            if own is None and ev not in alias:
                bad.append("%s: no synth sound" % ev)
            elif act != "step" and (own is None or own.get("variants", 1) < 2):
                bad.append("%s: needs an own recipe with >= 2 variants" % ev)
    seeds = {}
    for mob in OWN_STEP:
        ev = "entity.%s.step" % mob
        if ev in table:
            if ev not in by_id:
                bad.append("%s: step must be an own recipe (not an alias)" % ev)
            else:
                seeds.setdefault(by_id[ev]["seed"], []).append(ev)
    for sd, evs in seeds.items():
        if len(evs) > 1:
            bad.append("same seed %s: %s" % (sd, evs))
    for b in bad:
        print("  FAIL  mobvoices:", b)
    print("  mobvoices: %d mob events checked, %s" % (n, "ok" if not bad else "%d problems" % len(bad)))
    sys.exit(1 if bad else 0)


main()
