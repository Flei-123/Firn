import sys, json, zipfile

DIRS = ["down", "up", "north", "south", "west", "east"]
NORMAL = {"down": (0, -1, 0), "up": (0, 1, 0), "north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0)}
FACE_INFO = {
    "down": [("x1", "y1", "z2"), ("x1", "y1", "z1"), ("x2", "y1", "z1"), ("x2", "y1", "z2")],
    "up": [("x1", "y2", "z1"), ("x1", "y2", "z2"), ("x2", "y2", "z2"), ("x2", "y2", "z1")],
    "north": [("x2", "y2", "z1"), ("x2", "y1", "z1"), ("x1", "y1", "z1"), ("x1", "y2", "z1")],
    "south": [("x1", "y2", "z2"), ("x1", "y1", "z2"), ("x2", "y1", "z2"), ("x2", "y2", "z2")],
    "west": [("x1", "y2", "z1"), ("x1", "y1", "z1"), ("x1", "y1", "z2"), ("x1", "y2", "z2")],
    "east": [("x2", "y2", "z2"), ("x2", "y1", "z2"), ("x2", "y1", "z1"), ("x2", "y2", "z1")],
}


def rot_x(v, k):
    x, y, z = v
    for _ in range(k):
        y, z = z, -y
    return (x, y, z)


def rot_y(v, k):
    x, y, z = v
    for _ in range(k):
        x, z = -z, x
    return (x, y, z)


def turn(v, kx, ky):
    return rot_y(rot_x(v, kx), ky)


def facing_of(vec):
    for n, d in NORMAL.items():
        if d == vec:
            return n
    raise SystemExit(f"bad normal {vec}")


def load_model(z, name, seen=0):
    name = name.replace("minecraft:", "")
    return json.loads(z.read(f"assets/minecraft/models/{name}.json"))


def chain(z, name):
    out = []
    while name:
        m = load_model(z, name)
        out.append(m)
        name = m.get("parent")
    return out


def resolve_tex(ch, ref):
    tex = {}
    for m in reversed(ch):
        tex.update(m.get("textures", {}))
    while ref.startswith("#"):
        ref = tex[ref[1:]]
    return ref.replace("minecraft:", "")


def expected(z, model, kx, ky):
    ch = chain(z, model)
    elements = next(m["elements"] for m in ch if "elements" in m)
    quads = []
    for e in elements:
        b = {"x1": e["from"][0], "y1": e["from"][1], "z1": e["from"][2], "x2": e["to"][0], "y2": e["to"][1], "z2": e["to"][2]}
        for d, f in e["faces"].items():
            uv = f.get("uv")
            if uv is None:
                raise SystemExit("default uv not supported in reference")
            rot = f.get("rotation", 0) // 90
            verts = []
            for i, key in enumerate(FACE_INFO[d]):
                s = (i + rot) % 4
                u = uv[0] if s in (0, 1) else uv[2]
                v = uv[1] if s in (0, 3) else uv[3]
                p = tuple(b[k] - 8 for k in key)
                p = turn(p, kx, ky)
                verts.append((p[0] + 8, p[1] + 8, p[2] + 8, u, v))
            cull = f.get("cullface")
            cull = facing_of(turn(NORMAL[cull], kx, ky)) if cull else None
            quads.append({"dir": facing_of(turn(NORMAL[d], kx, ky)), "cull": cull, "tex": resolve_tex(ch, f["texture"]), "verts": verts})
    return quads


def parse_dump(path):
    states = {}
    cur = None
    for line in open(path):
        t = line.split()
        if t[0] == "STATE":
            cur = {"quads": [], "tex": {}}
            states[(t[1], t[2])] = cur
        elif t[0] == "Q":
            kv = dict(x.split("=") for x in t[1:])
            cur["quads"].append({"dir": int(kv["dir"]) // 1000, "cull": int(kv["cull"]) // 1000, "tex": int(kv["tex"]), "verts": []})
        elif t[0] == "V":
            n = [int(x) / 1000 * (16 if i < 3 else 1) for i, x in enumerate(t[1:6])]
            cur["quads"][-1]["verts"].append(tuple(n))
        elif t[0] == "T":
            cur["tex"][int(t[1])] = "".join(chr(int(c)) for c in t[2:])
    return states


def same_cycle(a, b):
    return any(a == b[i:] + b[:i] for i in range(4))


def main():
    jar, dump = sys.argv[1], sys.argv[2]
    z = zipfile.ZipFile(jar)
    states = parse_dump(dump)
    bad = 0
    for (block, props), st in sorted(states.items()):
        p = dict(x.split("=") for x in props.split(","))
        bs = json.loads(z.read(f"assets/minecraft/blockstates/{block}.json"))
        var = bs["variants"][props] if props in bs["variants"] else bs["variants"][",".join(f"{k}={p[k]}" for k in sorted(p))]
        exp = expected(z, var["model"], (var.get("x", 0) // 90) % 4, (var.get("y", 0) // 90) % 4)
        got = st["quads"]
        if len(exp) != len(got):
            print(f"FAIL {block} {props}: quads {len(got)} != {len(exp)}")
            bad += 1
            continue
        pool = list(got)
        for q in exp:
            hit = None
            for g in pool:
                if st["tex"][g["tex"]].replace("minecraft:", "") != q["tex"]:
                    continue
                if DIRS[g["dir"]] != q["dir"]:
                    continue
                if sorted(g["verts"]) == sorted(q["verts"]):
                    hit = g
                    break
            if hit is None:
                print(f"FAIL {block} {props}: no match for {q['dir']} {q['tex']} {q['verts']}")
                bad += 1
                continue
            pool.remove(hit)
            gc = None if hit["cull"] < 0 else DIRS[hit["cull"]]
            if gc != q["cull"]:
                print(f"FAIL {block} {props}: cull {gc} != {q['cull']} for {q['dir']}")
                bad += 1
            if not same_cycle(hit["verts"], q["verts"]):
                print(f"WARN {block} {props}: vertex order differs on {q['dir']} (winding)")
    print("piston_uv_ref: " + ("FAIL" if bad else "ok") + f" ({len(states)} states)")
    sys.exit(1 if bad else 0)


main()
