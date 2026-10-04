#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/gen_fixtures.py -- the fixtures of the compression tests
(tests/data/compress/), made by programs nobody here wrote: python's lz4, lzma,
bz2, brotli, zlib and gzip modules, and the zstd and xz command lines.

    gen_fixtures.py [outdir]        default: tests/data/compress

Everything is deterministic (fixed seed, fixed mtime), so the files do not
change from run to run. `plain.bin` is the text every `plain.*` fixture decodes
to; the tests compile the pairs in with __include_str and compare octet for octet.
Run it with a python that has lz4, brotli and zstandard (a venv is fine);
the zstd and xz commands must be on the PATH.
"""
import os, sys, struct, random, subprocess, zlib, gzip, io, bz2, lzma

root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "tests", "data", "compress")
os.makedirs(out, exist_ok=True)
rnd = random.Random(2026)


def w(name, data):
    with open(os.path.join(out, name), "wb") as f:
        f.write(data)


# ----------------------------------------------------------------- the plain text
spec = open(os.path.join(root, "SPEC.md"), "rb").read()
src = open(os.path.join(root, "lib", "std", "deflate.fi"), "rb").read()
binary = b"".join(struct.pack("<IHhf", i * 7, i % 65536, -i % 300, i / 3.0) for i in range(300))
plain = spec[2000:8000] + src[1000:4000] + binary[:2400] + bytes(600) + (b"abc" * 200) + b"\x00\xff" * 50
w("plain.bin", plain)
sys.stderr.write("plain.bin: %d octets\n" % len(plain))

# A second, tiny one for the dictionary tests (JSON-ish records)
recs = []
for i in range(300):
    u = "user%d" % rnd.randrange(10000)
    recs.append(('{"id": %d, "name": "%s", "email": "%s@example.com", "roles": ["admin", "editor"], '
                 '"active": %s, "score": %d.%d}' % (i, u, u, rnd.choice(["true", "false"]), rnd.randrange(100),
                                                    rnd.randrange(100))).encode())
w("record.bin", recs[7])
w("record2.bin", recs[8])

# ---------------------------------------------------------------------------- lz4
import lz4.frame as lf
w("plain.lz4", lf.compress(plain))
w("plain_cchk.lz4", lf.compress(plain, content_checksum=True))
w("plain_hc_linked.lz4", lf.compress(plain, compression_level=9, block_linked=True, block_checksum=True))
w("plain_nosize_64k.lz4", lf.compress(plain, block_size=lf.BLOCKSIZE_MAX64KB, block_linked=False, store_size=False,
                                      content_checksum=False))
w("plain_multi.lz4", lf.compress(plain[:7000]) + b"\x50\x2a\x4d\x18\x05\x00\x00\x00skip!" + lf.compress(plain[7000:]))
w("empty.lz4", lf.compress(b""))
import lz4.block as lb
w("record2_dict.lz4block", lb.compress(recs[8], mode="high_compression", compression=9, dict=recs[7], store_size=False))

# ---------------------------------------------------------------------------- zstd
def zstd(args, data):
    return subprocess.run(["zstd", "-q", "-c"] + args, input=data, capture_output=True, check=True).stdout


w("plain.zst", zstd(["-3"], plain))
w("plain_l1.zst", zstd(["-1"], plain))
w("plain_l19.zst", zstd(["-19"], plain))
w("plain_nochk.zst", zstd(["--no-check"], plain))
w("plain_multi.zst", zstd(["-3"], plain[:6000]) + b"\x55\x2a\x4d\x18\x04\x00\x00\x00skip" + zstd(["-3"], plain[6000:]))
w("empty.zst", zstd([], b""))
w("zeros.zst", zstd(["-3"], bytes(70000)))
rb = bytes(rnd.getrandbits(8) for _ in range(3000))
w("random.bin", rb)
w("random.zst", zstd(["-3"], rb))
# a dictionary trained on records
sdir = os.path.join("/tmp", "firn-compress-fixtures-%d" % os.getpid())
os.makedirs(sdir, exist_ok=True)
paths = []
for i, r in enumerate(recs):
    p = os.path.join(sdir, "r%03d" % i)
    open(p, "wb").write(r)
    paths.append(p)
dpath = os.path.join(sdir, "dict")
subprocess.run(["zstd", "--train", "-q", "-o", dpath, "--maxdict=4096"] + paths, check=True, capture_output=True)
dictb = open(dpath, "rb").read()
w("dict.zdict", dictb)
w("record.zst", zstd(["-3"], recs[7]))
w("record_dict.zst", zstd(["-3", "-D", dpath], recs[7]))
w("record2_dict.zst", zstd(["-19", "-D", dpath], recs[8]))
for p in paths + [dpath]:
    os.remove(p)
os.rmdir(sdir)

# ---------------------------------------------------------------------------- xz / lzma
w("plain.xz", lzma.compress(plain, preset=6))
w("plain_crc32.xz", lzma.compress(plain, check=lzma.CHECK_CRC32))
w("plain_sha256.xz", lzma.compress(plain, check=lzma.CHECK_SHA256))
w("plain_nocheck.xz", lzma.compress(plain, check=lzma.CHECK_NONE))
w("plain_delta.xz", lzma.compress(plain, format=lzma.FORMAT_XZ, filters=[
    {"id": lzma.FILTER_DELTA, "dist": 4}, {"id": lzma.FILTER_LZMA2, "preset": 3}]))
w("plain_x86.xz", lzma.compress(plain, format=lzma.FORMAT_XZ, filters=[
    {"id": lzma.FILTER_X86}, {"id": lzma.FILTER_LZMA2, "preset": 4}]))
w("plain_multi.xz", lzma.compress(plain[:5000]) + bytes(8) + lzma.compress(plain[5000:], check=lzma.CHECK_CRC32))
w("empty.xz", lzma.compress(b""))
w("plain.lzma", lzma.compress(plain, format=lzma.FORMAT_ALONE, preset=4))
w("zeros.xz", lzma.compress(bytes(1500000), preset=1))
# code-like data for the branch converters: random with call/branch patterns
mix = bytearray(rnd.getrandbits(8) for _ in range(5000))
for i in range(0, len(mix) - 16, 16):
    k = rnd.randrange(8)
    if k == 0:
        mix[i] = rnd.choice((0xE8, 0xE9)); mix[i + 4] = rnd.choice((0, 0xFF))
    elif k == 1:
        mix[i + 3] = 0xEB
    elif k == 2:
        mix[i + 1] = 0xF0 | (mix[i + 1] & 7); mix[i + 3] = 0xF8 | (mix[i + 3] & 7)
    elif k == 3:
        mix[i] = 0x48 | (mix[i] & 3); mix[i + 3] = (mix[i + 3] & 0xFC) | 1
    elif k == 4:
        mix[i] = 0x40; mix[i + 1] &= 0x3F
mix = bytes(mix)
w("mix.bin", mix)
for name, fid in (("x86", lzma.FILTER_X86), ("ppc", lzma.FILTER_POWERPC), ("ia64", lzma.FILTER_IA64),
                  ("arm", lzma.FILTER_ARM), ("armthumb", lzma.FILTER_ARMTHUMB), ("sparc", lzma.FILTER_SPARC)):
    w("mix_%s.xz" % name, lzma.compress(mix, format=lzma.FORMAT_XZ, filters=[{"id": fid}, {"id": lzma.FILTER_LZMA2, "preset": 1}]))
w("mix_arm64.xz", subprocess.run(["xz", "-c", "--arm64", "--lzma2=preset=1"], input=mix, capture_output=True, check=True).stdout)

# ---------------------------------------------------------------------------- bzip2
w("plain.bz2", bz2.compress(plain, 9))
w("plain_l1.bz2", bz2.compress(plain, 1))
w("plain_multi.bz2", bz2.compress(plain[:5000], 9) + bz2.compress(plain[5000:], 5))
w("empty.bz2", bz2.compress(b""))
w("runs.bz2", bz2.compress(b"a" * 100000 + b"xy" * 5000 + b"b" * 300, 9))


def bz_crc(data):
    crc = 0xFFFFFFFF
    tab = []
    for i in range(256):
        c = i << 24
        for _ in range(8):
            c = ((c << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if c & 0x80000000 else (c << 1) & 0xFFFFFFFF
        tab.append(c)
    for b in data:
        crc = ((crc << 8) & 0xFFFFFFFF) ^ tab[((crc >> 24) ^ b) & 0xFF]
    return crc ^ 0xFFFFFFFF


# A "randomised" block (bzip2 0.9.0, never written since): the BWT input is the
# text XOR the randomisation mask, the block header's randomised bit is set and
# the CRCs are those of the real text.
RNUMS = None
d = open(os.path.join(root, "lib", "compress", "bz2.fi"), "r").read()
i = d.index("static RNUMS")
i = d.index("= [", i) + 3
j = d.index("]", i)
RNUMS = [int(x) for x in d[i:j].replace("\n", " ").split(",") if x.strip()]
assert len(RNUMS) == 512, len(RNUMS)
text = bytes(rnd.choice(b"abcdefghijklmnopqrstuvwxyz ,.") for _ in range(900))
# no run of four equal octets (so that the initial run-length step is the identity)
t = bytearray(text)
for k in range(3, len(t)):
    if t[k] == t[k - 1] == t[k - 2] == t[k - 3]:
        t[k] = ord("Q")
text = bytes(t)
mask = bytearray(len(text))
togo, pos = 0, 0
for k in range(len(text)):
    if togo == 0:
        togo = RNUMS[pos]
        pos = (pos + 1) % 512
    togo -= 1
    mask[k] = 1 if togo == 1 else 0
q = bytes(a ^ b for a, b in zip(text, mask))
assert not any(q[k] == q[k - 1] == q[k - 2] == q[k - 3] for k in range(3, len(q)))
z = bytearray(bz2.compress(q, 9))


def getbit(buf, n):
    return (buf[n >> 3] >> (7 - (n & 7))) & 1


def setbit(buf, n, v):
    if v:
        buf[n >> 3] |= 1 << (7 - (n & 7))
    else:
        buf[n >> 3] &= ~(1 << (7 - (n & 7)))


def setbits(buf, n, width, val):
    for k in range(width):
        setbit(buf, n + k, (val >> (width - 1 - k)) & 1)


crc = bz_crc(text)
setbits(z, 32 + 48, 32, crc)  # the block CRC
setbit(z, 32 + 48 + 32, 1)  # randomised
# the end of stream: 48 bit magic 0x177245385090 then the 32 bit combined CRC
tot = len(z) * 8
found = None
for off in range(tot - 80, max(0, tot - 80 - 16), -1):
    v = 0
    for k in range(48):
        v = (v << 1) | getbit(z, off + k)
    if v == 0x177245385090:
        found = off
        break
assert found is not None
setbits(z, found + 48, 32, crc)  # one block: the combined CRC is the block CRC
w("random_block.bz2", bytes(z))
w("random_block.bin", text)

# ---------------------------------------------------------------------------- brotli
import brotli
w("plain_q5.br", brotli.compress(plain, quality=5))
w("plain_q11.br", brotli.compress(plain, quality=11))
w("plain_q1_lgwin10.br", brotli.compress(plain, quality=1, lgwin=10))
w("plain_text9.br", brotli.compress(plain, quality=9, mode=brotli.MODE_TEXT))
words = (b"the information of the people and the government is about the development of the community "
         b"which was created between the university and the international business ") * 12
w("words.bin", words)
w("words.br", brotli.compress(words, quality=11, mode=brotli.MODE_TEXT))
w("empty.br", brotli.compress(b""))


class BitW:
    """LSB-first bit writer (the order of Brotli)"""
    def __init__(self):
        self.bits = []

    def put(self, v, n):
        for i in range(n):
            self.bits.append((v >> i) & 1)

    def align(self):
        while len(self.bits) % 8:
            self.bits.append(0)

    def raw(self, data):
        self.align()
        for b in data:
            self.put(b, 8)

    def done(self):
        self.align()
        out = bytearray()
        for i in range(0, len(self.bits), 8):
            out.append(sum(b << k for k, b in enumerate(self.bits[i:i + 8])))
        return bytes(out)


# a stream made only of uncompressed and metadata meta-blocks (WBITS = 24, then 16 in a second file)
raw1, raw2 = plain[:300], plain[300:400]
bw = BitW()
bw.put(1, 1); bw.put(7, 3)          # WBITS: 17 + 7 = 24
bw.put(0, 1); bw.put(0, 2); bw.put(len(raw1) - 1, 16); bw.put(1, 1); bw.raw(raw1)   # stored meta-block
bw.put(0, 1); bw.put(3, 2); bw.put(0, 1); bw.put(1, 2); bw.put(19, 8); bw.raw(b"metadata is skipped!")  # metadata
bw.put(0, 1); bw.put(0, 2); bw.put(len(raw2) - 1, 16); bw.put(1, 1); bw.raw(raw2)   # stored again
bw.put(1, 1); bw.put(1, 1)          # the last, empty meta-block
w("stored.br", bw.done())
w("stored.bin", raw1 + raw2)
bw = BitW()
bw.put(0, 1)                         # WBITS 16
bw.put(1, 1); bw.put(1, 1)           # an empty stream
w("tiny_empty.br", bw.done())

# ---------------------------------------------------------------------------- gzip / zlib / deflate
w("plain.gz", gzip.compress(plain, 6, mtime=0))
b = io.BytesIO()
with gzip.GzipFile(filename="plain.bin", mode="wb", fileobj=b, mtime=1) as f:
    f.write(plain[:9000])
w("plain_multi.gz", b.getvalue() + gzip.compress(plain[9000:], 9, mtime=0) + bytes(8))
w("plain.zlib", zlib.compress(plain, 9))
c = zlib.compressobj(6, zlib.DEFLATED, -15)
w("plain.deflate", c.compress(plain) + c.flush())
c = zlib.compressobj(6, zlib.DEFLATED, -15, 9, zlib.Z_FIXED)
w("plain_fixed.deflate", c.compress(plain[:3000]) + c.flush())
w("plain_stored.gz", gzip.compress(plain[:20000], 0, mtime=0))
# a gzip stream whose second member refers to the first member's data (a preset dictionary):
# every inflater must refuse it -- a gzip member has a window of its own
m1 = b"hello world, hello world, hello world!!"
tail = b"hello world, hello world"
co = zlib.compressobj(9, zlib.DEFLATED, -15, 9, zlib.Z_DEFAULT_STRATEGY, m1)
raw2 = co.compress(tail) + co.flush()
hdr = bytes([0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3])
w("crossmember.gz", gzip.compress(m1, 6, mtime=0) + hdr + raw2 + struct.pack("<II", zlib.crc32(tail), len(tail)))

# ------------------------------------------------------------------ a small tar in every compression
import tarfile
tb = io.BytesIO()
with tarfile.open(fileobj=tb, mode="w", format=tarfile.USTAR_FORMAT) as tf:
    def add(name, data=b"", mode=0o644, typ=tarfile.REGTYPE, link=""):
        ti = tarfile.TarInfo(name)
        ti.size = len(data) if typ == tarfile.REGTYPE else 0
        ti.mode = mode
        ti.mtime = 1700000000
        ti.type = typ
        ti.linkname = link
        tf.addfile(ti, io.BytesIO(data) if typ == tarfile.REGTYPE else None)
    add("pkg/", mode=0o755, typ=tarfile.DIRTYPE)
    add("pkg/bin/", mode=0o755, typ=tarfile.DIRTYPE)
    add("pkg/bin/run", b"#!/bin/sh\necho hello from the archive\n", mode=0o755)
    add("pkg/bin/run-link", typ=tarfile.SYMTYPE, link="run")
    add("pkg/readme.txt", plain[:3000])
    add("pkg/data/", mode=0o755, typ=tarfile.DIRTYPE)
    add("pkg/data/blob.bin", rb)
tar_bytes = tb.getvalue()
w("pack.tar", tar_bytes)
w("pack.tar.zst", zstd(["-3"], tar_bytes))
w("pack.tar.xz", lzma.compress(tar_bytes, preset=3))
w("pack.tar.bz2", bz2.compress(tar_bytes, 9))
w("pack.tar.lz4", lf.compress(tar_bytes, content_checksum=True))
w("pack.tar.gz", gzip.compress(tar_bytes, 6, mtime=0))

sys.stderr.write("fixtures in %s: %d files, %d octets\n" % (
    out, len(os.listdir(out)), sum(os.path.getsize(os.path.join(out, f)) for f in os.listdir(out))))
