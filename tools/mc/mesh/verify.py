#!/usr/bin/env python3
"""Independent checker for mc_mesh: builds padded 18^3 inputs (several world shapes), runs the Firn
program, and verifies the mesh by BRUTE FORCE (its own face visibility, its own ambient occlusion):

  * the quads cover exactly the visible block faces, each exactly once (no hidden face, none missing);
  * every cell of a quad has the quad's tile and the quad's four corner values (AO + smooth light: the
    average of the non-opaque cells of the 2x2 patch in front of the face per corner, sky and block);
  * naive mode: one 1x1 quad per visible face; greedy mode: never more quads than naive;
  * the index buffer: 6 indices per quad into the quad's own 4 vertices, the diagonal joins the brighter
    pair of corners, and every triangle is wound counter-clockwise seen from outside.

usage: verify.py <firn-prog> <workdir>
       verify.py emit <mode> <seed> <out.bin>      write one padded input + <out.bin>.l the light (for the benchmark)
"""
import itertools, random, struct, subprocess, sys

N = 18
UNKNOWN = 65535


def tables():
    opaque = [0] * 32768
    tiles = [[0] * 6 for _ in range(32768)]
    for i in range(1, 10):
        for f in range(6):
            if 1 <= i <= 4: t = i * 10 + f + 1
            elif i in (5, 6): t = 100 + (i - 5) * 10 + f + 1
            elif i == 7: t = 200 + f + 1
            elif i == 9 and f != 3: t = 300 + f + 1
            else: t = 0
            tiles[i][f] = t
        if i in (1, 2, 3, 4, 9):
            opaque[i] = 1
    return opaque, tiles


OPAQUE, TILES = tables()


def idx(x, y, z): return (y * N + z) * N + x


def make(mode, seed):
    r = random.Random(seed)
    a = [0] * (N ** 3)
    inner = [(x, y, z) for y in range(1, 17) for z in range(1, 17) for x in range(1, 17)]
    if mode == "empty":
        pass
    elif mode == "full":
        a = [1] * (N ** 3)
    elif mode == "checker":
        for x in range(N):
            for y in range(N):
                for z in range(N):
                    a[idx(x, y, z)] = 1 if (x + y + z) % 2 == 0 else 0
    elif mode.startswith("random"):
        dens = {"random20": 0.2, "random50": 0.5, "random80": 0.8}[mode]
        for c in itertools.product(range(N), repeat=3):
            if r.random() < dens:
                a[idx(*c)] = r.choice([1, 1, 2, 3, 4, 5, 6, 7, 8, 9])
    elif mode == "mixed_unknown":
        for c in itertools.product(range(N), repeat=3):
            if r.random() < 0.5:
                a[idx(*c)] = r.choice([1, 2, 5, 6, 7, 9])
        for x in range(N):
            for y in range(N):
                for z in range(N):
                    if (x in (0, 17) or y in (0, 17) or z in (0, 17)) and r.random() < 0.6:
                        a[idx(x, y, z)] = UNKNOWN
    elif mode == "terrain":
        for x in range(N):
            for z in range(N):
                h = 4 + int(4 * (1 + __import__("math").sin(x * 0.5) * __import__("math").cos(z * 0.4)))
                for y in range(N):
                    a[idx(x, y, z)] = 1 if y < h - 1 else (2 if y == h - 1 else (5 if y == h and r.random() < 0.3 else 0))
    elif mode == "single":
        a[idx(8, 8, 8)] = 1
    elif mode == "glass_blob":
        for c in inner:
            if (c[0] - 8) ** 2 + (c[1] - 8) ** 2 + (c[2] - 8) ** 2 < 30:
                a[idx(*c)] = 5 if r.random() < 0.8 else 6
    elif mode == "layers":
        for y in range(N):
            for x in range(N):
                for z in range(N):
                    a[idx(x, y, z)] = [1, 2, 3, 4][y % 4]
    return a


NEIGH = [(-1, 0, 0), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)]


def make_light(mode, seed, kind):
    """padded light array (sky << 4 | block per cell). 'noise' = independent random nibbles everywhere
    (also in opaque / unknown cells: they must be ignored), 'smooth' = a smooth gradient, 'flat' = 15 / 0."""
    r = random.Random(seed * 31 + 7)
    lt = [0] * (N ** 3)
    for y in range(N):
        for z in range(N):
            for x in range(N):
                if kind == "noise":
                    v = (r.randrange(16) << 4) | r.randrange(16)
                elif kind == "smooth":
                    v = (max(0, 15 - (y // 2) - (x // 6)) << 4) | max(0, 12 - abs(x - 8) - abs(z - 8) - abs(y - 8) // 2)
                else:
                    v = 0xF0
                lt[idx(x, y, z)] = v
    return lt


def cell(a, p):
    return a[idx(*p)]


def occ(a, p):
    v = cell(a, p)
    return 1 if (v != UNKNOWN and v < 32768 and OPAQUE[v]) else 0


def visible(a, x, y, z, f, cull_same):
    id_ = a[idx(x, y, z)]
    if id_ == UNKNOWN or id_ >= 32768:
        return None
    t = TILES[id_][f]
    if t == 0:
        return None
    d = NEIGH[f]
    nb = (x + d[0], y + d[1], z + d[2])
    b = cell(a, nb)
    if b == UNKNOWN or (b < 32768 and OPAQUE[b]) or (cull_same and b == id_):
        return None
    return t, nb


def ao_corner(a, nb, f, cu, cv):
    axis = f // 2
    ua, va = (axis + 1) % 3, (axis + 2) % 3
    def off(du, dv):
        p = list(nb); p[ua] += du; p[va] += dv; return tuple(p)
    su = 1 if cu else -1
    sv = 1 if cv else -1
    s1, s2, c = occ(a, off(su, 0)), occ(a, off(0, sv)), occ(a, off(su, sv))
    return 0 if (s1 and s2) else 3 - (s1 + s2 + c)


def light_corner(a, lt, nb, f, cu, cv):
    """(sky, block) of one corner: front cell + side cells + diagonal; opaque / unknown cells skipped, the
    diagonal skipped when both sides are opaque; average rounded half up."""
    axis = f // 2
    ua, va = (axis + 1) % 3, (axis + 2) % 3
    su = 1 if cu else -1
    sv = 1 if cv else -1
    def off(du, dv):
        p = list(nb); p[ua] += du; p[va] += dv; return tuple(p)
    def solid(p):
        v = cell(a, p)
        return v == UNKNOWN or (v < 32768 and OPAQUE[v])
    cells = [nb]
    pb, pc, pd = off(su, 0), off(0, sv), off(su, sv)
    if not solid(pb): cells.append(pb)
    if not solid(pc): cells.append(pc)
    if not (solid(pb) and solid(pc)) and not solid(pd): cells.append(pd)
    n = len(cells)
    ss = sum(lt[idx(*p)] >> 4 for p in cells)
    sb = sum(lt[idx(*p)] & 15 for p in cells)
    return (2 * ss + n) // (2 * n), (2 * sb + n) // (2 * n)


def unit_ao(a, lt, nb, f, use_ao, use_light):
    out = []
    for cu, cv in ((0, 0), (1, 0), (1, 1), (0, 1)):
        ao = ao_corner(a, nb, f, cu, cv) if use_ao else 3
        s, b = light_corner(a, lt, nb, f, cu, cv) if use_light else (15, 0)
        out.append((ao, s, b))
    return tuple(out)


def bright(c):
    return max(c[1], c[2]) * (100 + 52 * c[0])


def run(prog, path, flags, lpath):
    out = subprocess.run([prog, "mesh", path, str(flags), lpath], capture_output=True, text=True, check=True).stdout.split("\n")
    head = out[0].split()
    stats = dict(kv.split("=") for kv in head[1:])
    quads = [tuple(map(int, l.split()[1:])) for l in out if l.startswith("Q ")]
    tris = [tuple(map(int, l.split()[1:])) for l in out if l.startswith("I ")]
    return {k: int(v) for k, v in stats.items()}, quads, tris


def check(a, lt, flags, res):
    greedy, use_ao, cull, use_light = bool(flags & 1), bool(flags & 2), bool(flags & 4), bool(flags & 16)
    stats, quads, tris = res
    want = {}
    for x in range(1, 17):
        for y in range(1, 17):
            for z in range(1, 17):
                for f in range(6):
                    v = visible(a, x, y, z, f, cull)
                    if v:
                        want[(x, y, z, f)] = (v[0], unit_ao(a, lt, v[1], f, use_ao, use_light))
    assert stats["nfaces"] == len(want), "nfaces %d != brute force %d" % (stats["nfaces"], len(want))
    assert stats["nquads"] == len(quads) and stats["nverts"] == 4 * len(quads) and stats["nidx"] == 6 * len(quads), "counts"
    seen = {}
    for qi, (f, pc, u0, v0, w, h, tile, *cvs) in enumerate(quads):
        cvs = tuple(tuple(cvs[3 * k:3 * k + 3]) for k in range(4))
        assert w >= 1 and h >= 1 and 0 <= u0 and u0 + w <= 16 and 0 <= v0 and v0 + h <= 16, "quad out of range %r" % ((f, pc, u0, v0, w, h),)
        axis = f // 2
        ua, va = (axis + 1) % 3, (axis + 2) % 3
        for i in range(w):
            for j in range(h):
                c = [0, 0, 0]
                c[axis] = pc if f % 2 == 0 else pc - 1
                c[ua] = u0 + i
                c[va] = v0 + j
                key = (c[0] + 1, c[1] + 1, c[2] + 1, f)
                assert key in want, "quad %d covers a face that is not visible: %r" % (qi, key)
                assert key not in seen, "face %r covered twice" % (key,)
                seen[key] = qi
                wt, wao = want[key]
                assert wt == tile, "tile %d != %d at %r" % (tile, wt, key)
                assert wao == cvs, "corner values (ao, sky, block) %r != quad %r at %r" % (wao, cvs, key)
        if not greedy:
            assert (w, h) == (1, 1), "naive mode made a merged quad"
    assert len(seen) == len(want), "%d visible faces, %d covered" % (len(want), len(seen))
    assert len(tris) == 2 * len(quads)
    for qi, (f, pc, u0, v0, w, h, tile, *cvs) in enumerate(quads):
        cvs = tuple(tuple(cvs[3 * k:3 * k + 3]) for k in range(4))
        c0, c1, c2, c3 = cvs
        t1, t2 = tris[2 * qi], tris[2 * qi + 1]
        ids = set(t1) | set(t2)
        assert ids == set(range(4 * qi, 4 * qi + 4)), "triangles of quad %d use foreign vertices" % qi
        diag = set(t1) & set(t2)
        base = 4 * qi
        want_diag = {base + 1, base + 3} if (bright(c1) + bright(c3) > bright(c0) + bright(c2)) else {base + 0, base + 2}
        assert diag == want_diag, "diagonal %r != %r (corners %r)" % (diag, want_diag, cvs)
        # winding: positions of the 4 corners (u,v) -> 3D, normal of each triangle vs the face normal
        axis = f // 2
        ua, va = (axis + 1) % 3, (axis + 2) % 3
        def pos(k):
            su, sv = [(0, 0), (1, 0), (1, 1), (0, 1)][k]
            p = [0, 0, 0]; p[axis] = pc; p[ua] = u0 + su * w; p[va] = v0 + sv * h
            return p
        for t in (t1, t2):
            p0, p1, p2 = (pos(i - base) for i in t)
            e1 = [p1[i] - p0[i] for i in range(3)]; e2 = [p2[i] - p0[i] for i in range(3)]
            n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]]
            sign = 1 if f % 2 == 1 else -1
            assert n[axis] * sign > 0, "triangle of quad %d wound the wrong way (face %d)" % (qi, f)
    return len(quads), len(want)


def main():
    if sys.argv[1] == "emit":
        a = make(sys.argv[2], int(sys.argv[3]))
        open(sys.argv[4], "wb").write(struct.pack("<%dH" % len(a), *a))
        open(sys.argv[4] + ".l", "wb").write(bytes(make_light(sys.argv[2], int(sys.argv[3]), "smooth")))
        return
    prog, work = sys.argv[1], sys.argv[2]
    modes = ["empty", "full", "checker", "random20", "random50", "random80", "mixed_unknown", "terrain", "single", "glass_blob", "layers"]
    total = 0
    worst_greedy = 0
    for mi, mode in enumerate(modes):
        for seed in ((1,) if mode in ("empty", "full", "checker", "single", "layers", "glass_blob") else (1, 2)):
            a = make(mode, seed * 100 + mi)
            path = "%s/in.bin" % work
            open(path, "wb").write(struct.pack("<%dH" % len(a), *a))
            counts = {}
            lkind = "noise" if seed == 1 else "smooth"
            lt = make_light(mode, seed, lkind)
            lpath = "%s/in.l" % work
            open(lpath, "wb").write(bytes(lt))
            for flags in list(range(8)) + list(range(16, 24)):
                res = run(prog, path, flags, lpath)
                try:
                    counts[flags] = check(a, lt, flags, res)
                except AssertionError as e:
                    print("FAIL mode=%s seed=%d flags=%d: %s" % (mode, seed, flags, e)); sys.exit(1)
                total += 1
            # greedy never needs more quads than naive for the same AO / cull setting
            for f2 in (0, 2, 4, 6, 16, 18, 20, 22):
                assert counts[f2 | 1][0] <= counts[f2][0], "greedy produced more quads than naive (%s)" % mode
                assert counts[f2][0] == counts[f2][1], "naive quad count != visible faces"
            print("  ok    %-14s seed %d: 16 flag combinations (light=bit 16) (light %s); quads naive/greedy (AO on): %d / %d, with light %d / %d for %d visible faces" % (mode, seed, lkind, counts[2][0], counts[3][0], counts[18][0], counts[19][0], counts[2][1]))
    print("verified %d meshes against the brute force" % total)


main()
