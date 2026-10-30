import gzip, os, random, struct, sys, zlib

M64 = (1 << 64) - 1
BASIS = 14695981039346656037
PRIME = 1099511628211


def fnv(data, h=BASIS):
    for b in data:
        h = ((h ^ b) * PRIME) & M64
    return h


class Reader:
    def __init__(self, data):
        self.d = data
        self.p = 0

    def take(self, n):
        if self.p + n > len(self.d):
            raise ValueError("underflow")
        v = self.d[self.p:self.p + n]
        self.p += n
        return v

    def u8(self): return self.take(1)[0]
    def i8(self): return struct.unpack(">b", self.take(1))[0]
    def i16(self): return struct.unpack(">h", self.take(2))[0]
    def u16(self): return struct.unpack(">H", self.take(2))[0]
    def i32(self): return struct.unpack(">i", self.take(4))[0]
    def i64(self): return struct.unpack(">q", self.take(8))[0]


def read_payload(r, tag):
    if tag == 1: return ("b", r.i8())
    if tag == 2: return ("s", r.i16())
    if tag == 3: return ("i", r.i32())
    if tag == 4: return ("l", r.i64())
    if tag == 5: return ("f", struct.unpack(">I", r.take(4))[0])
    if tag == 6: return ("d", struct.unpack(">Q", r.take(8))[0])
    if tag == 7: n = r.i32(); return ("B", r.take(n), n)
    if tag == 8: return ("S", r.take(r.u16()))
    if tag == 9:
        et = r.u8()
        n = r.i32()
        return ("L", et, [read_payload(r, et) for _ in range(n)])
    if tag == 10:
        items = []
        while True:
            t = r.u8()
            if t == 0:
                break
            name = r.take(r.u16())
            items.append((name, t, read_payload(r, t)))
        return ("C", items)
    if tag == 11: n = r.i32(); return ("I", r.take(4 * n), n)
    if tag == 12: n = r.i32(); return ("J", r.take(8 * n), n)
    raise ValueError("bad tag")


TAGOF = {"b": 1, "s": 2, "i": 3, "l": 4, "f": 5, "d": 6, "B": 7, "S": 8, "L": 9, "C": 10, "I": 11, "J": 12}


def parse_nbt(data):
    r = Reader(data)
    t = r.u8()
    if t != 10:
        raise ValueError("root not compound")
    r.take(r.u16())
    v = read_payload(r, 10)
    if r.p != len(data):
        raise ValueError("trailing")
    return v


def flatten(node, path, out):
    k = node[0]
    tag = TAGOF[k]
    if k in "bsil":
        val = str(node[1])
    elif k in "fd":
        val = str(node[1])
    elif k == "S":
        val = node[1].hex()
    elif k in "BIJ":
        val = "%d:%d" % (node[2], fnv(node[1]))
    elif k == "L":
        val = "L%d:%d" % (node[1], len(node[2]))
    else:
        val = "C"
    out.append(path + b"=" + str(tag).encode() + b":" + val.encode() + b"\n")
    if k == "C":
        for name, t, pl in node[1]:
            flatten(pl, path + b"/" + name, out)
    elif k == "L":
        for i, pl in enumerate(node[2]):
            flatten(pl, path + b"[%d]" % i, out)


def tree_digest(data):
    node = parse_nbt(data)
    out = []
    flatten(node, b"", out)
    h = BASIS
    for ln in out:
        h = fnv(ln, h)
    return len(out), h


def get(node, name):
    for n, t, pl in node[1]:
        if n == name:
            return pl
    return None


def lz4_block(src, olen):
    out = bytearray()
    i = 0
    while i < len(src):
        tok = src[i]; i += 1
        lit = tok >> 4
        if lit == 15:
            while True:
                b = src[i]; i += 1
                lit += b
                if b != 255: break
        out += src[i:i + lit]; i += lit
        if i >= len(src): break
        off = src[i] | (src[i + 1] << 8); i += 2
        ml = tok & 15
        if ml == 15:
            while True:
                b = src[i]; i += 1
                ml += b
                if b != 255: break
        ml += 4
        for _ in range(ml):
            out.append(out[-off])
    if len(out) != olen:
        raise ValueError("lz4 length")
    return bytes(out)


def lz4_stream(data):
    out = bytearray()
    p = 0
    while p < len(data):
        if data[p:p + 8] != b"LZ4Block":
            raise ValueError("lz4 magic")
        tok = data[p + 8]
        clen, olen, chk = struct.unpack("<III", data[p + 9:p + 21])
        p += 21
        if clen == 0 and olen == 0:
            break
        body = data[p:p + clen]; p += clen
        blk = body if tok & 0xF0 == 0x10 else lz4_block(body, olen)
        if xxh32(blk, 0x9747b28c) & 0x0FFFFFFF != chk:
            raise ValueError("lz4 checksum")
        out += blk
    return bytes(out)


def xxh32(data, seed):
    P1, P2, P3, P4, P5 = 2654435761, 2246822519, 3266489917, 668265263, 374761393
    M = 0xFFFFFFFF
    rotl = lambda x, r: ((x << r) | (x >> (32 - r))) & M
    n = len(data)
    i = 0
    if n >= 16:
        v = [(seed + P1 + P2) & M, (seed + P2) & M, seed & M, (seed - P1) & M]
        while i <= n - 16:
            for k in range(4):
                w = struct.unpack("<I", data[i:i + 4])[0]; i += 4
                v[k] = (rotl((v[k] + w * P2) & M, 13) * P1) & M
        h = (rotl(v[0], 1) + rotl(v[1], 7) + rotl(v[2], 12) + rotl(v[3], 18)) & M
    else:
        h = (seed + P5) & M
    h = (h + n) & M
    while i + 4 <= n:
        h = (rotl((h + struct.unpack("<I", data[i:i + 4])[0] * P3) & M, 17) * P4) & M; i += 4
    while i < n:
        h = (rotl((h + data[i] * P5) & M, 11) * P1) & M; i += 1
    h ^= h >> 15; h = (h * P2) & M
    h ^= h >> 13; h = (h * P3) & M
    h ^= h >> 16
    return h


def decompress(ct, data):
    if ct == 1: return gzip.decompress(data)
    if ct == 2: return zlib.decompress(data)
    if ct == 3: return data
    if ct == 4: return lz4_stream(data)
    raise ValueError("compression %d" % ct)


def region_chunks(path):
    raw = open(path, "rb").read()
    d = os.path.dirname(path)
    if not raw:
        return
    for s in range(1024):
        e = struct.unpack(">I", raw[s * 4:s * 4 + 4])[0]
        if not e:
            continue
        off, cnt = e >> 8, e & 255
        ln = struct.unpack(">I", raw[off * 4096:off * 4096 + 4])[0]
        ct = raw[off * 4096 + 4]
        body = raw[off * 4096 + 5:off * 4096 + 4 + ln]
        if ct & 128:
            parts = os.path.basename(path).split(".")
            cx, cz = int(parts[1]) * 32 + s % 32, int(parts[2]) * 32 + s // 32
            body = open(os.path.join(d, "c.%d.%d.mcc" % (cx, cz)), "rb").read()
            ct &= 127
        yield s, decompress(ct, body)


def list_files(root):
    res = []
    for dp, dn, fn in os.walk(root):
        for f in fn:
            res.append(os.path.relpath(os.path.join(dp, f), root))
    return res


def read_nbt_file(path):
    raw = open(path, "rb").read()
    return gzip.decompress(raw) if raw[:2] == b"\x1f\x8b" else raw


def cmd_tree(root):
    lines = []
    for rel in list_files(root):
        full = os.path.join(root, rel)
        if rel.endswith(".mca"):
            for s, data in region_chunks(full):
                n, h = tree_digest(data)
                lines.append("C %s %d %d %d" % (rel, s, n, h))
        elif rel.endswith(".dat") or rel.endswith(".dat_old"):
            n, h = tree_digest(read_nbt_file(full))
            lines.append("F %s -1 %d %d" % (rel, n, h))
    print("\n".join(sorted(lines)))


def entry_str(e):
    if e[0] == "S":
        return e[1]
    name = get(e, b"Name")[1]
    props = get(e, b"Properties")
    s = name
    if props is not None:
        s += b"[" + b",".join(n + b"=" + pl[1] for n, t, pl in props[1]) + b"]"
    return s


def unpack(longs, nlongs, bits, count):
    vpl = 64 // bits
    vals = struct.unpack(">%dQ" % nlongs, longs)
    mask = (1 << bits) - 1
    return [(vals[i // vpl] >> ((i % vpl) * bits)) & mask for i in range(count)]


def decode_section_part(part, count, min_bits, name_key="palette"):
    pal = get(part, name_key.encode())[2]
    data = get(part, b"data")
    np_ = len(pal)
    if np_ == 1 or data is None:
        return pal, [0] * count
    bits = max(min_bits, (np_ - 1).bit_length())
    return pal, unpack(data[1], data[2], bits, count)


def seq_hash(pal, idx):
    eh = [fnv(entry_str(e)) for e in pal]
    h = BASIS
    for i in idx:
        h = ((h ^ eh[i]) * PRIME) & M64
    return h


def light_sum(sec, name):
    a = get(sec, name)
    if a is None:
        return 0
    return sum((b & 15) + (b >> 4) for b in a[1])


def heightmap_hash(root, name):
    hm = get(get(root, b"Heightmaps") or ("C", []), name)
    if hm is None:
        return 0
    n = hm[2]
    for bits in range(1, 33):
        if (256 + 64 // bits - 1) // (64 // bits) == n:
            break
    vals = unpack(hm[1], n, bits, 256)
    h = BASIS
    for v in vals:
        h = ((h ^ (v & 255)) * PRIME) & M64
        h = ((h ^ (v >> 8)) * PRIME) & M64
    return h


def chunk_lines(rel, s, data):
    root = parse_nbt(data)
    out = []
    dv = get(root, b"DataVersion")[1]
    xp, zp = get(root, b"xPos")[1], get(root, b"zPos")[1]
    st = get(root, b"Status") or get(root, b"status")
    be = get(root, b"block_entities")
    secs = get(root, b"sections")[2]
    out.append("K %s %d dv=%d pos=%d,%d status=%s nsec=%d be=%d" % (rel, s, dv, xp, zp, st[1].decode() if st else "", len(secs), len(be[2]) if be else 0))
    for sec in secs:
        y = get(sec, b"Y")[1]
        bpal, bidx = decode_section_part(get(sec, b"block_states"), 4096, 4)
        ipal, iidx = decode_section_part(get(sec, b"biomes"), 64, 1)
        out.append("S %s %d y=%d bp=%d ip=%d blk=%d bio=%d sky=%d bl=%d rep=11" % (
            rel, s, y, len(bpal), len(ipal), seq_hash(bpal, bidx), seq_hash(ipal, iidx),
            light_sum(sec, b"SkyLight"), light_sum(sec, b"BlockLight")))
    for nm in ("MOTION_BLOCKING", "WORLD_SURFACE", "OCEAN_FLOOR", "MOTION_BLOCKING_NO_LEAVES"):
        out.append("H %s %d %s %d" % (rel, s, nm, heightmap_hash(root, nm.encode())))
    return out


def cmd_blocks(root):
    lines = []
    for rel in list_files(root):
        if rel.endswith(".mca") and os.path.basename(os.path.dirname(rel)) == "region":
            for s, data in region_chunks(os.path.join(root, rel)):
                lines += chunk_lines(rel, s, data)
    print("\n".join(sorted(lines)))


def block_name_at(root, x, y, z):
    secs = get(root, b"sections")[2]
    for sec in secs:
        if get(sec, b"Y")[1] == y >> 4:
            pal, idx = decode_section_part(get(sec, b"block_states"), 4096, 4)
            e = pal[idx[((y & 15) << 8) | ((z & 15) << 4) | (x & 15)]]
            return get(e, b"Name")[1].decode()
    return None


def cmd_picks(regiondir, count, seed, lo, hi):
    rnd = random.Random(int(seed))
    files = sorted(f for f in os.listdir(regiondir) if f.endswith(".mca"))
    cache = {}
    picks = []
    tries = 0
    while len(picks) < int(count) and tries < int(count) * 50:
        tries += 1
        x = rnd.randint(int(lo), int(hi)); z = rnd.randint(int(lo), int(hi)); y = rnd.randint(-64, 110)
        cx, cz = x >> 4, z >> 4
        rf = "r.%d.%d.mca" % (cx >> 5, cz >> 5)
        if rf not in files:
            continue
        key = (cx, cz)
        if key not in cache:
            cache[key] = None
            for s, data in region_chunks(os.path.join(regiondir, rf)):
                if s == (cx & 31) + (cz & 31) * 32:
                    cache[key] = parse_nbt(data)
        root = cache[key]
        if root is None or get(root, b"Status") is None and get(root, b"status") is None:
            continue
        st = (get(root, b"Status") or get(root, b"status"))[1]
        if not st.endswith(b"full"):
            continue
        nm = block_name_at(root, x, y, z)
        if nm:
            picks.append("%d %d %d %s" % (x, y, z, nm))
    print("\n".join(picks))


if __name__ == "__main__":
    c = sys.argv[1]
    if c == "tree": cmd_tree(sys.argv[2])
    elif c == "blocks": cmd_blocks(sys.argv[2])
    elif c == "picks": cmd_picks(*sys.argv[2:7])
