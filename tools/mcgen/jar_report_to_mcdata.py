#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import argparse
import json
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
STRIP = "minecraft:"
CATEGORY_TYPES = {
    "monster": "hostile",
    "creature": "animal",
    "ambient": "ambient",
    "axolotls": "water_creature",
    "underground_water_creature": "water_creature",
    "water_creature": "water_creature",
    "water_ambient": "water_creature",
    "misc": "other",
}


def load_json(path):
    with open(path) as fh:
        return json.load(fh)


def short(name):
    return name[len(STRIP):] if name.startswith(STRIP) else name


def property_spec(name, values):
    if values == ["true", "false"]:
        return {"name": name, "type": "bool", "num_values": 2}
    if all(v.isdigit() for v in values):
        return {"name": name, "type": "int", "num_values": len(values), "values": list(values)}
    return {"name": name, "type": "enum", "num_values": len(values), "values": list(values)}


def ordered_properties(info):
    props = info.get("properties", {})
    if len(props) < 2:
        return list(props.items())
    first = min(info["states"], key=lambda st: st["id"])
    by_key = {tuple(sorted(st["properties"].items())): st["id"] for st in info["states"]}
    strides = {}
    for name, values in props.items():
        if len(values) < 2:
            strides[name] = 0
            continue
        changed = dict(first["properties"])
        changed[name] = values[1]
        key = tuple(sorted(changed.items()))
        strides[name] = by_key[key] - first["id"]
    return sorted(props.items(), key=lambda kv: -strides[kv[0]])


def convert_blocks(reports, registries, dump, base, item_stack):
    entries = registries["minecraft:block"]["entries"]
    base_by_name = {b["name"]: b for b in base}
    dump_by_name = {b["name"]: b for b in dump["blocks"]}
    out = []
    for full, info in reports["blocks"].items():
        name = short(full)
        states = info["states"]
        ids = [s["id"] for s in states]
        default = [s["id"] for s in states if s.get("default")]
        rec = {
            "id": entries[full]["protocol_id"],
            "name": name,
            "minStateId": min(ids),
            "maxStateId": max(ids),
            "defaultState": default[0] if default else min(ids),
            "states": [property_spec(k, v) for k, v in ordered_properties(info)],
        }
        old = base_by_name.get(name)
        measured = dump_by_name.get(name)
        if old is not None:
            for key in ("hardness", "resistance", "stackSize", "diggable", "transparent",
                        "emitLight", "filterLight", "boundingBox", "material", "displayName"):
                if key in old:
                    rec[key] = old[key]
            rec["source"] = "base"
        if measured is not None and (old is None or "--remeasure" in sys.argv):
            hardness = measured["hardness"]
            rec["hardness"] = hardness
            rec["diggable"] = hardness >= 0 and not measured["air"]
            rec["emitLight"] = measured["emitLight"]
            rec["filterLight"] = measured["filterLight"]
            rec["transparent"] = not measured["canOcclude"]
            rec["boundingBox"] = "block" if measured["collides"] else "empty"
            rec["stackSize"] = item_stack.get(name, 64)
            rec["source"] = "jar"
        out.append(rec)
    out.sort(key=lambda b: b["id"])
    return out


def convert_items(report_dir, registries):
    entries = registries["minecraft:item"]["entries"]
    out = []
    for full, info in entries.items():
        name = short(full)
        comp_path = os.path.join(report_dir, "minecraft", "components", "item", name + ".json")
        stack = 64
        if os.path.exists(comp_path):
            comps = load_json(comp_path).get("components", {})
            stack = comps.get("minecraft:max_stack_size", 64)
        out.append({"id": info["protocol_id"], "name": name, "stackSize": stack})
    out.sort(key=lambda i: i["id"])
    return out


def convert_entities(registries, dump, base):
    entries = registries["minecraft:entity_type"]["entries"]
    base_by_name = {e["name"]: e for e in base}
    dump_by_name = {e["name"]: e for e in dump["entities"]}
    out = []
    for full, info in entries.items():
        name = short(full)
        old = base_by_name.get(name)
        measured = dump_by_name.get(name)
        rec = {"id": info["protocol_id"], "internalId": info["protocol_id"], "name": name}
        if old is not None:
            rec.update({k: old[k] for k in ("displayName", "width", "height", "type", "category", "metadataKeys") if k in old})
            rec["source"] = "base"
        elif measured is not None:
            rec["width"] = round(measured["width"], 6)
            rec["height"] = round(measured["height"], 6)
            rec["type"] = CATEGORY_TYPES.get(measured["category"], "other")
            rec["source"] = "jar"
        out.append(rec)
    out.sort(key=lambda e: e["id"])
    return out


def convert_sounds(registries):
    entries = registries["minecraft:sound_event"]["entries"]
    out = [{"id": info["protocol_id"] + 1, "name": short(full)} for full, info in entries.items()]
    out.sort(key=lambda e: e["id"])
    return out


def write_json(path, value, indent=None):
    with open(path, "w") as fh:
        json.dump(value, fh, separators=None if indent else (",", ":"), indent=indent)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reports", required=True)
    ap.add_argument("--dump", required=True)
    ap.add_argument("--base-data", required=True)
    ap.add_argument("--base-reports", required=True)
    ap.add_argument("--base-version", default="26.1")
    ap.add_argument("--version", required=True)
    ap.add_argument("--protocol", type=int, required=True)
    ap.add_argument("--delta")
    ap.add_argument("--out", required=True)
    ap.add_argument("--remeasure", action="store_true")
    args = ap.parse_args()
    import jar_protocol
    reports = {"blocks": load_json(os.path.join(args.reports, "blocks.json"))}
    registries = load_json(os.path.join(args.reports, "registries.json"))
    dump = load_json(args.dump)
    data_paths = load_json(os.path.join(args.base_data, "dataPaths.json"))
    paths = data_paths["pc"][args.base_version]
    base_blocks = load_json(os.path.join(args.base_data, paths["blocks"], "blocks.json"))
    base_entities = load_json(os.path.join(args.base_data, paths["entities"], "entities.json"))
    base_proto = load_json(os.path.join(args.base_data, paths["protocol"], "protocol.json"))
    base_packets = load_json(os.path.join(args.base_reports, "packets.json"))
    new_packets = load_json(os.path.join(args.reports, "packets.json"))
    delta = load_json(args.delta) if args.delta else {}
    version_dir = os.path.join(args.out, "pc", args.version)
    os.makedirs(version_dir, exist_ok=True)
    items = convert_items(args.reports, registries)
    stacks = {i["name"]: i["stackSize"] for i in items}
    write_json(os.path.join(version_dir, "blocks.json"), convert_blocks(reports, registries, dump, base_blocks, stacks))
    write_json(os.path.join(version_dir, "items.json"), items)
    write_json(os.path.join(version_dir, "entities.json"), convert_entities(registries, dump, base_entities))
    write_json(os.path.join(version_dir, "sounds.json"), convert_sounds(registries))
    proto, report = jar_protocol.apply_delta(base_proto, base_packets, new_packets, delta)
    write_json(os.path.join(version_dir, "protocol.json"), proto)
    write_json(os.path.join(version_dir, "version.json"), {"version": args.protocol, "minecraftVersion": args.version,
                                                          "majorVersion": args.version, "releaseType": "release"}, 2)
    write_json(os.path.join(version_dir, "report.json"), report, 2)
    entry = dict(paths)
    for what in ("blocks", "items", "entities", "sounds", "protocol", "version"):
        entry[what] = "pc/" + args.version
    write_json(os.path.join(args.out, "dataPaths.json"), {"pc": {args.version: entry}}, 2)
    for key in ("added", "removed", "unnamed"):
        print(key, report[key], file=sys.stderr)


if __name__ == "__main__":
    sys.path.insert(0, HERE)
    main()
