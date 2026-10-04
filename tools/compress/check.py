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
# RUNNER: a program that runs the probe (qemu-aarch64 for an AArch64 build, wine for a
# Windows build); CHECK_QUICK=1: a small corpus and fewer hostile cases (for the slow runners)
RUNNER = os.environ.get("RUNNER", "").split()
QUICK = int(os.environ.get("CHECK_QUICK") or 0)   # 1: reduced corpus; 2: tiny (for Wine)
FMTS = sys.argv[2:] or ["lz4", "zstd", "xz", "lzma", "bz2", "br", "gz", "zlib", "deflate", "auto"]
PARTS = os.environ.get("CHECK_PARTS", "decode,encode,hostile").split(",")
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
    # relative names, working directory W: the same call works for a Windows build under Wine
    cmd = RUNNER + [PROBE, op, fmt, "in.bin", "out.bin"] + ([str(arg if arg is not None else 0)] if (arg is not None or dict_path) else [])
    if dict_path:
        shutil.copyfile(dict_path, os.path.join(W, "dict.bin"))
        cmd.append("dict.bin")
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout, cwd=W)
        # a runner (Wine, qemu) now and then fails to start a process on a busy machine: no output,
        # no message. Ask again; a real crash of the probe repeats and is reported.
        tries = 0
        while RUNNER and r.returncode != 0 and not r.stdout and not r.stderr and tries < 3:
            tries += 1
            r = subprocess.run(cmd, capture_output=True, timeout=timeout, cwd=W)
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
    items = corpus_full()
    if not QUICK:
        return items
    if QUICK >= 2:
        keep = {"empty": None, "rand1": None, "a1": None, "abab": 6000, "text600k": 30000, "runs": 20000, "bcjmix": 12000,
                "dna": 10000, "sparse": 10000}
        return [(n, d if keep[n] is None else d[: keep[n]]) for n, d in items if n in keep]
    keep = {"empty": None, "rand1": None, "rand3": None, "rand12": None, "rand13": None, "a1": None, "abab": 20000,
            "text600k": 60000, "source": 60000, "runs": 40000, "dna": 30000, "bcjmix": 40000, "skew": 40000,
            "elf3M": 80000, "random100k": 30000, "zeros1M": 200000, "sparse": 40000}
    out = []
    for name, data in items:
        if name in keep:
            out.append((name, data if keep[name] is None else data[: keep[name]]))
    return out


def corpus_full():
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
    # machine-code-like data for the BCJ filters: random with branch patterns of every architecture
    mix = bytearray(rnd.getrandbits(8) for _ in range(160000))
    for i in range(0, len(mix) - 16, 16):
        k = rnd.randrange(8)
        if k == 0:    # x86 call/jmp with a 00/FF high byte
            mix[i] = rnd.choice((0xE8, 0xE9)); mix[i + 4] = rnd.choice((0, 0xFF))
        elif k == 1:  # ARM bl
            mix[i + 3] = 0xEB
        elif k == 2:  # Thumb bl pair
            mix[i + 1] = 0xF0 | (mix[i + 1] & 7); mix[i + 3] = 0xF8 | (mix[i + 3] & 7)
        elif k == 3:  # PowerPC bl
            mix[i] = 0x48 | (mix[i] & 3); mix[i + 3] = (mix[i + 3] & 0xFC) | 1
        elif k == 4:  # SPARC call
            mix[i] = 0x40; mix[i + 1] &= 0x3F
        elif k == 5:  # x86 with E8 close to another
            mix[i] = 0xE8; mix[i + 1] = 0; mix[i + 2] = 0; mix[i + 3] = 0xE8; mix[i + 4] = 0xFF; mix[i + 8] = 0xE9
    items.append(("bcjmix", bytes(mix)))
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
        import lz4.block as lb

        def legacy(d):
            # the legacy frame (lz4 -l): magic, then [u32 size][block] ..., blocks of up to 8 MiB
            o = struct.pack("<I", 0x184C2102)
            step = 1 << 20
            for i in range(0, max(len(d), 1), step):
                blk = lb.compress(d[i:i + step], mode="high_compression", compression=5, store_size=False)
                o += struct.pack("<I", len(blk)) + blk
                if i + step >= len(d):
                    break
            return o
        out.append(("lz4-legacy", legacy))
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
        try:  # python-zstandard (when installed): another libzstd front end, long distance matching, streaming writer
            import zstandard as zs
            out.append(("pyzstd-5-chk", lambda d: zs.ZstdCompressor(level=5, write_checksum=True).compress(d)))
            out.append(("pyzstd-ldm", lambda d: zs.ZstdCompressor(compression_params=zs.ZstdCompressionParameters.from_level(
                3, enable_ldm=True, window_log=24, ldm_hash_log=14)).compress(d)))

            def pystream(d):
                import io as _io
                b = _io.BytesIO()
                with zs.ZstdCompressor(level=4, write_content_size=False).stream_writer(b, closefd=False) as w:
                    for i in range(0, len(d), 50000):
                        w.write(d[i:i + 50000])
                return b.getvalue()
            out.append(("pyzstd-stream", pystream))
        except ImportError:
            pass
    elif fmt == "xz":
        import lzma
        out.append(("xz-6", lambda d: lzma.compress(d, preset=6)))
        out.append(("xz-0", lambda d: lzma.compress(d, preset=0)))
        out.append(("xz-9e", lambda d: lzma.compress(d, preset=9 | lzma.PRESET_EXTREME)))
        out.append(("xz-crc32", lambda d: lzma.compress(d, check=lzma.CHECK_CRC32)))
        out.append(("xz-crc64", lambda d: lzma.compress(d, check=lzma.CHECK_CRC64)))
        out.append(("xz-mt-blocks", lambda d: subprocess.run(["xz", "-c", "-T4", "--block-size=65536", "-3"], input=d,
                                                            capture_output=True).stdout))
        out.append(("xz-sha256", lambda d: lzma.compress(d, check=lzma.CHECK_SHA256)))
        out.append(("xz-none", lambda d: lzma.compress(d, check=lzma.CHECK_NONE)))
        out.append(("xz-delta+lzma2", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ, check=lzma.CHECK_CRC32,
                                                              filters=[{"id": lzma.FILTER_DELTA, "dist": 4},
                                                                       {"id": lzma.FILTER_LZMA2, "preset": 3}])))
        out.append(("xz-x86+lzma2", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ,
                                                           filters=[{"id": lzma.FILTER_X86},
                                                                    {"id": lzma.FILTER_LZMA2, "preset": 4}])))
        for fname, fid in (("ppc", lzma.FILTER_POWERPC), ("ia64", lzma.FILTER_IA64), ("arm", lzma.FILTER_ARM),
                           ("armthumb", lzma.FILTER_ARMTHUMB), ("sparc", lzma.FILTER_SPARC)):
            out.append(("xz-%s+lzma2" % fname, (lambda fid: lambda d: lzma.compress(d, format=lzma.FORMAT_XZ, filters=[
                {"id": fid}, {"id": lzma.FILTER_LZMA2, "preset": 1}]))(fid)))
        out.append(("xz-delta+x86+lzma2", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ, filters=[
            {"id": lzma.FILTER_DELTA, "dist": 2}, {"id": lzma.FILTER_X86}, {"id": lzma.FILTER_LZMA2, "preset": 1}])))
        out.append(("xz-arm64+lzma2", lambda d: subprocess.run(["xz", "-c", "--arm64", "--lzma2=preset=1"], input=d,
                                                               capture_output=True).stdout))
        out.append(("xz-lc0lp2pb0", lambda d: lzma.compress(d, format=lzma.FORMAT_XZ,
                                                           filters=[{"id": lzma.FILTER_LZMA2, "preset": 2, "lc": 0,
                                                                     "lp": 2, "pb": 0}])))
    elif fmt == "gz":
        import gzip, io
        out.append(("gzip-1", lambda d: gzip.compress(d, 1, mtime=0)))
        out.append(("gzip-6", lambda d: gzip.compress(d, 6, mtime=123456)))
        out.append(("gzip-9", lambda d: gzip.compress(d, 9)))
        out.append(("gzip-0-stored", lambda d: gzip.compress(d, 0)))
        def named(d):
            b = io.BytesIO()
            with gzip.GzipFile(filename="some/name.txt", mode="wb", fileobj=b, mtime=1) as f:
                f.write(d)
            return b.getvalue()
        out.append(("gzip-fname", named))
        out.append(("gzip-multimember", lambda d: gzip.compress(d[: len(d) // 3]) + gzip.compress(d[len(d) // 3:]) ))
        out.append(("gzip-zeropad", lambda d: gzip.compress(d) + bytes(16)))
        def header_crc(d):
            # a header with FHCRC and FEXTRA set, made by hand
            body = zlib.compressobj(6, zlib.DEFLATED, -15)
            raw = body.compress(d) + body.flush()
            hdr = bytes([0x1f, 0x8b, 8, 2 | 4, 0, 0, 0, 0, 0, 3]) + struct.pack("<H", 5) + b"extra"
            h = hdr + struct.pack("<H", zlib.crc32(hdr) & 0xFFFF)
            return h + raw + struct.pack("<II", zlib.crc32(d) & 0xFFFFFFFF, len(d) & 0xFFFFFFFF)
        out.append(("gzip-fhcrc-fextra", header_crc))
    elif fmt == "zlib":
        out.append(("zlib-1", lambda d: zlib.compress(d, 1)))
        out.append(("zlib-9", lambda d: zlib.compress(d, 9)))
        out.append(("zlib-0", lambda d: zlib.compress(d, 0)))
    elif fmt == "deflate":
        def raw(level, strategy=zlib.Z_DEFAULT_STRATEGY):
            def f(d):
                c = zlib.compressobj(level, zlib.DEFLATED, -15, 9, strategy)
                return c.compress(d) + c.flush()
            return f
        out.append(("deflate-6", raw(6)))
        out.append(("deflate-9", raw(9)))
        out.append(("deflate-fixed", raw(6, zlib.Z_FIXED)))
        out.append(("deflate-huffman-only", raw(6, zlib.Z_HUFFMAN_ONLY)))
        out.append(("deflate-rle", raw(6, zlib.Z_RLE)))
    elif fmt == "auto":
        for f in ("gz", "zstd", "xz", "bz2", "lz4"):
            out.append(("auto-" + f, ref_compressors(f)[1][1] if f != "gz" else ref_compressors(f)[0][1]))
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
    if fmt == "gz" or fmt == "auto":
        import gzip
        return gzip.decompress(data)
    if fmt == "zlib":
        return zlib.decompress(data)
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
ENC_LEVELS = {"lz4": [None], "zstd": [None, 1, 3, 6], "xz": [None, 1, 3, 6, 9], "lzma": [None, 1, 9], "bz2": [None, 1, 9], "br": [None], "gz": [None], "zlib": [None], "deflate": [None], "auto": [None]}

ENC_ENABLED = {"lz4": True, "zstd": True, "xz": True, "lzma": True, "bz2": False, "br": False, "gz": False, "zlib": False, "deflate": False, "auto": False}


def group(fmt):
    t0 = time.time()
    items = corpus()
    n0 = len(FAILS)
    # ---- decode: reference compressors -> ours
    refs = ref_compressors(fmt) if "decode" in PARTS else []
    if QUICK >= 2:
        refs = refs[:: max(1, len(refs) // 3)][:3]
    for label, comp in refs:
        for name, data in items:
            if len(data) > (1 << 20) and ("9e" in label or "-19" in label or "22" in label or "hc12" in label
                                         or "q11" in label or "q9" in label):
                data = data[: 700000]
            z = comp(data)
            st, got = run_probe("d", fmt, z)
            check("decode %s %s" % (label, name), st.startswith("OK") and got == data,
                  "-> %s (%d vs %d octets)" % (st, len(got), len(data)))
            if len(z) < 300000 or name in ("text600k",):
                for piece in (((4093,) if QUICK else (1, 4093, 65537)) if fmt != "lzma" else ()):
                    if piece == 1 and len(z) > 20000:
                        continue
                    st, got = run_probe("ds", fmt, z, piece)
                    check("stream-decode(%d) %s %s" % (piece, label, name), st.startswith("OK") and got == data,
                          "-> %s (%d vs %d octets)" % (st, len(got), len(data)))
    print("  %-4s decode (reference -> lib)        %s" % (fmt, "ok" if len(FAILS) == n0 else "FAILED"))
    n1 = len(FAILS)
    # ---- encode: ours -> reference
    if ENC_ENABLED.get(fmt) and "encode" in PARTS:
        for lv in (ENC_LEVELS[fmt][:2] if QUICK >= 2 else ENC_LEVELS[fmt]):
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
                if fmt != "lzma" and (lv is None or lv == ENC_LEVELS[fmt][-1]):
                    for chunk in ((4097,) if QUICK else (1, 4097, 1 << 20)):
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
    if "hostile" not in PARTS:
        print("  %-4s (hostile skipped)" % fmt)
        return
    comp = ref_compressors(fmt)[0][1]
    rnd = random.Random(7)
    base = dict(items)
    for name in ("text600k", "elf3M", "runs"):
        data = base[name][:40000]
        z = comp(data)
        step = max(1, len(z) // (10 if QUICK >= 2 else 30 if QUICK else 150))
        for cut in list(range(0, min(len(z), 40))) + list(range(40, len(z), step)):
            st, got = run_probe("d", fmt, z[:cut])
            check("cut %s at %d/%d" % (name, cut, len(z)), st.startswith("ERROR"), "-> %s" % st)
        for _ in range(5 if QUICK >= 2 else 20 if QUICK else 120):
            zz = bytearray(z)
            for _k in range(rnd.choice((1, 1, 2, 5))):
                zz[rnd.randrange(len(zz))] ^= 1 << rnd.randrange(8)
            st, got = run_probe("d", fmt, bytes(zz))
            check("flip %s" % name, st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
        for _ in range(3 if QUICK >= 2 else 10 if QUICK else 60):
            junk = bytes(rnd.getrandbits(8) for _ in range(rnd.randint(1, 300)))
            if rnd.random() < 0.7:
                junk = z[: rnd.randint(1, 12)] + junk
            st, got = run_probe("d", fmt, junk)
            check("junk %s" % name, st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
    # bomb: 64 MiB of zeros under a 1 MiB limit
    big = (2 << 20) if QUICK >= 2 else (4 << 20) if QUICK else (64 << 20)
    zeros = bytes(big)
    z = comp(zeros)
    st, got = run_probe("d", fmt, z, 1 << 20)
    check("bomb limited", st == "ERROR TooLarge" and len(got) == 0, "-> %s" % st)
    st, got = run_probe("d", fmt, z, big)
    check("bomb exact limit", st.startswith("OK") and len(got) == big, "-> %s" % st)
    st, got = run_probe("d", fmt, z, big - 1)
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
                           ("10samples", b"".join(samples[100:110])), ("big", big), ("short", b"{}")][: (2 if QUICK >= 2 else 3 if QUICK else 6)]:
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
    for cut in range(0, len(z), max(1, len(z) // (12 if QUICK >= 2 else 60))):
        st, got = run_probe("d", "zstd", z[:cut], None, dict_path=dpath)
        check("dict cut %d" % cut, st.startswith("ERROR"), "-> %s" % st)
    for _ in range(8 if QUICK >= 2 else 100):
        zz = bytearray(z)
        zz[rnd.randrange(len(zz))] ^= 1 << rnd.randrange(8)
        st, got = run_probe("d", "zstd", bytes(zz), None, dict_path=dpath)
        check("dict flip", st.startswith("OK") or st.startswith("ERROR"), "-> %s" % st)
    # damaged dictionary
    for _ in range(4 if QUICK >= 2 else 40):
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
