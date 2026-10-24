#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""extract_model_textures.py -- texture NAMES referenced by the block models of a client jar.

usage: extract_model_textures.py <client.jar> <out.txt>

Writes one texture file name (without folder and .png) per line, sorted. Only names are kept; nothing of the jar is copied.
The list feeds gen_blocktex.py (extra texture slots for model-only textures such as door_top or redstone_dust_dot).
"""
import json
import sys
import zipfile

MODEL_DIR = "assets/minecraft/models/block/"
PREFIXES = ("block/", "minecraft:block/")


def texture_names(jar_path):
    names = set()
    with zipfile.ZipFile(jar_path) as z:
        for entry in z.namelist():
            if not entry.startswith(MODEL_DIR) or not entry.endswith(".json"):
                continue
            try:
                model = json.loads(z.read(entry))
            except ValueError:
                continue
            for value in (model.get("textures") or {}).values():
                if not isinstance(value, str):
                    continue
                for prefix in PREFIXES:
                    if value.startswith(prefix):
                        names.add(value[len(prefix):])
    return sorted(n for n in names if n and "/" not in n)


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    names = texture_names(sys.argv[1])
    with open(sys.argv[2], "w") as fh:
        fh.write("\n".join(names) + "\n")
    print("extract_model_textures: %d names" % len(names))


if __name__ == "__main__":
    main()
