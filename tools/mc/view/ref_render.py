#!/usr/bin/env python3
"""tools/mc/view/ref_render.py -- independent reference for demos/mcview.

Reads the world file <dir>/world.mcw (the received world, format in demos/mcview/main.fi) and <dir>/view.txt (camera as
f64 bit patterns, block tables, the Firn image hash), builds the geometry with its OWN mesher (numpy, naive: one quad per
visible face, own visibility rule, own corner ambient occlusion -- written from docs/mc/mesh.md, not from lib/mc/mc_mesh.fi),
renders it with the python rasterizer of tools/mc/raster/ref.py and compares the image with <dir>/view.rgba byte for byte.

What is shared with the Firn side (and therefore NOT independently checked here): the block tables (state id -> face tiles /
opaque; checked on their own by tools/mc/texture) and the atlas pixels. What is independent: section assembly with the 3x3x3
neighbourhood, face visibility, AO, vertex words / triangulation, section culling (python draws every section within the
distance and drops only sections that are provably behind the near plane), the whole rasterization.

usage: ref_render.py <dir> [world-file]      exit 0 = identical, 1 = difference, 2 = not comparable (greedy meshing)
"""
import hashlib
import importlib.util
import math
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("raster_ref", os.path.join(HERE, "..", "raster", "ref.py"))
raster_ref = importlib.util.module_from_spec(spec)
spec.loader.exec_module(raster_ref)

UNKNOWN = 65535
NEIGH = [(-1, 0, 0), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)]   # (dx, dy, dz) per face


def f64(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


def read_view(path):
    v = dict(tables={}, metrics={})
    for line in open(path):
        t = line.split()
        if not t:
            continue
        k = t[0]
        if k == "protocol":
            v["protocol"] = int(t[1])
        elif k == "min_y":
            v["min_y"] = int(t[1])
        elif k == "size":
            v["w"], v["h"] = int(t[1]), int(t[2])
        elif k == "dist":
            v["dist"] = int(t[1])
        elif k == "flags":
            v["flags"] = int(t[1])
        elif k == "fog":
            v["fog"] = t[1] == "1"
            v["fs"], v["fe"] = f64(t[2]), f64(t[3])
            v["clear"] = int(t[4], 16)
        elif k == "cam":
            x = [f64(a) for a in t[1:]]
            v["cam"] = dict(pos=x[0:3], right=x[3:6], up=x[6:9], fwd=x[9:12], near=x[12], tan_x=x[13], tan_y=x[14])
        elif k == "hash":
            v["hash"] = t[1]
        elif k == "T":
            v["tables"][int(t[1])] = (int(t[2]), [int(a) for a in t[3:9]])
        elif k == "metric":
            kk, vv = t[1].split("=")
            v["metrics"][kk] = int(vv)
    return v


def read_world(path):
    """-> (min_y, {(cx, cz): uint16 array [row=y-min_y][z][x]})"""
    toks = open(path).read().split()
    p = 0
    assert toks[0] == "mcworld" and toks[1] == "1"
    p = 2
    meta = {}
    while toks[p] != "chunks":
        key = toks[p]
        if key in ("protocol", "min_y", "view"):
            meta[key] = int(toks[p + 1])
            p += 2
        elif key == "cam":
            p += 6
        else:
            raise ValueError("bad world header at " + key)
    nch = int(toks[p + 1])
    p += 2
    cols = {}
    for _ in range(nch):
        assert toks[p] == "C"
        cx, cz, nsec = int(toks[p + 1]), int(toks[p + 2]), int(toks[p + 3])
        p += 4
        col = np.zeros((nsec * 16, 16, 16), np.uint16)
        for s in range(nsec):
            kind, si = toks[p], int(toks[p + 1])
            assert si == s
            if kind == "U":
                col[s * 16:(s + 1) * 16] = int(toks[p + 2])
                p += 3
            else:
                nr = int(toks[p + 2])
                p += 3
                ids = np.empty(4096, np.uint16)
                at = 0
                for _r in range(nr):
                    ln = int(toks[p + 1])
                    ids[at:at + ln] = int(toks[p])
                    at += ln
                    p += 2
                assert at == 4096
                col[s * 16:(s + 1) * 16] = ids.reshape(16, 16, 16)    # index (y*16+z)*16+x
        cols[(cx, cz)] = col
    return meta["min_y"], cols, meta


def padded(cols, cx, sy, cz):
    """18^3 ids [y][z][x] of the section plus a one block shell; 65535 = not loaded / outside the world"""
    P = np.full((18, 18, 18), UNKNOWN, np.uint16)
    nrows = next(iter(cols.values())).shape[0]
    y0, y1 = sy * 16 - 1, sy * 16 + 17
    ya0, ya1 = max(y0, 0), min(y1, nrows)
    if ya0 >= ya1:
        return P
    for dz in (-1, 0, 1):
        for dx in (-1, 0, 1):
            col = cols.get((cx + dx, cz + dz))
            if col is None:
                continue
            px0, pz0 = 1 + 16 * dx, 1 + 16 * dz
            xa0, xa1 = max(px0, 0), min(px0 + 16, 18)
            za0, za1 = max(pz0, 0), min(pz0 + 16, 18)
            P[ya0 - y0:ya1 - y0, za0:za1, xa0:xa1] = col[ya0:ya1, za0 - pz0:za1 - pz0, xa0 - px0:xa1 - px0]
    return P


def mesh_section(P, opq_tab, tiles_tab, use_ao, cull_same):
    """naive mesh: list of (vertex words [4 x (w0, w1)], indices [6]) -- 1x1 quad per visible face"""
    inner = P[1:17, 1:17, 1:17]
    known = inner != UNKNOWN
    idc = np.where(known, inner, 0).astype(np.int64)
    pc = np.minimum(P, 32767).astype(np.int64)
    occ_pad = ((P != UNKNOWN) & (opq_tab[pc] != 0)).astype(np.uint8)
    occ_flat = occ_pad.reshape(-1).tolist()                      # index (y*18+z)*18+x

    def occ(x, y, z):
        return occ_flat[(y * 18 + z) * 18 + x]

    quads = []
    for f in range(6):
        dx, dy, dz = NEIGH[f]
        nb = P[1 + dy:17 + dy, 1 + dz:17 + dz, 1 + dx:17 + dx]
        tile = tiles_tab[idc, f]
        vis = known & (tile != 0) & (nb != UNKNOWN) & (opq_tab[np.minimum(nb, 32767).astype(np.int64)] == 0)
        if cull_same:
            vis &= ~(nb == inner)
        ys, zs, xs = np.nonzero(vis)
        axis = f // 2
        ua, va = (axis + 1) % 3, (axis + 2) % 3
        for y, z, x in zip(ys.tolist(), zs.tolist(), xs.tolist()):
            c = [x, y, z]                                         # block cell (x, y, z) inside the section
            nbp = [x + 1 + dx, y + 1 + dy, z + 1 + dz]            # padded coordinates of the neighbour cell
            ao = []
            for cu, cv in ((0, 0), (1, 0), (1, 1), (0, 1)):
                if use_ao:
                    su, sv = (1 if cu else -1), (1 if cv else -1)
                    p1 = list(nbp); p1[ua] += su
                    p2 = list(nbp); p2[va] += sv
                    p3 = list(nbp); p3[ua] += su; p3[va] += sv
                    s1, s2, cc = occ(*p1), occ(*p2), occ(*p3)
                    ao.append(0 if (s1 and s2) else 3 - (s1 + s2 + cc))
                else:
                    ao.append(3)
            plane = c[axis] + (1 if f % 2 == 1 else 0)
            verts = []
            for k, (su, sv) in enumerate(((0, 0), (1, 0), (1, 1), (0, 1))):
                pos = [0, 0, 0]
                pos[axis] = plane
                pos[ua] = c[ua] + su
                pos[va] = c[va] + sv
                w0 = pos[0] | (pos[1] << 8) | (pos[2] << 16) | (f << 24) | (ao[k] << 27)
                w1 = int(tile[y, z, x]) | (su << 16) | (sv << 24)
                verts.append((w0, w1))
            a0, a1, a2, a3 = ao
            q = [0, 1, 2, 0, 2, 3]
            if a1 + a3 > a0 + a2:
                q = [1, 2, 3, 1, 3, 0]
            if f % 2 == 0:                     # negative faces: swap the 2nd and 3rd index of each triangle
                q = [q[0], q[2], q[1], q[3], q[5], q[4]]
            quads.append((verts, q))
    return quads


def main():
    d = sys.argv[1]
    wpath = sys.argv[2] if len(sys.argv) > 2 else os.path.join(d, "world.mcw")
    view = read_view(os.path.join(d, "view.txt"))
    if view["flags"] & 1:
        print("  skip  greedy meshing was used: the python reference meshes naively, images are not comparable")
        return 2
    use_ao, cull_same = bool(view["flags"] & 2), bool(view["flags"] & 4)
    min_y, cols, meta = read_world(wpath)
    assert meta["protocol"] == view["protocol"] and min_y == view["min_y"]
    opq = np.zeros(32768, np.uint8)
    tiles = np.zeros((32768, 6), np.int64)
    for sid, (o, t) in view["tables"].items():
        opq[sid] = o
        tiles[sid] = t
    cam = view["cam"]
    atlas = dict(tile=16, cols=16, w=256, data=open(os.path.join(d, "atlas.bin"), "rb").read())
    sc = dict(name="view", w=view["w"], h=view["h"], cull=True, fog=view["fog"], fs=view["fs"], fe=view["fe"],
              clear=view["clear"], cam=cam)
    scene = raster_ref.Scene(sc, atlas)
    ccx, ccz = math.floor(cam["pos"][0]) // 16, math.floor(cam["pos"][2]) // 16
    fwd = cam["fwd"]
    order = []
    for (cx, cz), col in cols.items():
        if abs(cx - ccx) > view["dist"] or abs(cz - ccz) > view["dist"]:
            continue
        for sy in range(col.shape[0] // 16):
            ox, oy, oz = cx * 16, min_y + sy * 16, cz * 16
            cxm, cym, czm = ox + 8 - cam["pos"][0], oy + 8 - cam["pos"][1], oz + 8 - cam["pos"][2]
            # exact skip: every vertex of the section is within 8*sqrt(3) < 14 of the centre, so if the centre is more
            # than 14 behind the near plane (along the view direction) no vertex survives near clipping
            if cxm * fwd[0] + cym * fwd[1] + czm * fwd[2] < cam["near"] - 14.0:
                continue
            order.append((cxm * cxm + cym * cym + czm * czm, cx, sy, cz))
    order.sort()
    nq = 0
    ntri = 0
    for _d, cx, sy, cz in order:
        col = cols[(cx, cz)]
        if (col[sy * 16:(sy + 1) * 16] == col[sy * 16, 0, 0]).all() and tiles[col[sy * 16, 0, 0]].sum() == 0:
            continue                                                # uniform invisible section (air)
        P = padded(cols, cx, sy, cz)
        quads = mesh_section(P, opq, tiles, use_ao, cull_same)
        if not quads:
            continue
        verts, idx = [], []
        for vs, q in quads:
            base = len(verts)
            verts.extend(vs)
            idx.extend(base + i for i in q)
        nq += len(quads)
        ntri += len(idx) // 3
        scene.draw_mesh(dict(o=[cx * 16, min_y + sy * 16, cz * 16], verts=verts, idx=idx))
    img = scene.rgba()
    hx = hashlib.sha256(img).hexdigest()
    open(os.path.join(d, "ref.rgba"), "wb").write(img)
    fb = open(os.path.join(d, "view.rgba"), "rb").read()
    diff = sum(1 for i in range(0, len(fb), 4) if fb[i:i + 4] != img[i:i + 4]) if len(fb) == len(img) else -1
    painted = sum(1 for c in scene.color if c != view["clear"])
    same_hash = hx == view["hash"]
    print("  %s python reference: %d sections meshed, %d quads, %d triangles, %d painted pixels, differing pixels %d" % (
        "ok  " if diff == 0 and same_hash else "DIFF", len(order), nq, ntri, painted, diff))
    print("        python hash %s\n        firn   hash %s" % (hx, view["hash"]))
    if painted < view["w"] * view["h"] // 20:
        print("  DIFF  the image is nearly empty (%d painted pixels): the comparison would prove nothing" % painted)
        return 1
    return 0 if (diff == 0 and same_hash) else 1


if __name__ == "__main__":
    sys.exit(main())
