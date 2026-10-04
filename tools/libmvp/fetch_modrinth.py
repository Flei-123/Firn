#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/fetch_modrinth.py <dir> [count] -- downloads the project icons
# of the most downloaded / newest / recently updated mods from the Modrinth
# API (WebP, PNG and GIF, as the launcher sees them) into <dir>. Third-party
# pictures: they are test input only and are never committed. Exit 0 and an
# empty directory when the network is not there.
import json, os, sys, urllib.request
from concurrent.futures import ThreadPoolExecutor
dest = sys.argv[1]; count = int(sys.argv[2]) if len(sys.argv) > 2 else 300
os.makedirs(dest, exist_ok=True)
H = {"User-Agent": "fleitec-firn-tests/1.0 (justin@fleitec.com)"}
urls = []
try:
    for off in range(0, count, 100):
        for idx in ("downloads", "newest", "updated"):
            q = "https://api.modrinth.com/v2/search?limit=100&offset=%d&index=%s&facets=%%5B%%5B%%22project_type:mod%%22%%5D%%5D" % (off, idx)
            d = json.load(urllib.request.urlopen(urllib.request.Request(q, headers=H), timeout=20))
            urls += [h["icon_url"] for h in d["hits"] if h.get("icon_url")]
except Exception as e:
    print("  skip: Modrinth not reachable (%s)" % type(e).__name__)
    sys.exit(0)
urls = sorted(set(urls))[:count]
def get(u):
    fn = os.path.join(dest, u.split("/data/")[1].replace("/", "_"))
    if os.path.exists(fn): return 1
    try:
        open(fn, "wb").write(urllib.request.urlopen(urllib.request.Request(u, headers=H), timeout=20).read()); return 1
    except Exception:
        return 0
with ThreadPoolExecutor(8) as ex:
    n = sum(ex.map(get, urls))
print("  modrinth: %d icons in %s" % (n, dest))
