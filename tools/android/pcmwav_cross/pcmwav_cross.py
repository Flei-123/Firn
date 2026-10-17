#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/pcmwav_cross/pcmwav_cross.py -- cross-check of lib/plat/android/pcmwav.fi against Python's
# wave module and plain integer math. usage: pcmwav_cross.py <pcmwavtool> <trials> <seed>
# Prints "mismatches: N" (target 0). Also feeds damaged files to the parser: it must answer (no crash).
import io, math, random, subprocess, sys, wave

tool, trials, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
r = random.Random(seed)
lines, expect = [], []

def make_wav(rate, ch, pcm):
    b = io.BytesIO()
    w = wave.open(b, "wb")
    w.setnchannels(ch); w.setsampwidth(2); w.setframerate(rate)
    w.writeframes(pcm)
    w.close()
    return b.getvalue()

def rnd_pcm(n):
    mode = r.randrange(4)
    out = []
    for i in range(n):
        if mode == 0: v = r.randrange(-32768, 32768)
        elif mode == 1: v = r.choice([-32768, 32767, 0])
        elif mode == 2: v = int(r.gauss(0, 300))
        else: v = int(20000 * math.sin(i / 7.0))
        out.append(max(-32768, min(32767, v)))
    return out

def pack(vals):
    return b"".join((v & 0xFFFF).to_bytes(2, "little") for v in vals)

for t in range(trials):
    rate = r.choice([8000, 16000, 22050, 44100, 48000, r.randrange(1, 200000)])
    ch = r.choice([1, 1, 2, 3])
    nfr = r.randrange(0, 400)
    vals = rnd_pcm(nfr * ch)
    pcm = pack(vals)
    wav = make_wav(rate, ch, pcm)
    # H: header against Python's own
    lines.append("H %d %d %d" % (rate, ch, len(pcm)))
    expect.append(("H", wav[:44].hex()))
    # S: peak / rms
    if vals:
        pk = max(abs(v) for v in vals)
        rm = math.isqrt(sum(v * v for v in vals) // len(vals))
        lv = lambda x: math.isqrt(min(x, 32768) * 1000000 // 32768) if x > 0 else 0
        lines.append("S " + pcm.hex())
        expect.append(("S", "%d %d %d %d" % (pk, rm, min(lv(pk), 1000), min(lv(rm), 1000))))
    # P: parse, also with an extra odd chunk in front of data
    extra = b""
    if r.random() < 0.5:
        ln = r.randrange(0, 9)
        extra = b"LIST" + ln.to_bytes(4, "little") + bytes(r.randrange(256) for _ in range(ln)) + (b"\0" if ln % 2 else b"")
    f = wav[:36] + extra + wav[36:]
    lines.append("P " + f.hex())
    expect.append(("P", "OK %d %d 16 %d %d" % (rate, ch, 44 + len(extra), len(pcm))))
    # damaged variants: only "must answer"
    for _ in range(3):
        g = bytearray(f)
        for _ in range(r.randrange(1, 6)):
            g[r.randrange(len(g))] = r.randrange(256)
        if r.random() < 0.5:
            g = g[: r.randrange(len(g) + 1)]
        lines.append("P " + bytes(g).hex())
        expect.append(("X", None))

    # jtext: valid text both ways against Python's codecs, then damaged bytes (must stay valid UTF-16)
    cps = []
    for _ in range(r.randrange(0, 40)):
        k = r.randrange(5)
        cps.append(r.randrange(32, 127) if k == 0 else r.randrange(128, 2048) if k == 1 else
                   r.choice([r.randrange(2048, 55296), r.randrange(57344, 65534)]) if k == 2 else
                   r.randrange(65536, 1114112) if k == 3 else r.choice([10, 0x20, 0x1F600, 0xFFFD]))
    txt = "".join(map(chr, cps))
    u8, u16 = txt.encode("utf-8"), txt.encode("utf-16-le")
    lines.append("U " + u8.hex()); expect.append(("U", "%d %s" % (len(u16) // 2, u16.hex())))
    lines.append("V " + u16.hex()); expect.append(("V", "%d %s" % (len(u8), u8.hex())))
    units = bytes(r.randrange(256) for _ in range(2 * r.randrange(0, 20)))
    # random units, many of them surrogates
    ub = bytearray()
    for _ in range(r.randrange(0, 24)):
        v = r.choice([r.randrange(0xD800, 0xE000), r.randrange(0, 0xD800), r.randrange(0xE000, 0x10000)])
        ub += v.to_bytes(2, "little")
    ref = bytes(ub).decode("utf-16-le", "replace").encode("utf-8")
    lines.append("V " + bytes(ub).hex()); expect.append(("V", "%d %s" % (len(ref), ref.hex())))
    bad8 = bytes(r.randrange(256) for _ in range(r.randrange(0, 30)))
    lines.append("U " + bad8.hex()); expect.append(("W", None))

res = subprocess.run([tool], input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=600)
if res.returncode != 0:
    print("tool exit", res.returncode, res.stderr[:200]); print("mismatches: 1"); sys.exit(1)
got = res.stdout.split("\n")
bad = 0
if len(got) - 1 != len(expect):
    print("answer count", len(got) - 1, "want", len(expect)); bad += 1
for i, (kind, want) in enumerate(expect):
    if i >= len(got): break
    if kind == "X":
        if not (got[i] == "NO" or got[i].startswith("OK ")): bad += 1; print("damaged answer?", got[i][:40])
    elif kind == "W":
        try:
            n_, h_ = got[i].split(" ") if " " in got[i] else (got[i], "")
            bytes.fromhex(h_).decode("utf-16-le", "strict")
            if int(n_) * 4 != len(h_): raise ValueError("length")
        except Exception as e:
            bad += 1; print("damaged utf8 gave invalid utf16", got[i][:60], e)
    elif got[i] != want:
        bad += 1
        if bad < 6: print("mismatch", kind, "got", got[i][:80], "want", want[:80])
print("lines: %d  mismatches: %d" % (len(expect), bad))
sys.exit(1 if bad else 0)
