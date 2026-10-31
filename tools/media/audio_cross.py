#!/usr/bin/env python3
import array, math, os, random, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIRNC = os.environ.get("FIRNC", "/root/firn/compiler/target/release/firnc")
CASES = [
    ("wav", "pcm_s16le", "wav", 44100, 2, 0), ("wav", "pcm_u8", "wav", 8000, 1, 0), ("wav", "pcm_s24le", "wav", 48000, 2, 0),
    ("wav", "pcm_f32le", "wav", 32000, 1, 0), ("flac", "flac", "flac", 44100, 2, 0), ("flac", "flac", "flac", 22050, 1, 0),
    ("mp3", "libmp3lame", "mp3", 44100, 2, 1), ("mp3", "libmp3lame", "mp3", 22050, 1, 1), ("mp3", "libmp3lame", "mp3", 8000, 1, 1),
    ("vorbis", "libvorbis", "ogg", 44100, 2, 1), ("vorbis", "libvorbis", "ogg", 16000, 1, 1),
]

def run(*a):
    return subprocess.run(a, check=True, capture_output=True)

def floats(b):
    x = array.array("f")
    x.frombytes(b)
    return x

def stats(ref, got):
    n = min(len(ref), len(got))
    mx = se = ss = 0.0
    for i in range(n):
        d = ref[i] - got[i]
        mx = max(mx, abs(d))
        se += d * d
        ss += ref[i] * ref[i]
    return n, mx, (10 * math.log10(ss / se) if se > 0 and ss > 0 else 999.0)

def main():
    tmp = tempfile.mkdtemp()
    dump = os.path.join(tmp, "dump")
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    subprocess.run([FIRNC, "-o", dump, os.path.join(ROOT, "tools/media/audio_dump.fi")], check=True, env=env)
    bad = 0
    corpus = []
    for fmt, codec, ext, rate, ch, lossy in CASES:
        f = os.path.join(tmp, "t_%s_%d_%d.%s" % (codec, rate, ch, ext))
        src = "anoisesrc=d=2:r=%d:a=0.3,aecho=0.8:0.8:40:0.5" % rate
        run("ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", src, "-ac", str(ch), "-c:a", codec, f)
        ref = floats(run("ffmpeg", "-v", "error", "-i", f, "-f", "f32le", "-").stdout)
        out = os.path.join(tmp, "o.raw")
        info = run(dump, f, out).stdout.decode()
        got = floats(open(out, "rb").read())
        n, mx, snr = stats(ref, got)
        okc = len(ref) == len(got) and (mx == 0.0 if not lossy else snr > 70)
        frames = len(ref) // ch
        k = frames // 3
        run(dump, f, out, str(k), "5000")
        sk = floats(open(out, "rb").read())
        n2, mx2, snr2 = stats(ref[k * ch:k * ch + len(sk)], sk)
        oks = len(sk) == 5000 * ch and (mx2 < 1e-3 if lossy else mx2 == 0.0)
        print("%-7s %-11s %6d Hz %d ch  len %s  maxdiff %.2e snr %.1f  seek %s (maxdiff %.2e)" % (
            fmt, codec, rate, ch, "ok" if len(ref) == len(got) else "DIFF", mx, snr, "ok" if oks else "BAD", mx2))
        bad += 0 if okc and oks else 1
        corpus.append(f)
    rnd = random.Random(7)
    runs = int(os.environ.get("FUZZ_RUNS", "2000"))
    crashes = 0
    for i in range(runs):
        f = corpus[i % len(corpus)]
        b = bytearray(open(f, "rb").read())
        m = rnd.randrange(3)
        if m == 0:
            b = b[:rnd.randrange(1, len(b))]
        else:
            for _ in range(rnd.randrange(1, 40)):
                b[rnd.randrange(len(b))] = rnd.randrange(256)
            if m == 2:
                b = b[:rnd.randrange(len(b) // 2, len(b))]
        fz = os.path.join(tmp, "fz.bin")
        open(fz, "wb").write(b)
        try:
            r = subprocess.run([dump, fz, os.path.join(tmp, "fz.raw")], capture_output=True, timeout=20)
            if r.returncode not in (0, 3, 4, 5):
                crashes += 1
                print("CRASH", r.returncode, f)
        except subprocess.TimeoutExpired:
            crashes += 1
            print("HANG", f)
    print("fuzz runs %d crashes/hangs %d" % (runs, crashes))
    print("RESULT", "FAIL" if bad or crashes else "OK")
    subprocess.run(["rm", "-rf", tmp])
    return 1 if bad or crashes else 0

sys.exit(main())
