# tools/vorbis/floor0_gen.py -- writes SYNTHETIC Ogg Vorbis files that use floor type 0 (LSP).
# No common encoder produces floor 0, so we write the bitstream ourselves (random but valid floor and
# residue data); libvorbis (via soundfile) is the reference decoder for them.
# usage: floor0_gen.py <outdir>   -> floor0_mono_o8.ogg, floor0_stereo_o7.ogg
import struct, sys, os, math
import numpy as np


class Bits:
    def __init__(self):
        self.b = bytearray(); self.n = 0

    def w(self, v, bits):
        for i in range(bits):
            if self.n % 8 == 0:
                self.b.append(0)
            if (v >> i) & 1:
                self.b[-1] |= 1 << (self.n % 8)
            self.n += 1

    def bytes(self):
        return bytes(self.b)


def f32pack(x):                              # Vorbis "float32": 21 bit mantissa, 10 bit exponent, sign
    if x == 0:
        return 0
    s = 0
    if x < 0:
        s = 1; x = -x
    e = math.frexp(x)[1]                     # x = m * 2^e, 0.5 <= m < 1
    mant = int(round(x * 2.0 ** (21 - e)))   # in [2^20, 2^21)
    if mant >= 1 << 21:
        mant >>= 1; e += 1
    return (s << 31) | (((e - 21 + 788) & 0x3FF) << 21) | mant


def codeword(b, code, length):               # codewords are packed MSB first
    for i in range(length - 1, -1, -1):
        b.w((code >> i) & 1, 1)


def ilog(x):
    n = 0
    while x > 0:
        n += 1; x >>= 1
    return n


_CRC = []
for _i in range(256):
    _r = _i << 24
    for _ in range(8):
        _r = ((_r << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if _r & 0x80000000 else (_r << 1) & 0xFFFFFFFF
    _CRC.append(_r)


def crc32_ogg(data):
    c = 0
    for x in data:
        c = ((c << 8) & 0xFFFFFFFF) ^ _CRC[((c >> 24) & 0xFF) ^ x]
    return c


def page(serial, seq, granule, flags, lac, body):
    seg = bytes(lac)
    pg = bytearray(b'OggS' + struct.pack('<BBqIII', 0, flags, granule, serial, seq, 0) + bytes([len(seg)]) + seg + body)
    struct.pack_into('<I', pg, 22, crc32_ogg(pg))
    return bytes(pg)


def lacing(n):
    return [255] * (n // 255) + [n % 255]


def book_hdr(b, dims, entries, length, ltype, minv=0.0, delta=0.0, vbits=0, mults=None):
    b.w(0x564342, 24); b.w(dims, 16); b.w(entries, 24)
    b.w(0, 1); b.w(0, 1)                     # not ordered, not sparse
    for _ in range(entries):
        b.w(length - 1, 5)
    b.w(ltype, 4)
    if ltype:
        b.w(f32pack(minv), 32); b.w(f32pack(delta), 32); b.w(vbits - 1, 4); b.w(0, 1)
        for m in mults:
            b.w(m, vbits)


def well_conditioned(coefs, bark, smin=0.1):
    # sqrt(p + q) of the LSP filter must stay away from 0, else the floor curve overflows
    w = np.cos(np.pi * np.arange(bark) / bark)
    cs = np.cos(coefs)
    if len(coefs) % 2:
        p = 1 - w * w; q = np.full(bark, 0.25)
        for j in range((len(coefs) - 1) // 2):
            p = p * 4 * (cs[2 * j + 1] - w) ** 2
        for j in range((len(coefs) + 1) // 2):
            q = q * 4 * (cs[2 * j] - w) ** 2
    else:
        p = (1 - w) / 2; q = (1 + w) / 2
        for j in range(len(coefs) // 2):
            p = p * 4 * (cs[2 * j + 1] - w) ** 2
            q = q * 4 * (cs[2 * j] - w) ** 2
    return float(np.sqrt(p + q).min()) >= smin


def headers(ch, order, rate, bs, aoff, abits, bark, rtype):
    idp = Bits(); idp.w(1, 8)
    for c in b'vorbis': idp.w(c, 8)
    idp.w(0, 32); idp.w(ch, 8); idp.w(rate, 32); idp.w(0, 32); idp.w(64000, 32); idp.w(0, 32)
    idp.w(ilog(bs[0]) - 1, 4); idp.w(ilog(bs[1]) - 1, 4); idp.w(1, 1)
    cm = Bits(); cm.w(3, 8)
    for c in b'vorbis': cm.w(c, 8)
    v = b'floor0_gen'; cm.w(len(v), 32)
    for c in v: cm.w(c, 8)
    cm.w(0, 32); cm.w(1, 1)
    st = Bits(); st.w(5, 8)
    for c in b'vorbis': st.w(c, 8)
    st.w(3 - 1, 8)
    book_hdr(st, 4, 4096, 12, 1, 0.3, 0.12, 3, list(range(8)))   # book 0: floor, 8^4 lattice 0.30 .. 1.14
    book_hdr(st, 1, 2, 1, 0)                                      # book 1: residue class book
    book_hdr(st, 2, 16, 4, 1, -0.3, 0.2, 2, [0, 1, 2, 3])         # book 2: residue VQ
    st.w(0, 6); st.w(0, 16)                                       # one (unused) time transform
    st.w(0, 6)                                                    # 1 floor
    st.w(0, 16); st.w(order, 8); st.w(rate, 16); st.w(bark, 16); st.w(abits, 6); st.w(aoff, 8)
    st.w(0, 4); st.w(0, 8)                                        # 1 book: book 0
    st.w(0, 6)                                                    # 1 residue
    st.w(rtype, 16); st.w(0, 24); st.w(2048, 24); st.w(16 - 1, 24); st.w(2 - 1, 6); st.w(1, 8)
    st.w(0, 3); st.w(0, 1)                                        # class 0: no book
    st.w(1, 3); st.w(0, 1)                                        # class 1: pass 0 only
    st.w(2, 8)                                                    # class 1 pass 0 -> book 2
    st.w(0, 6)                                                    # 1 mapping
    st.w(0, 16); st.w(0, 1)                                       # type 0, one submap
    if ch == 2:
        st.w(1, 1); st.w(1 - 1, 8); st.w(0, 1); st.w(1, 1)        # one coupling step: mag 0, ang 1
    else:
        st.w(0, 1)
    st.w(0, 2); st.w(0, 8); st.w(0, 8); st.w(0, 8)                # reserved, (time), floor 0, residue 0
    st.w(2 - 1, 6)
    for blockflag in (0, 1):
        st.w(blockflag, 1); st.w(0, 16); st.w(0, 16); st.w(0, 8)
    st.w(1, 1)
    return idp.bytes(), cm.bytes(), st.bytes()


def write_ogg(path, serial, ch, order, seed, nblocks, bs=(256, 2048), rate=44100, aoff=1, abits=6,
              res_on=True, p_off=0.12, bark=256):
    rng = np.random.default_rng(seed)
    rtype = 2 if ch == 2 else 1
    idp, cm, st = headers(ch, order, rate, bs, aoff, abits, bark, rtype)
    flags_l = rng.integers(0, 2, size=nblocks)       # 1 = long block
    flags_l[0] = 1
    pk = []
    for i in range(nblocks):
        long_ = int(flags_l[i]); n2 = bs[long_] // 2
        b = Bits(); b.w(0, 1); b.w(long_, 1)
        if long_:
            b.w(int(flags_l[i - 1]) if i > 0 else 0, 1)
            b.w(int(flags_l[i + 1]) if i + 1 < nblocks else 0, 1)
        used = False
        for c in range(ch):
            if rng.random() < p_off:
                b.w(0, abits)                        # amplitude 0 = floor unused
                continue
            used = True
            b.w(int(rng.integers(1, 1 << abits)), abits); b.w(0, 1)   # amplitude, book number (ilog(1) = 1 bit)
            # vector elements are NOT cumulative: element = lattice value + last element of the previous vector
            while True:
                cw = []; coefs = []; last = 0.0
                for _ in range((order + 3) // 4):
                    dg = sorted(rng.choice(8, size=4, replace=False).tolist())
                    cw.append(sum(d << (3 * k) for k, d in enumerate(dg)))
                    vals = [0.3 + 0.12 * d + last for d in dg]
                    coefs += vals; last = vals[-1]
                if well_conditioned(coefs[:order], bark):
                    break
            for e in cw:
                codeword(b, e, 12)
        if used:
            parts = min(2048, n2 * (2 if rtype == 2 else 1)) // 16
            for p in range(parts):
                cls = int(rng.integers(0, 2)) if res_on else 0
                codeword(b, cls, 1)
                if cls:
                    for _ in range(8):               # 16 / dims 2 vectors
                        codeword(b, int(rng.integers(0, 16)), 4)
        pk.append(b.bytes())
    out = bytearray()
    out += page(serial, 0, 0, 2, lacing(len(idp)), idp)
    out += page(serial, 1, 0, 0, lacing(len(cm)) + lacing(len(st)), cm + st)
    seq = 2; cum = 0; i = 0; prev_n = 0
    while i < nblocks:
        grp = pk[i:i + 12]
        lac = []; body = b''
        for k, p in enumerate(grp):
            n = bs[int(flags_l[i + k])]
            if prev_n:
                cum += prev_n // 4 + n // 4
            prev_n = n
            lac += lacing(len(p)); body += p
        i += len(grp)
        out += page(serial, seq, cum, 4 if i >= nblocks else 0, lac, body); seq += 1
    open(path, 'wb').write(bytes(out))


if __name__ == '__main__':
    d = sys.argv[1]
    os.makedirs(d, exist_ok=True)
    write_ogg(os.path.join(d, 'floor0_mono_o8.ogg'), 0x1111, 1, 8, 1, 60)
    write_ogg(os.path.join(d, 'floor0_stereo_o7.ogg'), 0x2222, 2, 7, 2, 60, rate=22050)
