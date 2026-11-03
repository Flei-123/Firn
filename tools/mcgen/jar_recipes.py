#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""jar_recipes.py <jar_data_root> <items.json> <out recipes.json> -- crafting recipes of a server jar in minecraft-data recipes.json form.

<jar_data_root> holds data/minecraft/recipe/*.json and data/minecraft/tags/item/**/*.json extracted from the jar. Item tags are expanded to
one recipe per member combination (the table has no ingredient sets); recipes above MAX_COMBOS combinations are dropped and counted.
Output: {"<result item id>": [{"result": {"id", "count"}, "inShape": [[id|null]], "ingredients": [id]}]} with the ids of items.json.
"""
import itertools
import json
import os
import sys

MAX_COMBOS = 512
SHAPED = "minecraft:crafting_shaped"
SHAPELESS = "minecraft:crafting_shapeless"


def short(name):
    return name.split(":", 1)[1] if ":" in name else name


def load_tag(root, tag, memo):
    if tag in memo:
        return memo[tag]
    memo[tag] = []
    path = os.path.join(root, "data", "minecraft", "tags", "item", short(tag) + ".json")
    out = []
    if os.path.exists(path):
        for v in json.load(open(path))["values"]:
            v = v["id"] if isinstance(v, dict) else v
            if v.startswith("#"):
                out.extend(load_tag(root, v[1:], memo))
            else:
                out.append(v)
    memo[tag] = sorted(set(out))
    return memo[tag]


def expand(root, spec, memo):
    specs = spec if isinstance(spec, list) else [spec]
    out = []
    for s in specs:
        if isinstance(s, dict):
            s = s.get("item") or s.get("tag")
        if s.startswith("#"):
            out.extend(load_tag(root, s[1:], memo))
        else:
            out.append(s)
    return sorted(set(out))


def convert(root, ids, memo):
    rdir = os.path.join(root, "data", "minecraft", "recipe")
    out = {}
    stats = {"shaped": 0, "shapeless": 0, "dropped_combos": 0, "unknown_item": 0, "other": 0}
    for fn in sorted(f for f in os.listdir(rdir) if f.endswith(".json")):
        r = json.load(open(os.path.join(rdir, fn)))
        t = r["type"]
        if t not in (SHAPED, SHAPELESS):
            stats["other"] += 1
            continue
        res = r["result"]
        rid = ids.get(res["id"])
        if rid is None:
            stats["unknown_item"] += 1
            continue
        count = res.get("count", 1)
        if t == SHAPED:
            keys = sorted(r["key"])
            options = [[ids.get(n) for n in expand(root, r["key"][k], memo)] for k in keys]
            slots = keys
        else:
            slots = list(range(len(r["ingredients"])))
            options = [[ids.get(n) for n in expand(root, ing, memo)] for ing in r["ingredients"]]
        if any(len(o) == 0 or None in o for o in options):
            stats["unknown_item"] += 1
            continue
        combos = 1
        for o in options:
            combos *= len(o)
        if combos > MAX_COMBOS:
            stats["dropped_combos"] += 1
            continue
        for pick in itertools.product(*options):
            entry = {"result": {"id": rid, "count": count}}
            if t == SHAPED:
                m = dict(zip(slots, pick))
                entry["inShape"] = [[m.get(c) if c != " " else None for c in row] for row in r["pattern"]]
                stats["shaped"] += 1
            else:
                entry["ingredients"] = list(pick)
                stats["shapeless"] += 1
            out.setdefault(str(rid), []).append(entry)
    return out, stats


def main():
    root, items_path, out_path = sys.argv[1:4]
    ids = {"minecraft:" + i["name"]: i["id"] for i in json.load(open(items_path))}
    out, stats = convert(root, ids, {})
    with open(out_path, "w") as fh:
        json.dump(out, fh, separators=(",", ":"))
    sys.stderr.write("jar_recipes: %s\n" % json.dumps(stats))


if __name__ == "__main__":
    main()
