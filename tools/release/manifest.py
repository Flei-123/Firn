# SPDX-License-Identifier: MPL-2.0
import datetime
import hashlib
import json
import os
import sys


def parse(v):
    return tuple(int(x) for x in v.split("."))


def check_newer(path, version):
    with open(path) as f:
        cur = json.load(f)["version"]
    if parse(version) <= parse(cur):
        sys.exit(f"refusing to publish {version}: channel already has {cur} (--force)")


def entry_line(plat, dest, file):
    data = open(os.path.join(dest, file), "rb").read()
    body = {"file": file, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data), "signature": ""}
    return f'    "{plat}": ' + json.dumps(body)


def write(dest, channel, version, min_version, commit, entries):
    lines = [entry_line(e.split("=", 1)[0], dest, e.split("=", 1)[1]) for e in entries]
    head = {
        "format": 1,
        "channel": channel,
        "version": version,
        "min_version": min_version,
        "date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "commit": commit,
    }
    text = json.dumps(head, indent=2)[:-2] + ',\n  "platforms": {\n' + ",\n".join(lines) + "\n  }\n}\n"
    json.loads(text)
    for name in (f"manifest-{version}.json", "manifest.json"):
        tmp = os.path.join(dest, name + ".tmp")
        with open(tmp, "w") as f:
            f.write(text)
        os.replace(tmp, os.path.join(dest, name))


if __name__ == "__main__":
    if sys.argv[1] == "check-newer":
        check_newer(sys.argv[2], sys.argv[3])
    elif sys.argv[1] == "write":
        write(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6], sys.argv[7:])
    else:
        sys.exit("usage: manifest.py check-newer|write ...")
