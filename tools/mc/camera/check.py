#!/usr/bin/env python3
"""tools/mc/camera/check.py -- independent verification of the lines written by dump.fi.

The reference shares no code with lib/mc/mc_camera.fi:
  basis        built from rotation matrices (pitch about the right axis, yaw about +y), not from the
               closed form of the library
  view / proj  numpy matrices from the basis (rows right/up/-forward) and the textbook projection
  frustum      the 8 corner points (eye + forward * d +- right/up extents) -> six planes through
               4 corners each, normals pointing inward; compared with the library's planes
  inv_vp       must map the NDC cube corners to the same 8 corner points
  pick ray     near/far points unprojected with numpy's inverse of proj @ view
  projection   proj @ view, perspective divide, viewport mapping
  boxes /      exact separating-axis test box vs frustum polytope: an exact intersection must be
  sections     reported (never 0), "2" must mean all 8 corners inside, all corners inside must be
               reported as 2; false positives (1 although exactly outside) are counted only
usage: check.py <dump.txt>   exit 0 = every check passed.
Also runs a counter-check: damaged copies of the dump MUST be rejected.
"""
import struct
import sys
import math
import numpy as np


def fl(tok):
    return struct.unpack('>d', bytes.fromhex(tok))[0]


def mat(toks):
    return np.array([fl(t) for t in toks]).reshape(4, 4).T


def close(a, b, atol, rtol=1e-9):
    return np.allclose(a, b, atol=atol, rtol=rtol)


def basis(yaw_deg, pitch_deg):
    th = -math.radians(yaw_deg)
    p = math.radians(pitch_deg)
    f = np.array([0.0, -math.sin(p), math.cos(p)])
    r = np.array([-1.0, 0.0, 0.0])
    u = np.array([0.0, math.cos(p), math.sin(p)])
    ry = np.array([[math.cos(th), 0, math.sin(th)], [0, 1, 0], [-math.sin(th), 0, math.cos(th)]])
    return ry @ f, ry @ r, ry @ u


def frustum_corners(eye, f, r, u, fov, aspect, near, far):
    t = math.tan(math.radians(fov) / 2)
    cs = []
    for d in (near, far):
        for sy in (-1, 1):
            for sx in (-1, 1):
                cs.append(eye + f * d + r * (sx * d * t * aspect) + u * (sy * d * t))
    return np.array(cs)  # index = (far*4 + sy_idx*2 + sx_idx)


def planes_from_corners(cs, eye):
    """six inward planes (n, d) with n.x + d >= 0, computed relative to the eye (eye = origin)."""
    c = cs - eye
    idx = {
        'left': (0, 2, 4, 6), 'right': (1, 3, 5, 7), 'bottom': (0, 1, 4, 5),
        'top': (2, 3, 6, 7), 'near': (0, 1, 2, 3), 'far': (4, 5, 6, 7),
    }
    centre = c.mean(axis=0)
    out = []
    for name in ('left', 'right', 'bottom', 'top', 'near', 'far'):
        a, b, cc, d = (c[i] for i in idx[name])
        n = np.cross(b - a, cc - a)
        if np.linalg.norm(n) < 1e-12:
            n = np.cross(d - a, cc - a)
        n = n / np.linalg.norm(n)
        dd = -n @ a
        if n @ centre + dd < 0:
            n, dd = -n, -dd
        out.append((n, dd))
    return out


def sat_intersect(box_c, fr_c, f_edges):
    """exact separating-axis test of an axis aligned box (8 corners) and the frustum (8 corners)."""
    axes = [np.array([1.0, 0, 0]), np.array([0, 1.0, 0]), np.array([0, 0, 1.0])]
    # frustum face normals: from the frustum corners
    for idx in ((0, 2, 4), (1, 3, 5), (0, 1, 4), (2, 3, 6), (0, 1, 2), (4, 5, 6)):
        a, b, c = (fr_c[i] for i in idx)
        n = np.cross(b - a, c - a)
        l = np.linalg.norm(n)
        if l > 1e-12:
            axes.append(n / l)
    for e in f_edges:
        for k in range(3):
            ax = np.cross(np.eye(3)[k], e)
            l = np.linalg.norm(ax)
            if l > 1e-9:
                axes.append(ax / l)
    for ax in axes:
        pa = box_c @ ax
        pb = fr_c @ ax
        if pa.max() < pb.min() - 1e-9 or pb.max() < pa.min() - 1e-9:
            return False
    return True


def check(lines, stats):
    errs = []

    def err(msg):
        if len(errs) < 20:
            errs.append(msg)

    cam = None
    for ln, line in enumerate(lines):
        t = line.split()
        if not t:
            continue
        k = t[0]
        if k == 'C':
            v = t[1:]
            x, y, z, yaw, pitch, fov, near, far = (fl(s) for s in v[:8])
            w, h, zo = int(v[8]), int(v[9]), int(v[10])
            fwd = np.array([fl(s) for s in v[11:14]])
            rgt = np.array([fl(s) for s in v[14:17]])
            upv = np.array([fl(s) for s in v[17:20]])
            view = mat(v[20:36])
            proj = mat(v[36:52])
            inv_vp = mat(v[52:68])
            planes = np.array([fl(s) for s in v[68:92]]).reshape(6, 4)
            eye = np.array([x, y, z])
            stats['C'] += 1
            if not (-180.0 <= yaw < 180.0) or not (-90.0 <= pitch <= 90.0):
                err('line %d: angles not normalised yaw=%r pitch=%r' % (ln, yaw, pitch))
            f, r, u = basis(yaw, pitch)
            if not (close(fwd, f, 1e-9) and close(rgt, r, 1e-9) and close(upv, u, 1e-9)):
                err('line %d: basis differs' % ln)
            aspect = w / h
            rot = np.array([r, u, -f])
            vexp = np.eye(4)
            vexp[:3, :3] = rot
            vexp[:3, 3] = -rot @ eye
            if not close(view, vexp, 1e-6 * (1 + abs(eye).max()) * 1e-3, 1e-9):
                err('line %d: view matrix differs' % ln)
            tf = 1.0 / math.tan(math.radians(fov) / 2)
            pexp = np.zeros((4, 4))
            pexp[0, 0] = tf / aspect
            pexp[1, 1] = tf
            pexp[3, 2] = -1
            if zo:
                pexp[2, 2] = far / (near - far)
                pexp[2, 3] = far * near / (near - far)
            else:
                pexp[2, 2] = (far + near) / (near - far)
                pexp[2, 3] = 2 * far * near / (near - far)
            if not close(proj, pexp, 1e-9, 1e-9):
                err('line %d: projection differs' % ln)
            cs = frustum_corners(eye, f, r, u, fov, aspect, near, far)
            # inverse: NDC cube corners -> frustum corners
            for i in range(8):
                sx = 1 if i & 1 else -1
                sy = 1 if i & 2 else -1
                zc = (-1 if not zo else 0) if i < 4 else 1
                hp = inv_vp @ np.array([sx, sy, zc, 1.0])
                pt = hp[:3] / hp[3]
                if not close(pt, cs[i], 5e-3 + 1e-9 * abs(eye).max()):
                    err('line %d: inv_vp corner %d differs' % (ln, i))
                    break
            csr = frustum_corners(np.zeros(3), f, r, u, fov, aspect, near, far)
            pl = planes_from_corners(csr, np.zeros(3))
            for i in range(6):
                n, d0 = pl[i]
                a = planes[i]
                # library planes are in world space: d_world = d_rel - n.eye
                d_world = d0 - n @ eye
                if not (close(a[:3], n, 1e-9) and abs(a[3] - d_world) < 1e-5 * (1 + abs(eye).max()) * 1e-3):
                    err('line %d: frustum plane %d differs (%r vs %r)' % (ln, i, a, (n, d_world)))
                    break
            f_edges = [r, u, csr[4] - csr[0], csr[5] - csr[1], csr[6] - csr[2], csr[7] - csr[3]]
            f_edges = [e / np.linalg.norm(e) for e in f_edges]
            cam = dict(eye=eye, f=f, r=r, u=u, w=w, h=h, near=near, far=far, fov=fov, zo=zo,
                       vp=pexp @ vexp, cs_rel=csr, pl=pl, f_edges=f_edges, aspect=aspect)
        elif k == 'R':
            stats['R'] += 1
            qx, qy = fl(t[1]), fl(t[2])
            org = np.array([fl(s) for s in t[3:6]])
            dr = np.array([fl(s) for s in t[6:9]])
            c = cam
            if not close(org, c['eye'], 1e-12):
                err('line %d: ray origin differs' % ln)
            ndc_x = 2 * qx / c['w'] - 1
            ndc_y = 1 - 2 * qy / c['h']
            inv = np.linalg.inv(c['vp'])
            zn = 0.0 if c['zo'] else -1.0
            a = inv @ np.array([ndc_x, ndc_y, zn, 1.0])
            b = inv @ np.array([ndc_x, ndc_y, 1.0, 1.0])
            dexp = b[:3] / b[3] - a[:3] / a[3]
            dexp /= np.linalg.norm(dexp)
            if not close(dr, dexp, 1e-6):
                err('line %d: pick ray differs' % ln)
        elif k == 'P':
            stats['P'] += 1
            pt = np.array([fl(s) for s in t[1:4]])
            ok = int(t[4])
            px, py, dep = fl(t[5]), fl(t[6]), fl(t[7])
            c = cam
            vz = (pt - c['eye']) @ c['f']
            if abs(vz - c['near']) > 1e-7:
                if (vz > c['near']) != (ok == 1):
                    err('line %d: projection ok flag %d but depth %r (near %r)' % (ln, ok, vz, c['near']))
            if ok == 1:
                hp = c['vp'] @ np.append(pt - c['eye'] * 0, 1.0)
                # vp contains the eye translation, so use world coordinates directly
                ndc = hp[:3] / hp[3]
                ex = (ndc[0] + 1) / 2 * c['w']
                ey = (1 - ndc[1]) / 2 * c['h']
                tol = 1e-6 * (1 + abs(ex) + abs(ey)) * 10
                if abs(px - ex) > tol or abs(py - ey) > tol or abs(dep - vz) > 1e-6:
                    err('line %d: projection differs (%r %r %r vs %r %r %r)' % (ln, px, py, dep, ex, ey, vz))
        elif k == 'B' or k == 'S':
            stats[k] += 1
            if k == 'B':
                lo = np.array([fl(s) for s in t[1:4]])
                hi = np.array([fl(s) for s in t[4:7]])
                res = int(t[7])
            else:
                cx, sy, cz, miny, res = (int(s) for s in t[1:6])
                lo = np.array([cx * 16.0, miny + sy * 16.0, cz * 16.0])
                hi = lo + 16.0
            c = cam
            lo = lo - c['eye']
            hi = hi - c['eye']
            corners = np.array([[lo[0] if i & 1 == 0 else hi[0], lo[1] if i & 2 == 0 else hi[1],
                                 lo[2] if i & 4 == 0 else hi[2]] for i in range(8)])
            margins = np.array([[n @ p + d for (n, d) in c['pl']] for p in corners])
            all_in = bool((margins > 1e-7).all())
            any_close = bool((np.abs(margins) < 1e-6).any())
            inter = sat_intersect(corners, c['cs_rel'], c['f_edges'])
            if not any_close:
                if inter and res == 0:
                    err('line %d: %s reported OUTSIDE but intersects the frustum' % (ln, k))
                if all_in and res != 2:
                    err('line %d: %s fully inside but reported %d' % (ln, k, res))
                if res == 2 and not all_in:
                    err('line %d: %s reported INSIDE but a corner is outside' % (ln, k))
                if not inter and res != 0:
                    stats['false_pos'] += 1
                if res == 0:
                    stats['out'] += 1
                elif res == 2:
                    stats['in'] += 1
                else:
                    stats['mid'] += 1
    return errs


class Stats(dict):
    def __missing__(self, key):
        return 0


def main():
    lines = open(sys.argv[1]).read().split('\n')
    stats = Stats()
    errs = check(lines, stats)
    if errs:
        for e in errs:
            print('  FAIL ', e)
        return 1
    print('  cases: cameras=%d rays=%d projections=%d boxes=%d sections=%d  (outside=%d intersecting=%d inside=%d, conservative false positives=%d)'
          % (stats['C'], stats['R'], stats['P'], stats['B'], stats['S'], stats['out'], stats['mid'], stats['in'], stats['false_pos']))
    # counter-check: damaged dumps must be rejected
    bad = 0
    def mutate(kind):
        out = list(lines)
        for i, ln in enumerate(out):
            t = ln.split()
            if not t:
                continue
            if kind == 'view' and t[0] == 'C':
                t[33] = '%016x' % (int(t[33], 16) ^ 0x0000100000000000)
                out[i] = ' '.join(t)
                return out
            if kind == 'ray' and t[0] == 'R':
                t[6] = '%016x' % (int(t[6], 16) ^ 0x0000200000000000)
                out[i] = ' '.join(t)
                return out
            if kind == 'proj' and t[0] == 'P' and t[4] == '1':
                t[5] = '%016x' % (int(t[5], 16) ^ 0x0000400000000000)
                out[i] = ' '.join(t)
                return out
            if kind == 'sect_out' and t[0] == 'S' and t[5] != '0':
                t[5] = '0'
                out[i] = ' '.join(t)
                return out
            if kind == 'box_in' and t[0] == 'B' and t[7] == '1':
                t[7] = '2'
                out[i] = ' '.join(t)
                return out
            if kind == 'plane' and t[0] == 'C':
                t[69] = '%016x' % (int(t[69], 16) ^ 0x0000800000000000)
                out[i] = ' '.join(t)
                return out
        return None
    for kind in ('view', 'ray', 'proj', 'sect_out', 'box_in', 'plane'):
        m = mutate(kind)
        if m is None:
            continue
        if not check(m, Stats()):
            print('  FAIL  counter-check %s: a damaged dump was accepted' % kind)
            bad += 1
        else:
            print('  ok    counter-check %s: damaged dump rejected' % kind)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
