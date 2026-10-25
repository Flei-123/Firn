#!/usr/bin/env python3
import json
import re
import sys
import zipfile

FACES = ("up", "down", "north", "south", "east", "west")


def load_tables(path):
    src = open(path, encoding="utf-8").read()
    tiles = re.search(r'const BT_TILES: str = "(.*?)"\n', src, re.S).group(1).split("\\x00")
    blocks = re.search(r'const BT_BLOCKS: str = "(.*?)"\n', src, re.S).group(1).split("\\x00")
    return [t.split(",") for t in tiles], [b.split(" ") for b in blocks]


class Jar:
    def __init__(self, path):
        self.z = zipfile.ZipFile(path)
        self.names = set(self.z.namelist())
        self.cache = {}

    def json(self, name):
        if name not in self.cache:
            self.cache[name] = json.loads(self.z.read(name)) if name in self.names else None
        return self.cache[name]

    def has_block_tex(self, tex):
        return f"assets/minecraft/textures/block/{tex}.png" in self.names

    def model(self, ref):
        ref = ref.split(":")[-1]
        return self.json(f"assets/minecraft/models/{ref}.json")

    def resolve(self, ref):
        chain = []
        cur = ref
        while cur:
            m = self.model(cur)
            if m is None:
                break
            chain.append(m)
            cur = m.get("parent")
        textures = {}
        elements = None
        for m in reversed(chain):
            textures.update(m.get("textures", {}))
            if "elements" in m:
                elements = m["elements"]
        return textures, elements

    def face_textures(self, ref, rot_y=0, rot_x=0):
        textures, elements = self.resolve(ref)
        if not elements:
            return None
        out = {}
        for el in elements:
            for face, spec in el.get("faces", {}).items():
                t = spec.get("texture", "")
                seen = 0
                while t.startswith("#") and seen < 8:
                    t = textures.get(t[1:], "")
                    seen += 1
                if not t.startswith("#"):
                    out.setdefault(face, set()).add(t.split(":")[-1].replace("block/", "", 1))
        return out

    def is_unit_cube(self, ref):
        _, elements = self.resolve(ref)
        if not elements or len(elements) != 1:
            return False
        el = elements[0]
        return el.get("from") == [0, 0, 0] and el.get("to") == [16, 16, 16] and "rotation" not in el

    def first_model(self, block):
        bs = self.json(f"assets/minecraft/blockstates/{block}.json")
        if bs is None:
            return None
        if "variants" in bs:
            variants = bs["variants"]
            key = next((k for k in variants if "axis=y" in k), None)
            if key is None:
                key = next((k for k in variants if "facing=north" in k and "half=top" not in k), None)
            if key is None:
                key = next(iter(variants))
            v = variants[key]
        else:
            v = bs["multipart"][0]["apply"]
        if isinstance(v, list):
            v = v[0]
        return v["model"]


def pick(jar, cands):
    for c in cands:
        if jar.has_block_tex(c):
            return c
    return None


def main():
    jar = Jar(sys.argv[1])
    tiles, blocks = load_tables(sys.argv[2])
    bad = 0
    total = 0
    missing = 0
    for rec in blocks:
        if len(rec) < 4:
            continue
        name, side, top, bottom = rec[0], int(rec[1]), int(rec[2]), int(rec[3])
        model = jar.first_model(name)
        if model is None:
            continue
        faces = jar.face_textures(model)
        if not faces or not jar.is_unit_cube(model):
            continue
        want = {"side": pick(jar, tiles[side]), "top": pick(jar, tiles[top]), "bottom": pick(jar, tiles[bottom])}
        real_side = faces.get("north") or faces.get("east") or faces.get("south") or faces.get("west")
        real_top = faces.get("up")
        real_bottom = faces.get("down")
        total += 1
        problems = []
        for key, real in (("side", real_side), ("top", real_top), ("bottom", real_bottom)):
            if real is None:
                continue
            if want[key] is None:
                problems.append(f"{key}: no candidate exists (real {sorted(real)})")
            elif want[key] not in real:
                problems.append(f"{key}: got {want[key]} real {sorted(real)}")
        if any("no candidate" in x for x in problems):
            missing += 1
        if problems:
            bad += 1
            print(name, "|", "; ".join(problems))
    print(f"checked {total} blocks, {bad} differ from the model, {missing} without any candidate texture")
    return 1 if missing else 0


sys.exit(main())
