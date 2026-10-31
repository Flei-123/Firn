import json
import os
import re
import shutil
import sys

SRC = os.environ.get("WG_DATA", "/root/.w5/wg-data/1.20.1/data/minecraft")
DST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "mcgen", "worldgen", "1.20.1")


def bare(name):
    return name.split(":", 1)[-1]


def copy_json(kind, name, sub):
    src = os.path.join(SRC, sub, kind, name + ".json")
    dst = os.path.join(DST, "tags", kind, name + ".json") if sub == "tags" else os.path.join(DST, kind, name + ".json")
    if not os.path.exists(src):
        raise SystemExit("missing " + src)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copyfile(src, dst)
    return src


def configured_refs(node, out):
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "feature" and isinstance(v, str):
                out.add(bare(v))
            else:
                configured_refs(v, out)
    elif isinstance(node, list):
        for v in node:
            configured_refs(v, out)


def selector_refs(node, out):
    if isinstance(node, dict):
        cfg = node.get("config")
        kind = bare(node.get("type", ""))
        if kind == "random_selector" and isinstance(cfg, dict):
            if isinstance(cfg.get("default"), str):
                out.add(bare(cfg["default"]))
            for entry in cfg.get("features", []):
                if isinstance(entry.get("feature"), str):
                    out.add(bare(entry["feature"]))
        if kind == "simple_random_selector" and isinstance(cfg, dict):
            for entry in cfg.get("features", []):
                if isinstance(entry, str):
                    out.add(bare(entry))
        for v in node.values():
            selector_refs(v, out)
    elif isinstance(node, list):
        for v in node:
            selector_refs(v, out)


def pick_configured(name, seen):
    if name in seen:
        return
    seen.add(name)
    src = copy_json("configured_feature", name, "worldgen")
    refs = set()
    doc = json.load(open(src))
    configured_refs(doc, refs)
    placed_refs = set()
    selector_refs(doc, placed_refs)
    for ref in sorted(placed_refs):
        if os.path.exists(os.path.join(SRC, "worldgen", "placed_feature", ref + ".json")):
            pick_placed(ref, seen)
    for ref in sorted(refs):
        if os.path.exists(os.path.join(SRC, "worldgen", "configured_feature", ref + ".json")):
            pick_configured(ref, seen)


def pick_tag(name, seen):
    if name in seen:
        return
    seen.add(name)
    src = copy_json("blocks", name, "tags")
    for v in json.load(open(src)).get("values", []):
        v = v if isinstance(v, str) else v.get("id", "")
        if v.startswith("#"):
            pick_tag(bare(v[1:]), seen)


def pick_placed(name, seen_cfg):
    src = copy_json("placed_feature", name, "worldgen")
    doc = json.load(open(src))
    feat = doc["feature"]
    if isinstance(feat, str):
        pick_configured(bare(feat), seen_cfg)




def main():
    args = sys.argv[1:]
    tags = []
    placed = []
    mode = "placed"
    for a in args:
        if a == "--tags":
            mode = "tags"
        elif a == "--placed":
            mode = "placed"
        elif mode == "tags":
            tags.append(a)
        else:
            placed.append(a)
    seen_cfg = set()
    seen_tag = set()
    for p in placed:
        pick_placed(p, seen_cfg)
    for t in tags:
        pick_tag(t, seen_tag)
    print("placed", len(placed), "configured", len(seen_cfg), "tags", len(seen_tag))


main()
