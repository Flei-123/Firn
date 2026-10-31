#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import json
import os
import sys


def link_all(src, dst):
    os.makedirs(dst, exist_ok=True)
    for entry in os.listdir(src):
        target = os.path.join(dst, entry)
        if not os.path.lexists(target):
            os.symlink(os.path.join(src, entry), target)


def main():
    base, extra, out = [os.path.abspath(a) for a in sys.argv[1:4]]
    os.makedirs(os.path.join(out, "pc"), exist_ok=True)
    for entry in os.listdir(base):
        if entry not in ("dataPaths.json", "pc"):
            os.symlink(os.path.join(base, entry), os.path.join(out, entry))
    link_all(os.path.join(base, "pc"), os.path.join(out, "pc"))
    paths = json.load(open(os.path.join(base, "dataPaths.json")))
    extra_paths = os.path.join(extra, "dataPaths.json")
    if os.path.exists(extra_paths):
        link_all(os.path.join(extra, "pc"), os.path.join(out, "pc"))
        for version, entry in json.load(open(extra_paths))["pc"].items():
            paths["pc"][version] = entry
    with open(os.path.join(out, "dataPaths.json"), "w") as fh:
        json.dump(paths, fh)


if __name__ == "__main__":
    main()
