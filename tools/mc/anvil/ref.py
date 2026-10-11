#!/usr/bin/env python3
"""Independent reference for mc_anvil: its own reader and writer of the public .mca format
(struct + zlib/gzip), and the same scenario generator as prog.fi.

  ref.py expect <seed>        the lines `prog read` must print after `prog write <file> <seed>`
  ref.py read <file>          parse a region file strictly (sector rules!) and print the same lines
  ref.py write <file> <seed>  write the end state of the scenario with python's own allocator
"""
import gzip, struct, sys, zlib, random


def lcg(seed):
    x = seed
    while True:
        x = (x * 1103515245 + 12345) % 2147483648
        yield x >> 8


SIZES = [1, 100, 4091, 4092, 4096, 8183, 8187, 20000, 70000, 250000, 900000]


def scenario(seed):
    g = lcg(seed)
    state = {}  # slot -> (stamp, payload)
    for step in range(80):
        act = next(g) % 10
        x = next(g) % 6 - 3
        z = next(g) % 6 - 3
        slot = (x & 31) + (z & 31) * 32
        if act < 8:
            n = SIZES[next(g) % 11]
            next(g)  # the compression choice of the Firn side (the content does not depend on it)
            low = step % 3 == 0
            data = bytes((next(g) % 7) if low else (next(g) % 256) for _ in range(n))
            state[slot] = (1000 + step, data)
        else:
            state.pop(slot, None)
    return state


def fnv(data):
    h = 0xcbf29ce484222325
    for b in data:
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


def lines(state):
    return ["c %d stamp=%d len=%d fnv=%016x" % (s, st, len(d), fnv(d)) for s, (st, d) in sorted(state.items())]


def read_region(path):
    raw = open(path, "rb").read()
    assert len(raw) >= 8192 and len(raw) % 4096 == 0, "size %d" % len(raw)
    nsec = len(raw) // 4096
    used = [None] * nsec
    state = {}
    for s in range(1024):
        e = struct.unpack(">I", raw[s * 4:s * 4 + 4])[0]
        off, cnt = e >> 8, e & 255
        if off == 0 and cnt == 0:
            continue
        assert off >= 2 and cnt >= 1 and off + cnt <= nsec, "bad entry %d" % s
        for k in range(off, off + cnt):
            assert used[k] is None, "sector %d shared by %d and %d" % (k, used[k], s)
            used[k] = s
        ln, ct = struct.unpack(">IB", raw[off * 4096:off * 4096 + 5])
        assert 1 <= ln and ln + 4 <= cnt * 4096, "length %d in %d sectors" % (ln, cnt)
        body = raw[off * 4096 + 5:off * 4096 + 4 + ln]
        data = {1: gzip.decompress, 2: zlib.decompress, 3: lambda b: b}[ct](body)
        stamp = struct.unpack(">I", raw[4096 + s * 4:4096 + s * 4 + 4])[0]
        state[s] = (stamp, data)
    return state


def write_region(path, state, rng):
    out = bytearray(8192)
    sec = 2
    body = bytearray()
    for s, (stamp, data) in sorted(state.items()):
        ct = rng.choice([1, 2, 3])
        comp = {1: lambda d: gzip.compress(d), 2: lambda d: zlib.compress(d), 3: lambda d: d}[ct](data)
        chunk = struct.pack(">IB", len(comp) + 1, ct) + comp
        cnt = (len(chunk) + 4095) // 4096
        chunk += bytes(cnt * 4096 - len(chunk))
        struct.pack_into(">I", out, s * 4, (sec << 8) | cnt)
        struct.pack_into(">I", out, 4096 + s * 4, stamp)
        body += chunk
        sec += cnt
    open(path, "wb").write(bytes(out) + bytes(body))


def main():
    cmd = sys.argv[1]
    if cmd == "expect":
        print("\n".join(lines(scenario(int(sys.argv[2])))))
    elif cmd == "read":
        print("\n".join(lines(read_region(sys.argv[2]))))
    elif cmd == "write":
        write_region(sys.argv[2], scenario(int(sys.argv[3])), random.Random(int(sys.argv[3])))
    else:
        raise SystemExit(__doc__)


main()
