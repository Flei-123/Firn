#!/usr/bin/env python3
"""tools/mc/math/check.py -- independent verification of the lines written by dump.fi.

Reference implementations here share no code with lib/mc/mc_math.fi:
  M / I / T   numpy matmul / inv / transpose (column-major flat -> reshape(4,4).T)
  P / B / S   projection + look_at properties, planes vs clip-space test, boxes and
              spheres by corner projection and random sampling
  W / R       voxel raycast vs brute force: slab test against EVERY solid cell, plus
              sampling along the ray
  V           box sweep vs brute force over ALL solid cells, plus no-overlap and
              maximality properties

usage: check.py <dump.txt>        exit 0 = every check passed
Also runs a counter-check: damaged copies of the dump MUST be rejected.
"""
import struct
import sys
import math
import numpy as np

EPS = 1e-9


def fl(tok):
    return struct.unpack('>d', bytes.fromhex(tok))[0]


def mat(toks):
    return np.array([fl(t) for t in toks]).reshape(4, 4).T


def check(lines, stats):
    errs = []
    rng = np.random.default_rng(12345)

    def err(msg):
        if len(errs) < 20:
            errs.append(msg)
        else:
            errs.append(None)

    i = 0
    n = len(lines)
    cam = None  # (vp, zo) of the current P block
    world = None
    while i < n:
        t = lines[i].split()
        i += 1
        if not t:
            continue
        k = t[0]
        if k == 'M':
            a, b, c = mat(t[1:17]), mat(t[17:33]), mat(t[33:49])
            if not np.allclose(a @ b, c, rtol=1e-12, atol=1e-12):
                err('M product differs')
            stats['M'] += 1
        elif k == 'T':
            a, c = mat(t[1:17]), mat(t[17:33])
            if not np.array_equal(a.T, c):
                err('T transpose differs')
            stats['T'] += 1
        elif k == 'I':
            a = mat(t[1:17])
            ok = int(t[17])
            inv = mat(t[18:34])
            cond = np.linalg.cond(a)
            if cond > 1e15 or not np.isfinite(cond):
                if ok != 0:
                    err('I singular matrix reported invertible, cond=%g' % cond)
            elif cond < 1e11:
                if ok != 1:
                    err('I regular matrix (cond=%g) reported singular' % cond)
                else:
                    want = np.linalg.inv(a)
                    if not np.allclose(inv, want, rtol=1e-6 * cond, atol=1e-9 * cond):
                        err('I inverse differs, cond=%g' % cond)
                    if not np.allclose(a @ inv, np.eye(4), atol=1e-9 * cond):
                        err('I a*inv != identity')
            stats['I'] += 1
        elif k == 'P':
            zo = int(t[1])
            f = [fl(x) for x in t[2:15]]
            fovy, aspect, near, far = f[0:4]
            eye = np.array(f[4:7])
            cen = np.array(f[7:10])
            proj, view, vp = mat(t[15:31]), mat(t[31:47]), mat(t[47:63])
            planes = np.array([fl(x) for x in t[63:87]]).reshape(6, 4)
            cam = (vp, zo, planes)
            # projection properties
            th = math.tan(fovy / 2)
            for dist, zwant in ((near, 0.0 if zo else -1.0), (far, 1.0)):
                v = proj @ np.array([0, 0, -dist, 1.0])
                if abs(v[2] / v[3] - zwant) > 1e-9:
                    err('P depth at %g is %g, want %g' % (dist, v[2] / v[3], zwant))
                v = proj @ np.array([aspect * th * dist, th * dist, -dist, 1.0])
                if abs(v[0] / v[3] - 1) > 1e-9 or abs(v[1] / v[3] - 1) > 1e-9:
                    err('P frustum edge not at ndc 1')
            # view properties
            v = view @ np.append(eye, 1.0)
            if np.abs(v[:3]).max() > 1e-9:
                err('P eye not at the origin of view space')
            v = view @ np.append(cen, 1.0)
            d = np.linalg.norm(cen - eye)
            if abs(v[0]) > 1e-9 or abs(v[1]) > 1e-9 or abs(v[2] + d) > 1e-9 * max(1, d):
                err('P look-at centre not on -z')
            r = view[:3, :3]
            if not np.allclose(r @ r.T, np.eye(3), atol=1e-12):
                err('P view rotation not orthonormal')
            if not np.allclose(vp, proj @ view, rtol=1e-12, atol=1e-12):
                err('P vp != proj*view')
            # planes against the clip-space test on random points
            pts = eye + rng.uniform(-1, 1, (4000, 3)) * rng.choice([5, 40, 200], (4000, 1))
            h = np.hstack([pts, np.ones((len(pts), 1))])
            c = h @ vp.T
            w = c[:, 3]
            slack = np.stack([w + c[:, 0], w - c[:, 0], w + c[:, 1], w - c[:, 1],
                              (c[:, 2] if zo else w + c[:, 2]), w - c[:, 2]], axis=1)
            by_clip = (slack >= 0).all(axis=1)
            by_planes = ((h @ planes.T) >= 0).all(axis=1)
            # skip points too near a boundary (slack scale: planes are normalised separately)
            marg = np.abs(h @ planes.T).min(axis=1) < 1e-7
            bad = (by_clip != by_planes) & ~marg
            if bad.any():
                err('P planes disagree with clip-space test at %d of %d points' % (bad.sum(), len(pts)))
            stats['P'] += 1
        elif k == 'B':
            vp, zo, planes = cam
            x0, y0, z0, x1, y1, z1 = [fl(x) for x in t[1:7]]
            res = int(t[7])
            cs = np.array([[x, y, z, 1.0] for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)])
            pts = np.vstack([cs, np.column_stack([rng.uniform(x0, x1, 1500), rng.uniform(y0, y1, 1500),
                                                  rng.uniform(z0, z1, 1500), np.ones(1500)])])

            def inside(p, margin=0.0):
                return ((p @ planes.T) >= margin).all(axis=1)

            any_in = inside(pts).any()
            corners_in = inside(cs, 1e-7).all()
            corners_not_in = (~inside(cs, -1e-7)).any()
            if any_in and res == 0:
                err('B box with a point inside reported outside')
            if corners_in and res != 2:
                err('B box with all corners inside reported %d' % res)
            if res == 2 and corners_not_in:
                err('B box reported fully inside but a corner is outside')
            if not any_in:
                stats['B_out_sampled'] += 1
            if res == 1 and not any_in:
                stats['B_fp'] += 1
            stats['B_res%d' % res] += 1
            stats['B'] += 1
        elif k == 'S':
            vp, zo, planes = cam
            cx, cy, cz, r = [fl(x) for x in t[1:5]]
            res = int(t[5])
            d = rng.normal(size=(1500, 3))
            d /= np.linalg.norm(d, axis=1, keepdims=True)
            d *= (r * rng.uniform(0, 1, (1500, 1)) ** (1 / 3))
            pts = np.hstack([np.array([cx, cy, cz]) + d, np.ones((1500, 1))])
            ins = ((pts @ planes.T) >= 0).all(axis=1)
            cin = ((np.array([cx, cy, cz, 1.0]) @ planes.T) >= 0).all()
            if (ins.any() or cin) and res == 0:
                err('S sphere with a point inside reported outside')
            if res == 2 and not ins.all():
                err('S sphere reported fully inside but samples are outside')
            stats['S_res%d' % res] += 1
            stats['S'] += 1
        elif k == 'W':
            ox, oy, oz, nx, ny, nz = [int(x) for x in t[1:7]]
            sol = [0, int(t[7]), int(t[8]), int(t[9])]
            sol = [0, sol[1], sol[2], sol[3]]
            ids = lines[i].strip()
            i += 1
            assert len(ids) == nx * ny * nz, 'world size'
            arr = np.frombuffer(ids.encode(), dtype=np.uint8) - 48
            sol_tab = np.array([0, sol[1], sol[2], sol[3]], dtype=bool)
            grid = sol_tab[arr].reshape(ny, nz, nx)  # [y][z][x]
            ys, zs, xs = np.nonzero(grid)
            cells = np.stack([xs + ox, ys + oy, zs + oz], axis=1)  # world cells
            world = (ox, oy, oz, nx, ny, nz, grid, cells)
        elif k == 'R':
            ox_, oy_, oz_, nx_, ny_, nz_, grid, cells = world
            o = np.array([fl(x) for x in t[1:4]])
            d0 = np.array([fl(x) for x in t[4:7]])
            maxd = fl(t[7])
            hit = int(t[8])
            stats['R'] += 1
            ln = math.sqrt(d0[0] * d0[0] + d0[1] * d0[1] + d0[2] * d0[2])
            if ln == 0:
                if hit:
                    err('R zero direction reported a hit')
                continue
            d = d0 / ln

            def solid_at(c):
                x, y, z = c[0] - ox_, c[1] - oy_, c[2] - nz_ * 0 - oz_
                if x < 0 or y < 0 or z < 0 or x >= nx_ or y >= ny_ or z >= nz_:
                    return False
                return bool(grid[y, z, x])

            def any_solid(pts_):
                # drop samples that sit exactly on a cell boundary of a moving axis
                # (a corner touch: which cell "contains" it is a convention, not geometry)
                amb = np.zeros(len(pts_), bool)
                for a_ in range(3):
                    if d[a_] != 0:
                        amb |= np.abs(pts_[:, a_] - np.round(pts_[:, a_])) < 1e-9
                pts_ = pts_[~amb]
                if len(pts_) == 0:
                    return False
                c_ = np.floor(pts_).astype(int)
                x_ = c_[:, 0] - ox_
                y_ = c_[:, 1] - oy_
                z_ = c_[:, 2] - oz_
                inb = (x_ >= 0) & (y_ >= 0) & (z_ >= 0) & (x_ < nx_) & (y_ < ny_) & (z_ < nz_)
                if not inb.any():
                    return False
                return bool(grid[y_[inb], z_[inb], x_[inb]].any())

            m = len(cells)
            if m:
                lo = cells.astype(float)
                hi = lo + 1
                tmin = np.zeros(m)
                tmax = np.full(m, np.inf)
                okc = np.ones(m, bool)
                tl_axis = []
                for a in range(3):
                    if d[a] == 0:
                        okc &= (o[a] >= lo[:, a]) & (o[a] < hi[:, a])
                        tl_axis.append(np.full(m, -np.inf))
                    else:
                        t0 = (lo[:, a] - o[a]) / d[a]
                        t1 = (hi[:, a] - o[a]) / d[a]
                        tl = np.minimum(t0, t1)
                        th = np.maximum(t0, t1)
                        tl_axis.append(tl)
                        tmin = np.maximum(tmin, tl)
                        tmax = np.minimum(tmax, th)
                okc &= tmin <= tmax
                proper = okc & ((tmax - tmin) > EPS) & (tmin <= maxd)
            else:
                okc = np.zeros(0, bool)
                proper = okc
                tmin = np.zeros(0)
            P = tmin[proper].min() if proper.any() else None
            oc = [math.floor(o[0]), math.floor(o[1]), math.floor(o[2])]
            if solid_at(oc):
                if not hit:
                    err('R origin inside solid but no hit')
                else:
                    h = [int(x) for x in t[9:15]]
                    dist = fl(t[15])
                    if dist != 0.0 or h[3:] != [0, 0, 0] or h[:3] != oc:
                        err('R origin inside solid: wrong hit %s %g' % (h, dist))
                continue
            if hit:
                h = [int(x) for x in t[9:15]]
                dist = fl(t[15])
                if dist > maxd + 1e-12:
                    err('R hit beyond maxd')
                found = False
                idx = np.nonzero((cells[:, 0] == h[0]) & (cells[:, 1] == h[1]) & (cells[:, 2] == h[2]))[0] if m else []
                if len(idx) == 0:
                    err('R hit cell %s is not solid' % h[:3])
                else:
                    j = idx[0]
                    if not okc[j] or abs(tmin[j] - dist) > 1e-9:
                        err('R hit cell %s dist %g but slab says ok=%s t=%g' % (h[:3], dist, okc[j], tmin[j]))
                    if P is not None and dist > P + 1e-9:
                        err('R hit at %g but a nearer proper cell exists at %g' % (dist, P))
                    # normal, when unambiguous
                    near_ties = proper & (np.abs(tmin - tmin[j]) < 1e-9)
                    if proper[j] and near_ties.sum() == 1:
                        tls = np.array([tl_axis[a][j] for a in range(3)])
                        order = np.argsort(-tls)
                        if abs(tls[order[0]] - tls[order[1]]) > 1e-9:
                            a = order[0]
                            want = [0, 0, 0]
                            want[a] = -int(np.sign(d[a]))
                            if h[3:] != want:
                                err('R normal %s want %s' % (h[3:], want))
                # sampling: nothing solid strictly before the hit
                ts = np.arange(0.0, max(dist - 1e-6, 0.0), 0.01)
                if len(ts):
                    if any_solid(o + ts[:, None] * d):
                        err('R sampling finds a solid cell before the hit')
                stats['R_hit'] += 1
            else:
                if P is not None and P < maxd - 1e-9:
                    err('R miss but a proper cell is at %g (maxd %g)' % (P, maxd))
                ts = np.arange(0.0, maxd, 0.01)
                if any_solid(o + ts[:, None] * d):
                    err('R miss but sampling finds a solid cell')
        elif k == 'V':
            ox_, oy_, oz_, nx_, ny_, nz_, grid, cells = world
            f = [fl(x) for x in t[1:13]]
            b = list(f[0:6])  # x0 y0 z0 x1 y1 z1
            mv = f[6:9]
            got = f[9:12]
            flags = [int(x) for x in t[13:16]]
            stats['V'] += 1
            # reference: same documented algorithm, brute force over ALL solid cells
            cl = cells.astype(float)

            def move_axis(axis, d, bx):
                o1, o2 = [a for a in range(3) if a != axis]
                lo, hi = bx[axis], bx[axis + 3]
                if len(cl) == 0 or d == 0:
                    return d
                e = EPS
                perp = ((bx[o1] + e < cl[:, o1] + 1) & (cl[:, o1] + e < bx[o1 + 3]) &
                        (bx[o2] + e < cl[:, o2] + 1) & (cl[:, o2] + e < bx[o2 + 3]))
                c = cl[perp, axis]
                if d > 0:
                    sel = c[hi <= c]
                    if len(sel):
                        d = min(d, float((sel - hi).min()))
                else:
                    sel = c[lo >= c + 1]
                    if len(sel):
                        d = max(d, float((sel + 1 - lo).max()))
                return d

            bx = list(b)
            want = [0.0, 0.0, 0.0]
            wflags = [0, 0, 0]
            states = []
            for axis in (1, 0, 2):
                states.append(list(bx))
                d = move_axis(axis, mv[axis], bx)
                want[axis] = d
                wflags[axis] = 1 if d != mv[axis] else 0
                bx[axis] += d
                bx[axis + 3] += d
            for a in range(3):
                if abs(got[a] - want[a]) > 1e-12:
                    err('V axis %d moved %.17g, brute force %.17g' % (a, got[a], want[a]))
            if flags != wflags:
                err('V flags %s want %s' % (flags, wflags))
            # independent properties
            def pen_overlap(box, margin):
                if len(cl) == 0:
                    return False
                m_ = np.ones(len(cl), bool)
                for a in range(3):
                    m_ &= (box[a] + margin < cl[:, a] + 1) & (cl[:, a] + margin < box[a + 3])
                return bool(m_.any())

            if not pen_overlap(b, 1e-7):
                if pen_overlap(bx, 1e-7):
                    err('V final box overlaps a solid block')
            # maximality of every blocked axis: 1e-6 further would penetrate
            for idx_, axis in enumerate((1, 0, 2)):
                if wflags[axis]:
                    st = list(states[idx_])
                    sgn = 1.0 if mv[axis] > 0 else -1.0
                    st[axis] += want[axis] + sgn * 1e-6
                    st[axis + 3] += want[axis] + sgn * 1e-6
                    if not pen_overlap(st, 0.0) and not pen_overlap(states[idx_], 1e-7):
                        err('V axis %d blocked but 1e-6 further is free' % axis)
            if sum(wflags):
                stats['V_blocked'] += 1
        elif k == 'E':
            world = None
        else:
            err('unknown line kind %r' % k)
    return [e for e in errs if e is not None], sum(1 for e in errs if e is None) + len([e for e in errs if e is not None])


class Stats(dict):
    def __missing__(self, k):
        return 0


def mutate(lines, kind):
    out = list(lines)
    for idx, ln in enumerate(out):
        t = ln.split()
        if not t:
            continue
        if kind == 'matrix' and t[0] == 'M':
            t[40] = '3ff0000000000000'
            out[idx] = ' '.join(t)
            return out
        if kind == 'ray' and t[0] == 'R' and t[8] == '1':
            t[9] = str(int(t[9]) + 1)
            out[idx] = ' '.join(t)
            return out
        if kind == 'frustum' and t[0] == 'B' and t[7] == '2':
            t[7] = '0'
            out[idx] = ' '.join(t)
            return out
        if kind == 'sweep' and t[0] == 'V' and t[13] != '3ff0000000000000':
            t[10] = '3fe0000000000000' if t[10] != '3fe0000000000000' else '3fd0000000000000'
            out[idx] = ' '.join(t)
            return out
        if kind == 'invert' and t[0] == 'I' and t[17] == '1':
            t[20] = '3ff0000000000000' if t[20] != '3ff0000000000000' else '4000000000000000'
            out[idx] = ' '.join(t)
            return out
    raise SystemExit('mutation %s found nothing to damage' % kind)


def main():
    lines = open(sys.argv[1]).read().split('\n')
    stats = Stats()
    errs, total = check(lines, stats)
    for e in errs:
        print('  FAIL ', e)
    s = ', '.join('%s=%d' % kv for kv in sorted(stats.items()))
    print('  cases:', s)
    if errs:
        print('RESULT check: %d errors' % total)
        return 1
    print('  ok    all %d cases agree with the independent references' %
          sum(stats[k] for k in 'MITPBSRV'))
    bad = 0
    for kind in ('matrix', 'ray', 'frustum', 'sweep', 'invert'):
        e2, _ = check(mutate(lines, kind), Stats())
        if not e2:
            print('  FAIL  counter-check %s: a damaged dump was accepted' % kind)
            bad = 1
        else:
            print('  ok    counter-check %s: damaged dump rejected' % kind)
    return bad


if __name__ == '__main__':
    sys.exit(main())
