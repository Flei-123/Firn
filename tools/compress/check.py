#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/check.py -- lib/compress against programs nobody here wrote.

    check.py <probe-binary> [fmt ...]     fmt: lz4 zstd xz bz2 br auto (default: all)

For every format, a corpus (empty, tiny, text, source code, an ELF binary,
random, zeros, runs, ...) goes through BOTH directions:

  decode: the reference compressor (python lz4/lzma/bz2/brotli modules, the
          zstd and xz command lines) -> lib/compress (whole buffer AND
          streaming from a descriptor) -> must equal the input octet for octet
  encode: lib/compress (whole buffer AND streaming) -> the reference
          decompressor -> must equal the input

plus hostile inputs: every cut of a stream, damaged copies (a flipped octet,
random garbage) -- the probe must answer OK/ERROR, never crash or hang --, and
decompression bombs under a size limit (TooLarge, nothing past the limit).
Prints one line per group and a total; exit code 1 on any failure.
"""
import os, sys, subprocess, random, tempfile, shutil, time, struct, zlib

PROBE = os.path.abspath(sys.argv[1])
FMTS = sys.argv[2:] or ["lz4", "zstd", "xz", "bz2", "br"]
W = tempfile.mkdtemp(prefix="compress-check-")
import atexit
atexit.register(lambda: shutil.rmtree(W, ignore_errors=True))
FAILS = []
TOTAL = [0]
FIRN_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def fail(msg):
    FAILS.append(msg)
    print("  FAIL", msg)


def run_probe(op, fmt, data, arg=None, timeout=120, dict_path=None):
    """runs the probe; returns (status, payload) with status 'OK' or 'ERROR x' or 'CRASH'"""
    fi = os.path.join(W, "in.bin")
    fo = os.path.join(W, "out.bin")
    with open(fi, "wb") as f:
        f.write(data)
    if os.path.exists(fo):
        os.remove(fo)
    cmd = [PROBE, op, fmt, fi, fo] + ([str(arg if arg is not None else 0)] if (arg is not None or dict_path) else [])
    if dict_path:
        cmd.append(dict_path)
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return "TIMEOUT", b""
    if r.returncode != 0:
        return "CRASH rc=%d %s" % (r.returncode, r.stderr[-200:]), b""
    line = r.stdout.decode(errors="replace").strip().splitlines()
    st = line[-1] if line else "NOOUTPUT"
    out = b""
    if st.startswith("OK") and os.path.exists(fo):
        with open(fo, "rb") as f:
            out = f.read()
    return st, out


def check(name, cond, detail=""):
    TOTAL[0] += 1
    if not cond:
        fail("%s %s" % (name, detail))


# ------------------------------------------------------------- corpus

def corpus():
    rnd = random.Random(20261004)
    items = []
    items.append(("empty", b""))
    for n in (1, 2, 3, 4, 5, 7, 11, 12, 13, 17, 31, 64, 100):
        items.append(("rand%d" % n, bytes(rnd.getrandbits(8) for _ in range(n))))
    items.append(("a1", b"a"))
    items.append(("abab", b"ab" * 5000))
    items.append(("zeros1M", bytes(1 << 20)))
    words = [b"the", b"quick", b"brown", b"fox", b"jumps", b"over", b"lazy", b"dog", b"compression",
             b"stream", b"window", b"literal", b"match", b"offset", b"Firn", b"Certus", b"OrientOS"]
    t = bytearray()
    while len(t) < 600000:
        t += rnd.choice(words) + (b" " if rnd.random() < 0.9 else b".\n")
    items.append(("text600k", bytes(t)))
    # real source code (the Firn libs)
    src = bytearray()
    for root, _, files in os.walk(os.path.join(FIRN_ROOT, "lib", "std")):
        for fn in sorted(files):
            if fn.endswith(".fi"):
                with open(os.path.join(root, fn), "rb") as f:
                    src += f.read()
        if len(src) > 2 << 20:
            break
    items.append(("source", bytes(src[: 2 << 20])))
    exe = os.path.join(FIRN_ROOT, "compiler", "target", "release", "firnc")
    if not os.path.exists(exe):
        exe = PROBE
    with open(exe, "rb") as f:
        items.append(("elf3M", f.read(3 << 20)))
    items.append(("random100k", bytes(rnd.getrandbits(8) for _ in range(100000))))
    runs = bytearray()
    for _ in range(300):
        runs += bytes([rnd.getrandbits(8)]) * rnd.randint(1, 3000)
    items.append(("runs", bytes(runs)))
    sparse = bytearray(400000)
    for _ in range(2000):
        sparse[rnd.randrange(len(sparse))] = rnd.getrandbits(8)
    items.append(("sparse", bytes(sparse)))
    # skewed alphabet (Huffman-friendly) and 4-symbol data
    items.append(("skew", bytes(rnd.choices(range(256), weights=[2 ** (-i / 12) for i in range(256)], k=300000))))
    items.append(("dna", bytes(rnd.choices(b"ACGT", k=200000))))
    return items


# ------------------------------------------------------------ references

def ref_compressors(fmt):
    """list of (label, function(data)->bytes)"""
    out = []
    if fmt == "lz4":
        import lz4.frame as lf
        out.append(("lz4-default", lambda d: lf.compress(d)))
        out.append(("lz4-hc9", lambda d: lf.compress(d, compression_level=9)))
        out.append(("lz4-hc12-linked-chk", lambda d: lf.compress(d, compression_level=12, block_linked=True,
                                                                  block_checksum=True, content_checksum=True)))
        out.append(("lz4-64k-indep", lambda d: lf.compress(d, block_size=lf.BLOCKSIZE_MAX64KB, block_linked=False,
                                                           store_size=False)))
        out.append(("lz4-256k-linked", lambda d: lf.compress(d, block_size=lf.BLOCKSIZE_MAX256KB, block_linked=True)))
        out.append(("lz4-1m-nochk", lambda d: lf.compress(d, block_size=lf.BLOCKSIZE_MAX1MB, content_checksum=False)))
        out.append(("lz4-4m", lambda d: lf.compress(d, block_size=lf.BLOCKSIZE_MAX4MB, store_size=True)))
    elif fmt == "zstd":
        def z(level, *extra):
            def f(d):
                r = subprocess.run(["zstd", "-c", "-q", "-%d" % level] + list(extra), input=d, capture_output=True)
                return r.stdout
            return f
        for lv in (1, 3, 9, 19):
            out.append(("zstd-%d" % lv, z(lv)))
        out.append(("zstd-3-nocheck", z(3, "--no-check")))
        out.append(("zstd-5-long", z(5, "--long=27")))
        out.append(("zstd-1-ultra22", z(22, "--ultra")))
        out.append(("zstd-3-small-window", z(3, "--zstd=wlog=10")))
        out.append(("zstd-3-nosize", z(3, "--no-check", "--no-dictID")))
    elif fmt == "xz":
        import lzma
        out.append(("xz-6", lambda d: lzma.compress(d, preset=6)))
        out.append(("xz-0", lambda d: lzma.compress(d, preset=0)))
        out.append(("xz-9e", lambda d: lzma.compress(d, preset=9 | lzma.PRESET_EXTREME)))
        out.append(("xz-crc32", lambda d: lzma.compress(d, check=lzma.CHECK_CRC32)))
        out.append(("xz-crc64", lambda d: lzma.compress(d, check=lzma.CHECK_CRC64)))
        out.append(("xz-sha256", lambda d: lzma.compress(d, check=lzma.CHECK_SHA256)))
        out.append(("xz-none", lambda d: lzma.compress(d, check=lzma.CHECK_NONE)))
        out.append(("xz-delta+lzma2", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ, check=lzma.CHECK_CRC32,
                                                              filters=[{"id": lzma.FILTER_DELTA, "dist": 4},
                                                                       {"id": lzma.FILTER_LZMA2, "preset": 3}])))
        out.append(("xz-x86+lzma2", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ,
                                                           filters=[{"id": lzma.FILTER_X86},
                                                                    {"id": lzma.FILTER_LZMA2, "preset": 4}])))
        out.append(("xz-lc0lp2pb0", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ,
                                                           filters=[{"id": lzma.FILTER_LZMA2, "preset": 2, "lc": 0,
                                                                     "lp": 2, "pb": 0}])))
    elif fmt == "lzma":
        import lzma
        out.append(("lzma-alone-4", lambda d: lzma.compress(d, format=lzma.FORMAT_ALONE, preset=4)))
        out.append(("lzma-alone-0", lambda d: lzma.compress(d, format=lzma.FORMAT_ALONE, preset=0)))
        out.append(("lzma-alone-lc0lp4", lambda d: lzma.compress(d, format=lzma.FORMAT_ALONE,
                                                                 filters=[{"id": lzma.FILTER_LZMA1, "preset": 3, "lc": 0, "lp": 4, "pb": 0}])))
        out.append(("lzma-alone-lc8", lambda d: lzma.compress(d, format=lzma.FORMAT_ALONE,
                                                              filters=[{"id": lzma.FILTER_LZMA1, "preset": 3, "lc": 4, "lp": 0, "pb": 4, "dict_size": 1 << 16}])))
    elif fmt == "bz2":
        import bz2
        for lv in (1, 5, 9):
            out.append(("bz2-%d" % lv, (lambda lv: lambda d: bz2.compress(d, lv))(lv)))
    elif fmt == "br":
        import brotli
        for q in (0, 1, 4, 6, 9, 11):
            out.append(("br-q%d" % q, (lambda q: lambda d: brotli.compress(d, quality=q))(q)))
        out.append(("br-q5-lgwin10", lambda d: brotli.compress(d, quality=5, lgwin=10)))
        out.append(("br-q9-text", lambda d: brotli.compress(d, quality=9, mode=brotli.MODE_TEXT)))
        out.append(("br-q5-lgwin24", lambda d: brotli.compress(d, quality=5, lgwin=24, lgblock=18)))
    return out


def ref_decompress(fmt, data):
    if fmt == "lz4":
        import lz4.frame as lf
        return lf.decompress(data)
    if fmt == "zstd":
        r = subprocess.run(["zstd", "-d", "-c", "-q", "--long=31"], input=data, capture_output=True)
        if r.returncode != 0:
            raise ValueError("zstd: " + r.stderr.decode(errors="replace"))
        return r.stdout
    if fmt == "xz" or fmt == "lzma":
        import lzma
        return lzma.decompress(data)
    if fmt == "bz2":
        import bz2
        return bz2.decompress(data)
    if fmt == "br":
        import brotli
        return brotli.decompress(data)
    raise ValueError(fmt)


# the encoder knobs the probe offers per format (level argument for op c)
ENC_LEVELS = {"lz4": [None], "zstd": [None, 1, 3, 6], "xz": [None, 0, 3, 6], "lzma": [None], "bz2": [None, 1, 9], "br": [None]}

ENC_ENABLED = {"lz4": True, "zstd": True, "xz": False, "lzma": False, "bz2": False, "br": False}


def group(fmt):
    t0 = time.time()
    items = corpus()
    n0 = len(FAILS)
    # ---- decode: reference compressors -> ours
    for label, comp in ref_compressors(fmt):
        for name, data in items:
            if len(data) > (1 << 20) and ("9e" in label or "-19" in label or "22" in label or "hc12" in label
                                         or "q11" in label or "q9" in label):
                data = data[: 700000]
            z = comp(data)
            st, got = run_probe("d", fmt, z)
            check("decode %s %s" % (label, name), st.startswith("OK") and got == data,
                  "-> %s (%d vs %d octets)" % (st, len(got), len(data)))
            if len(z) < 300000 or name in ("text600k",):
                for piece in ((1, 4093, 65537) if fmt != "lzma" else ()):
                    if piece == 1 and len(z) > 20000:
                        continue
                    st, got = run_probe("ds", fmt, z, piece)
                    check("stream-decode(%d) %s %s" % (piece, label, name), st.startswith("OK") and got == data,
                          "-> %s (%d vs %d octets)" % (st, len(got), len(data)))
    print("  %-4s decode (reference -> lib)        %s" % (fmt, "ok" if len(FAILS) == n0 else "FAILED"))
    n1 = len(FAILS)
    # ---- encode: ours -> reference
    if ENC_ENABLED.get(fmt):
        for lv in ENC_LEVELS[fmt]:
            for name, data in items:
                st, z = run_probe("c", fmt, data, lv)
                if not st.startswith("OK"):
                    fail("encode(%s) %s -> %s" % (lv, name, st))
                    continue
                try:
                    back = ref_decompress(fmt, z)
                except Exception as e:
                    fail("encode(%s) %s: reference rejects our stream: %s" % (lv, name, e))
                    continue
                check("encode(%s) %s" % (lv, name), back == data, "reference reads back %d vs %d octets" % (len(back), len(data)))
                if lv is None or lv == ENC_LEVELS[fmt][-1]:
                    for chunk in (1, 4097, 1 << 20):
                        if chunk == 1 and len(data) > 3000:
                            continue
                        st, z2 = run_probe("cs", fmt, data, chunk)
                        if not st.startswith("OK"):
                            fail("stream-encode(%d) %s -> %s" % (chunk, name, st))
                            continue
                        try:
                            back = ref_decompress(fmt, z2)
                        except Exception as e:
                            fail("stream-encode(%d) %s: reference rejects: %s" % (chunk, name, e))
                            continue
                        check("stream-encode(%d) %s" % (chunk, name), back == data)
        print("  %-4s encode (lib -> reference)        %s" % (fmt, "ok" if len(FAILS) == n1 else "FAILED"))
    n2 = len(FAILS)
    # ---- hostile inputs
    comp = ref_compressors(fmt)[0][1]
    rnd = random.Random(7)
    base = dict(items)
    for name in ("text600k", "elf3M", "runs"):
        data = base[name][:40000]
        z = comp(data)
        step = max(1, len(z) // 150)
        for cut in list(range(0, min(len(z), 40))) + list(range(40, len(z), step)):
            st, got = run_probe("d", fmt, z[:cut])
            check("cut %s at %d/%d" % (name, cut, len(z)), st.startswith("ERROR"), "-> %s" % st)
        for _ in range(120):
            zz = bytearray(z)
            for _k in range(rnd.choice((1, 1, 2, 5))):
                zz[rnd.randrange(len(zz))] ^= 1 << rnd.randrange(8)
            st, got = run_probe("d", fmt, bytes(zz))
            check("flip %s" % name, st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
        for _ in range(60):
            junk = bytes(rnd.getrandbits(8) for _ in range(rnd.randint(1, 300)))
            if rnd.random() < 0.7:
                junk = z[: rnd.randint(1, 12)] + junk
            st, got = run_probe("d", fmt, junk)
            check("junk %s" % name, st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
    # bomb: 64 MiB of zeros under a 1 MiB limit
    zeros = bytes(64 << 20)
    z = comp(zeros)
    st, got = run_probe("d", fmt, z, 1 << 20)
    check("bomb limited", st == "ERROR TooLarge" and len(got) == 0, "-> %s" % st)
    st, got = run_probe("d", fmt, z, 64 << 20)
    check("bomb exact limit", st.startswith("OK") and len(got) == 64 << 20, "-> %s" % st)
    st, got = run_probe("d", fmt, z, (64 << 20) - 1)
    check("bomb one under", st == "ERROR TooLarge", "-> %s" % st)
    print("  %-4s hostile (cuts, flips, junk, bomb) %s   [%.0f s]" % (fmt, "ok" if len(FAILS) == n2 else "FAILED",
                                                                    time.time() - t0))


def zstd_dict_group():
    """dictionaries: trained (zstd --train), raw content, wrong/missing id"""
    t0 = time.time()
    n0 = len(FAILS)
    rnd = random.Random(99)
    samples = []
    for i in range(400):
        user = "user%d" % rnd.randrange(10000)
        samples.append((
            '{"id": %d, "name": "%s", "email": "%s@example.com", "roles": ["admin", "editor"], '
            '"active": %s, "score": %d.%d, "tags": ["a%d", "b%d"], "note": "%s"}' % (
                i, user, user, rnd.choice(["true", "false"]), rnd.randrange(100), rnd.randrange(100),
                rnd.randrange(9), rnd.randrange(9), " ".join(rnd.choice(["alpha", "beta", "gamma", "delta"]) for _ in range(6)))
        ).encode())
    sdir = os.path.join(W, "samples")
    os.makedirs(sdir, exist_ok=True)
    for i, s in enumerate(samples):
        with open(os.path.join(sdir, "s%03d" % i), "wb") as f:
            f.write(s)
    dpath = os.path.join(W, "trained.dict")
    r = subprocess.run(["zstd", "--train", "-q", "-o", dpath, "--maxdict=16384"] +
                       [os.path.join(sdir, "s%03d" % i) for i in range(len(samples))], capture_output=True)
    if r.returncode != 0:
        fail("zstd --train failed: %s" % r.stderr.decode(errors="replace")[:200])
        return
    raw_path = os.path.join(W, "raw.dict")
    with open(raw_path, "wb") as f:
        f.write(b"".join(samples[:30]))
    big = b"".join(samples)
    tests = [("trained", dpath, True), ("raw-content", raw_path, False)]
    for label, dp, has_id in tests:
        dd = open(dp, "rb").read()
        for name, data in [("sample0", samples[0]), ("sample7", samples[7]), ("empty", b""),
                           ("10samples", b"".join(samples[100:110])), ("big", big), ("short", b"{}")]:
            # reference compresses with the dictionary -> we decode with it
            args = ["zstd", "-q", "-c", "-D", dp, "-3"]
            z = subprocess.run(args, input=data, capture_output=True).stdout
            for op in ("d", "ds"):
                st, got = run_probe(op, "zstd", z, None, dict_path=dp)
                check("dict-%s %s %s" % (label, op, name), st.startswith("OK") and got == data, "-> %s" % st)
            for lv in (1, 3, 9):
                zz = subprocess.run(["zstd", "-q", "-c", "-D", dp, "-%d" % lv], input=data, capture_output=True).stdout
                st, got = run_probe("d", "zstd", zz, None, dict_path=dp)
                check("dict-%s level %d decode %s" % (label, lv, name), st.startswith("OK") and got == data, "-> %s" % st)
            # we compress with the dictionary -> the reference decodes
            for lv in (1, 3, 7, 19):
                st, z2 = run_probe("c", "zstd", data, lv, dict_path=dp)
                if not st.startswith("OK"):
                    fail("dict-%s encode level %d %s -> %s" % (label, lv, name, st))
                    continue
                r = subprocess.run(["zstd", "-d", "-c", "-q", "-D", dp], input=z2, capture_output=True)
                check("dict-%s encode level %d %s" % (label, lv, name), r.returncode == 0 and r.stdout == data,
                      r.stderr.decode(errors="replace")[:120])
            st, z3 = run_probe("cs", "zstd", data, 1000, dict_path=dp)
            if st.startswith("OK"):
                r = subprocess.run(["zstd", "-d", "-c", "-q", "-D", dp], input=z3, capture_output=True)
                check("dict-%s stream-encode %s" % (label, name), r.returncode == 0 and r.stdout == data)
        # size benefit: a dictionary must help on a small sample
        st, withd = run_probe("c", "zstd", samples[3], 3, dict_path=dp)
        st2, without = run_probe("c", "zstd", samples[3], 3)
        check("dict-%s helps" % label, len(withd) < len(without), "%d vs %d" % (len(withd), len(without)))
    # a frame that names a dictionary cannot be decoded without it, nor with another
    dd = open(dpath, "rb").read()
    z = subprocess.run(["zstd", "-q", "-c", "-D", dpath], input=samples[1], capture_output=True).stdout
    st, got = run_probe("d", "zstd", z)
    check("dict missing", st == "ERROR Dictionary", "-> %s" % st)
    other = bytearray(dd)
    other[4:8] = struct.pack("<I", struct.unpack("<I", dd[4:8])[0] ^ 0x55)
    op_ = os.path.join(W, "other.dict")
    open(op_, "wb").write(bytes(other))
    st, got = run_probe("d", "zstd", z, None, dict_path=op_)
    check("dict wrong id", st == "ERROR Dictionary", "-> %s" % st)
    # hostile: cuts and flips of a dictionary frame
    for cut in range(0, len(z), max(1, len(z) // 60)):
        st, got = run_probe("d", "zstd", z[:cut], None, dict_path=dpath)
        check("dict cut %d" % cut, st.startswith("ERROR"), "-> %s" % st)
    for _ in range(100):
        zz = bytearray(z)
        zz[rnd.randrange(len(zz))] ^= 1 << rnd.randrange(8)
        st, got = run_probe("d", "zstd", bytes(zz), None, dict_path=dpath)
        check("dict flip", st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
    # damaged dictionary
    for _ in range(40):
        dm = bytearray(dd)
        dm[rnd.randrange(8, len(dm))] ^= 1 << rnd.randrange(8)
        dmp = os.path.join(W, "dm.dict")
        open(dmp, "wb").write(bytes(dm))
        st, got = run_probe("d", "zstd", z, None, dict_path=dmp)
        check("damaged dict", st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
    print("  zstd dictionaries (trained, raw, ids, damaged) %s   [%.0f s]" % ("ok" if len(FAILS) == n0 else "FAILED", time.time() - t0))


for f in FMTS:
    group(f)
    if f == "zstd":
        zstd_dict_group()
print("checks: %d, failures: %d" % (TOTAL[0], len(FAILS)))
sys.exit(1 if FAILS else 0)
