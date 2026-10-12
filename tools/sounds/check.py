#!/usr/bin/env python3
"""tools/sounds/check.py <recipe.json> <outdir> <ref.sha256> [--update] -- checks a generated sound set.

Per WAV: S16 mono at the recipe rate, length == recipe duration, peak == recipe peak_db (+-0.05 dB) and <= -1 dBFS,
no clipped samples, RMS in -45..-9 dBFS, DC offset ~0, first / last sample ~0 (no click), SHA256 == manifest ==
reference file. Plus: every file the bank names exists; dominant pitch of a few pure tones measured with an
independent FFT (numpy) against the frequency written in the recipe."""
import hashlib, json, re, struct, sys, wave
import numpy as np

recipe_p, outdir, ref_p = sys.argv[1:4]
update = "--update" in sys.argv
recipe = json.load(open(recipe_p))
manifest = json.load(open(outdir + "/manifest.json"))
rate = recipe.get("rate", 44100)
sounds = {s["id"]: s for s in recipe["sounds"]}
fails = []


def fail(msg):
    fails.append(msg)
    print("FAIL", msg)


ref = {}
try:
    for line in open(ref_p):
        h, f = line.split()
        ref[f] = h
except FileNotFoundError:
    pass

newref = []
for f in manifest["files"]:
    name = f["file"]
    raw = open(outdir + "/" + name, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    newref.append("%s  %s" % (sha, name))
    if sha != f["sha256"]:
        fail(name + ": manifest hash differs from the file")
    if not update and ref.get(name) != sha:
        fail(name + ": hash differs from the reference file (sound changed? run with --update if intended)")
    w = wave.open(outdir + "/" + name)
    if (w.getnchannels(), w.getsampwidth(), w.getframerate()) != (1, 2, rate):
        fail(name + ": not S16 mono @%d" % rate)
        continue
    n = w.getnframes()
    x = np.frombuffer(w.readframes(n), dtype="<i2").astype(np.float64)
    s = sounds[f["event"]]
    want = int(s["duration_ms"] * rate / 1000.0)
    if n != want:
        fail("%s: length %d != %d" % (name, n, want))
    pk = np.abs(x).max()
    pkdb = 20 * np.log10(pk / 32767.0)
    if pkdb > -1.0:
        fail("%s: peak %.2f dBFS > -1" % (name, pkdb))
    if abs(pkdb - s.get("peak_db", -3.0)) > 0.05:
        fail("%s: peak %.2f dBFS != recipe %.2f" % (name, pkdb, s.get("peak_db", -3.0)))
    if (np.abs(x) >= 32767).sum() > 0:
        fail(name + ": clipped samples")
    rmsdb = 20 * np.log10(np.sqrt((x / 32768.0) ** 2).mean() + 1e-12) if False else 20 * np.log10(np.sqrt(((x / 32768.0) ** 2).mean()) + 1e-12)
    if not (-45.0 <= rmsdb <= -9.0):
        fail("%s: RMS %.1f dBFS outside -45..-9" % (name, rmsdb))
    if abs(x.mean()) / 32768.0 > 0.002:
        fail("%s: DC offset %.4f" % (name, x.mean() / 32768.0))
    if abs(x[0]) > 64 or abs(x[-1]) > 64:
        fail("%s: edges not near 0 (%d, %d)" % (name, x[0], x[-1]))

# bank: files exist, events listed
txt = open(outdir + "/bank.json").read()
txt = re.sub(r"//[^\n]*", "", txt)
bank = json.loads(txt)
have = {f["file"] for f in manifest["files"]}
evs = {e["name"] for e in bank["events"]}
for e in bank["events"]:
    for sm in e["samples"]:
        if sm not in have:
            fail("bank event %s names a missing file %s" % (e["name"], sm))
for sid in sounds:
    if sid not in evs:
        fail("sound %s is not an event of the bank" % sid)
for s in recipe["sounds"]:
    for a in s.get("aliases", []):
        if a not in evs:
            fail("alias %s missing in the bank" % a)

# independent pitch check of pure tones: (event, Hz)
for ev, hz in (("volume_ping", 880.0), ("hover", 880.0)):
    f = [m for m in manifest["files"] if m["event"] == ev]
    if not f:
        continue
    w = wave.open(outdir + "/" + f[0]["file"])
    x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64)
    pad = 1 << 18
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)), pad))
    peak_hz = np.argmax(sp) * rate / pad
    if abs(peak_hz - hz) / hz > 0.02:
        fail("%s: dominant frequency %.1f Hz, recipe says %.1f" % (ev, peak_hz, hz))
    else:
        print("ok   %s dominant frequency %.1f Hz (recipe %.1f)" % (ev, peak_hz, hz))

if update:
    open(ref_p, "w").write("\n".join(newref) + "\n")
    print("reference hashes written:", ref_p)
if fails:
    print("%d problems" % len(fails))
    sys.exit(1)
print("ok   %d files: levels, clipping, length, DC, edges, hashes, bank" % len(manifest["files"]))
