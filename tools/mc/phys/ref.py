import math, struct, sys

def f32(x):
    return struct.unpack('f', struct.pack('f', x))[0]

SIN = lambda i: f32(math.sin(i * math.pi * 2.0 / 65536.0))

def mth_sin(x):
    return SIN(int(f32(x * f32(10430.378))) & 65535)

def mth_cos(x):
    return SIN(int(f32(f32(x * f32(10430.378)) + f32(16384.0))) & 65535)

DEG = f32(0.017453292)

def move_relative(v, strafe, fwd, speed, yaw):
    d0 = strafe * strafe + fwd * fwd
    if d0 < 1e-7:
        return
    ix, iz = strafe, fwd
    if d0 > 1.0:
        d = math.sqrt(d0)
        ix, iz = strafe / d, fwd / d
    ax, az = ix * speed, iz * speed
    f = f32(f32(yaw) * DEG)
    sn, cs = mth_sin(f), mth_cos(f)
    v[0] += ax * cs - az * sn
    v[2] += az * cs + ax * sn

def sim(ticks, floor_friction=0.6, yaw=0.0, sprint_jump_bonus=True):
    s = dict(x=0.0, y=0.0, z=0.0, vx=0.0, vy=f32(0.0 - 0.08) * 0 + (0.0 - 0.08) * f32(0.98), vz=0.0, ground=True, delay=0)
    out = []
    for (fwd_k, left_k, jump_k, sneak_k, sprint_k) in ticks:
        v = [s['vx'], s['vy'], s['vz']]
        v = [0.0 if abs(c) < 0.003 else c for c in v]
        slow = sneak_k
        def axis(a, b):
            return f32(0.0) if a == b else (f32(1.0) if a else f32(-1.0))
        zz = axis(fwd_k > 0, fwd_k < 0)
        xx = axis(left_k > 0, left_k < 0)
        if slow:
            zz, xx = f32(zz * f32(0.3)), f32(xx * f32(0.3))
        zz, xx = f32(zz * f32(0.98)), f32(xx * f32(0.98))
        sprinting = sprint_k and fwd_k > 0 and not sneak_k
        if s['delay'] > 0:
            s['delay'] -= 1
        if not jump_k:
            s['delay'] = 0
        if jump_k and s['ground'] and s['delay'] == 0:
            v[1] = f32(f32(0.42) * f32(1.0))
            if sprinting:
                f1 = f32(f32(yaw) * DEG)
                v[0] += f32(f32(-mth_sin(f1)) * f32(0.2))
                v[2] += f32(mth_cos(f1) * f32(0.2))
            s['delay'] = 10
        f2 = f32(floor_friction)
        if s['ground']:
            attr = 0.10000000149011612
            if sprinting:
                attr = attr * (1.0 + 0.30000001192092896)
            attr = f32(attr)
            c3 = f32(f32(f2 * f2) * f2)
            speed = f32(attr * f32(f32(0.21600002) / c3))
        else:
            speed = f32(0.025999999) if sprinting else f32(0.02)
        move_relative(v, xx, zz, speed, yaw)
        ny = s['y'] + v[1]
        ground = False
        if ny <= 0.0:
            if v[1] < 0.0:
                ground = True
            ny = 0.0
            v[1] = 0.0
        s['x'] += v[0]
        s['z'] += v[2]
        s['y'] = ny
        f3 = f32(f2 * f32(0.91)) if s['ground'] else f32(0.91)
        was = s['ground']
        s['ground'] = ground
        s['vx'] = v[0] * f3
        s['vz'] = v[2] * f3
        s['vy'] = (v[1] - 0.08) * f32(0.98)
        out.append((s['x'], s['y'], s['z']))
    return out

def emit(name, rows):
    print("// " + name)
    print(", ".join("%.9f,%.9f,%.9f" % r for r in rows))

if __name__ == "__main__":
    W = (1, 0, 0, 0, 0)
    S = (1, 0, 0, 0, 1)
    N = (0, 0, 0, 0, 0)
    J = (0, 0, 1, 0, 0)
    print(sim([J] + [N] * 13)[:14])
