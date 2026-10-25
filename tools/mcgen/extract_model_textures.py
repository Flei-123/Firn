#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""extract_model_textures.py -- texture NAMES referenced by the block models of one or more client jars.

usage: extract_model_textures.py <client.jar> [<client.jar> ...] <out.txt> [--report]

Writes one texture file name (without folder and .png) per line, sorted: the union over all given jars.
Only names are kept; nothing of the jars is copied. The list feeds gen_blocktex.py (extra texture slots for
model-only textures such as door_top or redstone_dust_dot).
A jar counts as "unmapped" input when a model names a block texture that the same jar does not ship as a png,
or a texture in a sub folder (those cannot become a flat slot). --report prints per jar: names, names new compared
to the previous jars, unmapped names; the exit code is 1 when any unmapped name exists.
"""
import json
import sys
import zipfile

MODEL_DIR = "assets/minecraft/models/block/"
TEXTURE_DIR = "assets/minecraft/textures/block/"
PREFIXES = ("block/", "minecraft:block/")


def scan_jar(jar_path):
    flat = set()
    nested = set()
    with zipfile.ZipFile(jar_path) as z:
        entries = z.namelist()
        shipped = {e[len(TEXTURE_DIR):-4] for e in entries if e.startswith(TEXTURE_DIR) and e.endswith(".png")}
        for entry in entries:
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
                        name = value[len(prefix):]
                        (nested if "/" in name else flat).add(name)
                        break
    flat.discard("")
    missing = {n for n in flat if n not in shipped}
    return flat, nested, missing


def parse_args(argv):
    report = "--report" in argv
    args = [a for a in argv[1:] if a != "--report"]
    if len(args) < 2:
        raise SystemExit(__doc__)
    return args[:-1], args[-1], report


def main():
    jars, out_path, report = parse_args(sys.argv)
    union = set()
    unmapped_total = 0
    for jar in jars:
        flat, nested, missing = scan_jar(jar)
        unmapped = nested | missing
        unmapped_total += len(unmapped)
        if report:
            print("%s: %d names, %d new, %d unmapped" % (jar, len(flat), len(flat - union), len(unmapped)))
            for name in sorted(unmapped):
                print("  unmapped: %s" % name)
        union |= flat
    names = sorted(union)
    with open(out_path, "w") as fh:
        fh.write("\n".join(names) + "\n")
    print("extract_model_textures: %d names from %d jar(s)" % (len(names), len(jars)))
    if report and unmapped_total:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
