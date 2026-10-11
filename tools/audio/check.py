#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/audio/check.py -- lib/audio sine and mixer against an independent
# Python implementation, BIT FOR BIT.
#
#   usage: check.py <mixcheck binary>
#
#  * audio.synth: a CORDIC rewritten with Python integers; the 12000 + 1000
#    octets must be equal; and the result is compared with math.sin
#    (the maximum error must be <= 2 steps of 16 bit)
#  * audio.mixer: the scenario of mixcheck_main.fi (mono loop with pan, stereo
#    with balance, a clipping voice, a ring stream that ends, volume/pan/stop
#    between blocks, master volume) computed here with the documented formulas.
import math, os, struct, subprocess, sys, tempfile, zlib

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

ATAN = [536870912, 316933406, 167458907, 85004756, 42667331, 21354465, 10679838, 5340245, 2670163, 1335087,
        667544, 333772, 166886, 83443, 41722, 20861, 10430, 5215, 2608, 1304, 652, 326, 163, 81]
K = 652032874

def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q

def sine_q30(phase):
    quad = phase >> 30
    z = phase & 0x3FFFFFFF
    x, y = K, 0
    for i in range(24):
        dx, dy = y >> i, x >> i
        if z >= 0:
            x, y, z = x - dx, y + dy, z - ATAN[i]
        else:
            x, y, z = x + dx, y - dy, z + ATAN[i]
    return [y, x, -y, -x][quad]

def sine(frames, ch, freq, rate, amp, phase):
    step = (freq << 32) // rate
    out = []
    ph = phase
    for _ in range(frames):
        v = tdiv(sine_q30(ph) * amp, 1 << 30)
        out += [v] * ch
        ph = (ph + step) & 0xFFFFFFFF
    return out, ph

def pack(vals):
    return struct.pack("<%dh" % len(vals), *vals)

def gain(percent):
    return min(percent, 400) * 65536 // 100

def clamp16(v):
    return max(-32768, min(32767, v))

def mono_gains(vol, pan):
    g = gain(vol)
    return tdiv(g * (100 - pan), 200), tdiv(g * (100 + pan), 200)

def stereo_gains(vol, pan):
    g = gain(vol)
    gl = tdiv(g * (100 - pan), 100) if pan > 0 else g
    gr = tdiv(g * (100 + pan), 100) if pan < 0 else g
    return gl, gr

mixcheck = sys.argv[1]
td = tempfile.mkdtemp(prefix="audio-mix-")
r = subprocess.run([mixcheck, td], capture_output=True, text=True, timeout=60)
check("mixcheck runs", r.returncode == 0, r.stderr)
crc = dict(kv.split("=") for kv in r.stdout.split()[1:]) if r.stdout.startswith("CRC") else {}

# ---- sine
ph = 0
ref = []
for _ in range(3):
    o, ph = sine(1000, 2, 440, 48000, 20000, ph)
    ref += o
sine_ref = pack(ref)
sine_got = open(os.path.join(td, "sine.pcm"), "rb").read()
check("sine stereo (3 calls, phase continues): bit for bit", sine_got == sine_ref)
mono_ref = pack(sine(500, 1, 1000, 48000, 32767, 1073741824)[0])
mono_got = open(os.path.join(td, "sine_mono.pcm"), "rb").read()
check("sine mono, start phase 90 degrees: bit for bit", mono_got == mono_ref)
check("sine CRC-32 equals zlib's", str(zlib.crc32(sine_got)) == crc.get("sine") and str(zlib.crc32(mono_got)) == crc.get("sine_mono"))
# accuracy against math.sin
step = (440 << 32) // 48000
worst = 0
for n in range(3000):
    p = (step * n) & 0xFFFFFFFF
    exact = math.sin(2 * math.pi * p / 2 ** 32) * 20000
    got = ref[n * 2] if n < 3000 else 0
    worst = max(worst, abs(got - exact))
check("sine error against math.sin <= 1.01 steps (max %.3f)" % worst, worst <= 1.01)
worst_q = max(abs(sine_q30(p << 20) / 2 ** 30 - math.sin(2 * math.pi * (p << 20) / 2 ** 32)) for p in range(4096))
check("sine_q30 error <= 2^-21 (max %.2e)" % worst_q, worst_q <= 2 ** -21)

# ---- mixer scenario
A = [((i * 37) % 2001 - 1000) * 20 for i in range(1500)]
B = [(((i * 91) % 3001 - 1500) * 10, ((i * 53) % 2501 - 1250) * 12) for i in range(700)]
C = [30000] * 3000
def wrap16(v):
    v &= 0xFFFF
    return v - 65536 if v >= 32768 else v
D = [(wrap16(i * 20), wrap16(65536 - (i * 20) % 65536)) for i in range(1500)]

master = gain(90)
state = {"v1": (100, -50), "c_alive": True}
out = []
for f in range(3000):
    if f == 1000:
        state["v1"] = (30, 100)
    if f == 1777:
        state["c_alive"] = False
    accL = accR = 0
    # v1: mono loop
    gl, gr = mono_gains(*state["v1"])
    s = A[f % 1500]
    accL += tdiv(s * gl, 65536); accR += tdiv(s * gr, 65536)
    # v2: stereo, one shot 700 frames
    if f < 700:
        gl, gr = stereo_gains(150, 30)
        accL += tdiv(B[f][0] * gl, 65536); accR += tdiv(B[f][1] * gr, 65536)
    # v3: mono constant, stopped at 1777
    if state["c_alive"]:
        gl, gr = mono_gains(200, 0)
        accL += tdiv(C[f] * gl, 65536); accR += tdiv(C[f] * gr, 65536)
    # v4: stream of 1500 frames, vol 80, pan -100
    if f < 1500:
        gl, gr = stereo_gains(80, -100)
        accL += tdiv(D[f][0] * gl, 65536); accR += tdiv(D[f][1] * gr, 65536)
    out += [clamp16(tdiv(accL * master, 65536)), clamp16(tdiv(accR * master, 65536))]
mix_ref = pack(out)
mix_got = open(os.path.join(td, "mix.pcm"), "rb").read()
check("mixer scenario (4 voices, pan, balance, clip, stream, changes between blocks): bit for bit", mix_got == mix_ref,
      next(((i, a, b) for i, (a, b) in enumerate(zip(struct.unpack("<6000h", mix_got), out)) if a != b), None))
check("the scenario really clips", any(abs(v) in (32767, 32768) for v in out))
check("mix CRC-32 equals zlib's", str(zlib.crc32(mix_got)) == crc.get("mix"))
check("no underruns, one voice (the loop) left", crc.get("underruns") == "0" and crc.get("voices_left") == "1", crc)
print("SINE_CRC=%s SINE_MONO_CRC=%s MIX_CRC=%s" % (zlib.crc32(sine_got), zlib.crc32(mono_got), zlib.crc32(mix_got)))
print("FAILED: %d" % len(FAILED) if FAILED else "ALL OK")
sys.exit(1 if FAILED else 0)
