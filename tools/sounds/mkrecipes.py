#!/usr/bin/env python3
"""tools/sounds/mkrecipes.py -- writes the sound recipes tools/sounds/mc.json and tools/sounds/orientos.json.

The JSON files are the source of truth for tools/sounds/gen (they are committed); this script only expands the
material / tone templates so that the 200+ recipes stay consistent. Re-run after changing a template:
    python3 tools/sounds/mkrecipes.py
Everything is synthesised (own IP, license 'own'); no Mojang sound is copied or imitated sample by sample.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def R(lo, hi):
    return {"rand": [lo, hi]}


def SW(a, b, ms=0, curve=0, log=True):
    d = {"sweep": [a, b], "curve": curve, "log": log}
    if ms:
        d["ms"] = ms
    return d


def noise(i, color="white"):
    return {"id": i, "type": "noise", "color": color}


def adsr(i, a, d, s=0, r=5, at=0, hold=0, dc=-3, peak=1):
    n = {"id": i, "type": "adsr", "a": a, "d": d, "s": s, "r": r, "dc": dc, "rc": -3, "peak": peak}
    if at:
        n["at"] = at
    if hold:
        n["hold"] = hold
    return n


def filt(i, kind, src, cutoff, q=0.7):
    return {"id": i, "type": "filter", "kind": kind, "in": src, "cutoff": cutoff, "q": q}


def gain(i, src, g):
    return {"id": i, "type": "gain", "in": src, "gain": g}


def mix(i, items, g=1.0):
    return {"id": i, "type": "mix", "in": [{"src": s, "gain": gg} for s, gg in items], "gain": g}


def osc(i, wave, freq, **kw):
    n = {"id": i, "type": "osc", "wave": wave, "freq": freq}
    n.update(kw)
    return n


def sound(sid, dur, seed, nodes, out, variants=1, peak=-3, bus="sfx", aliases=None, fi=1, fo=8, ev=None, **kw):
    e = {"bus": bus, "volume": 1.0}
    if ev:
        e.update(ev)
    s = {"id": sid, "duration_ms": dur, "seed": seed, "variants": variants, "peak_db": peak, "fade_in_ms": fi,
         "fade_out_ms": fo, "nodes": nodes, "out": out, "event": e}
    if aliases:
        s["aliases"] = aliases
    s.update(kw)
    return s


BUSES_MC = [
    {"name": "master", "volume": 1.0},
    {"name": "music", "parent": "master", "volume": 0.6},
    {"name": "sfx", "parent": "master", "volume": 0.9},
    {"name": "ui", "parent": "master", "volume": 0.8},
]

# ------------------------------------------------------------------------------------------------ block sounds
# material: noise layer (color, filter kind, cutoff, q, decay ms, gain, grains), body (f0, f1, decay, gain),
# partials [(freq, gain, decay)]
MATS = {
    "stone": dict(nz=("white", "bp", 1500, 1.2, 55, 1.0, 0), body=(140, 70, 70, 0.8), parts=[(310, 0.15, 40)]),
    "deepslate": dict(nz=("pink", "bp", 900, 1.0, 70, 1.0, 0), body=(95, 50, 100, 1.0), parts=[(210, 0.12, 60)]),
    "wood": dict(nz=("white", "bp", 1800, 2.5, 30, 0.5, 0), body=(330, 220, 60, 1.0), parts=[(660, 0.25, 40), (980, 0.1, 25)]),
    "gravel": dict(nz=("white", "lp", 4200, 0.7, 18, 1.0, 5), body=(90, 60, 50, 0.3), parts=[]),
    "grass": dict(nz=("pink", "bp", 3500, 0.6, 70, 0.9, 0), body=None, parts=[], attack=12),
    "sand": dict(nz=("white", "bp", 5000, 0.5, 110, 0.6, 3), body=None, parts=[], attack=6),
    "snow": dict(nz=("pink", "bp", 2200, 0.8, 90, 0.8, 3), body=None, parts=[], attack=8),
    "glass": dict(nz=("white", "hp", 4000, 0.7, 12, 0.4, 0), body=None,
                  parts=[(2300, 0.8, 180), (3470, 0.5, 140), (5130, 0.35, 100), (6900, 0.2, 70)]),
    "wool": dict(nz=("brown", "lp", 700, 0.7, 70, 0.8, 0), body=(80, 60, 50, 0.3), parts=[], attack=8),
    "metal": dict(nz=("white", "hp", 3000, 0.7, 8, 0.3, 0), body=None,
                  parts=[(560, 0.7, 260), (1190, 0.5, 200), (1710, 0.35, 150), (2610, 0.25, 100)]),
    "netherrack": dict(nz=("brown", "lp", 1100, 0.7, 60, 1.0, 3), body=(70, 45, 90, 0.7), parts=[]),
    "slime_block": dict(nz=("pink", "lp", 900, 0.7, 80, 0.4, 0), body=(380, 140, 200, 1.0), parts=[], wobble=True),
    "bone_block": dict(nz=("white", "bp", 3000, 3.0, 20, 0.5, 0), body=(220, 160, 40, 0.5), parts=[(780, 0.8, 90), (1900, 0.4, 60)]),
}
# action: decay multiplier, noise gain, body gain, brightness, extra noise layers (offset ms, gain), variants
ACTS = {
    "step": dict(dm=0.8, ng=0.7, bg=0.8, br=1.0, extra=[], var=4, w=3),
    "hit": dict(dm=0.55, ng=0.9, bg=0.7, br=1.3, extra=[], var=3, w=3),
    "place": dict(dm=1.0, ng=0.9, bg=1.0, br=0.95, extra=[], var=3, w=3),
    "break": dict(dm=1.6, ng=1.0, bg=1.2, br=1.0, extra=[(40, 0.6), (85, 0.4)], var=3, w=3),
    "fall": dict(dm=1.3, ng=0.8, bg=1.5, br=0.85, extra=[(25, 0.4)], var=2, w=3),
}


def block(mat, act):
    m = MATS[mat]
    a = ACTS[act]
    nodes = []
    mixin = []
    maxend = 0
    color, kind, cut, q, dec, ng, grains = m["nz"]
    cut = cut * a["br"]
    att = m.get("attack", 1.5)
    layers = [(0, 1.0)] + a["extra"]
    for li, (off, lg) in enumerate(layers):
        p = "n%d" % li
        nodes.append(noise(p, color))
        nodes.append(filt(p + "f", kind, p, R(cut * 0.85, cut * 1.15), q))
        d = dec * a["dm"]
        if grains and li == 0:
            # ascending times: only the first grain starts at t, later ones start after the previous ends
            pts = [[0, 0, 0]]
            for g in range(grains):
                t = 3 + g * (d * 0.9 + 4)
                pts += [[t, 0, 0], [t + 2, 1.0 - 0.1 * g, 0], [t + 2 + d, 0, -3]]
            nodes.append({"id": p + "e", "type": "env", "points": pts})
            maxend = max(maxend, pts[-1][0] + 10)
        else:
            nodes.append(adsr(p + "e", att, d, at=off, r=4))
            maxend = max(maxend, off + att + d * 3 + 6)
        nodes.append(gain(p + "g", p + "f", p + "e"))
        mixin.append((p + "g", ng * a["ng"] * lg))
    if m["body"]:
        f0, f1, bd, bgain = m["body"]
        bd = bd * a["dm"]
        bf = 0.85 if act == "fall" else 1.0
        fr = SW(f0 * bf, f1 * bf, bd * 0.7, -2)
        if m.get("wobble"):
            nodes.append(osc("lf", "sine", R(18, 26), scale=0.08))
            nodes.append(osc("bo", "sine", fr, pm="lf", pm_depth=1.0))
            nodes.append(adsr("be", 3, bd, r=10, dc=-2.5))
        else:
            nodes.append(osc("bo", "sine", fr))
            nodes.append(adsr("be", 1.5, bd, r=6))
        nodes.append(gain("bg", "bo", "be"))
        mixin.append(("bg", bgain * a["bg"]))
        maxend = max(maxend, 1.5 + bd * 3 + 8)
    for k, (pf, pg, pd) in enumerate(m["parts"]):
        pd = pd * a["dm"]
        pfq = R(pf * 0.97, pf * 1.03)
        nodes.append(osc("pm%d" % k, "sine", pfq))
        nodes.append(adsr("pe%d" % k, 0.5, pd, r=5, dc=-3))
        nodes.append(gain("pg%d" % k, "pm%d" % k, "pe%d" % k))
        mixin.append(("pg%d" % k, pg))
        maxend = max(maxend, 0.5 + pd * 3 + 6)
        if act == "break" and mat in ("glass", "metal", "bone_block"):
            # second, later ping: shattering / ringing
            nodes.append(osc("pq%d" % k, "sine", R(pf * 1.1, pf * 1.6)))
            nodes.append(adsr("pw%d" % k, 0.5, pd * 0.8, at=R(25, 70), r=5))
            nodes.append(gain("ph%d" % k, "pq%d" % k, "pw%d" % k))
            mixin.append(("ph%d" % k, pg * 0.5))
            maxend += 80
    nodes.append(mix("out", mixin))
    dur = int(min(2500, max(60, maxend)))
    return sound("block.%s.%s" % (mat, act), dur, 1000 + hash_str(mat) * 10 + list(ACTS).index(act), nodes, "out",
                 variants=a["var"], peak=-3, fo=max(6, dur // 12), aliases=[])


def hash_str(s):
    h = 0
    for c in s:
        h = (h * 31 + ord(c)) % 997
    return h


# ------------------------------------------------------------------------------------------------ ambience, UI, entities
def ambience():
    out = []
    # wind: brown noise through a slowly sweeping band pass; loopable (fades)
    out.append(sound("ambient.wind", 6000, 301, [
        noise("n", "brown"), osc("l", "sine", 0.35, scale=1, offset=0),
        osc("l2", "sine", 0.13, phase=0.3),
        filt("f", "bp", "n", {"src": "l", "base": 520, "depth": 260}, 0.9),
        {"id": "lv", "type": "osc", "wave": "sine", "freq": 0.21, "scale": 0.35, "offset": 0.65},
        {"id": "o", "type": "mul", "in": ["f", "lv"]},
    ], "o", variants=2, peak=-6, fi=500, fo=500, bus="sfx", ev={"loop": True}, aliases=["ambient.weather.wind"]))
    # rain: filtered white noise with fast, shallow amplitude flicker
    out.append(sound("weather.rain", 4000, 302, [
        noise("n", "white"), noise("m", "pink"),
        filt("hp", "hp", "n", 1800, 0.7), filt("lp", "lp", "hp", R(7500, 9500), 0.7),
        filt("ml", "lp", "m", 90, 0.7),
        {"id": "mg", "type": "gain", "in": "ml", "gain": 6.0},
        {"id": "mm", "type": "mix", "in": [{"src": "mg", "gain": 0.6}], "gain": 1.0},
        {"id": "co", "type": "osc", "wave": "sine", "freq": 0.01, "scale": 0.0, "offset": 0.65},
        {"id": "am", "type": "mix", "in": [{"src": "co", "gain": 1.0}, {"src": "mm", "gain": 0.3}]},
        {"id": "o", "type": "mul", "in": ["lp", "am"]},
    ], "o", variants=2, peak=-8, fi=400, fo=400, ev={"loop": True}, bus="sfx", aliases=["weather.rain.above"]))
    # thunder: crack + rumble
    out.append(sound("entity.lightning_bolt.thunder", 3800, 303, [
        noise("c", "white"), filt("cf", "hp", "c", 1500, 0.7), adsr("ce", 1, 40, r=10, dc=-4), gain("cg", "cf", "ce"),
        noise("r", "brown"), filt("rf", "lp", "r", SW(420, 70, 3000, -1), 0.8),
        osc("tl", "sine", 6.5, scale=0.3, offset=0.7),
        {"id": "re", "type": "env", "points": [[0, 0, 0], [60, 1, 0], [3600, 0, -3]]},
        {"id": "rm", "type": "mul", "in": ["rf", "re", "tl"]},
        {"id": "mx", "type": "mix", "in": [{"src": "cg", "gain": 0.7}, {"src": "rm", "gain": 3.0}]},
        {"id": "o", "type": "reverb", "in": "mx", "room": 0.6, "damp": 0.5, "mix": 0.25},
    ], "o", variants=2, peak=-3, fo=300, aliases=["weather.thunder"], ev={"volume": 1.0}))
    # water: little bubbles and a gentle bed
    nodes = [noise("b", "pink"), filt("bf", "lp", "b", 700, 0.7),
             {"id": "bs", "type": "osc", "wave": "sine", "freq": 0.4, "scale": 0.3, "offset": 0.5},
             {"id": "bm", "type": "mul", "in": ["bf", "bs"]}]
    mixin = [("bm", 0.6)]
    for k in range(6):
        t = 100 + k * 330
        nodes += [osc("w%d" % k, "sine", SW(R(380, 520), R(900, 1500), 70, 0, True)),
                  adsr("we%d" % k, 4, R(50, 90), at=t, r=15),
                  gain("wg%d" % k, "w%d" % k, "we%d" % k)]
        mixin.append(("wg%d" % k, 0.45))
    nodes.append(mix("o", mixin))
    out.append(sound("block.water.ambient", 2400, 304, nodes, "o", variants=2, peak=-6, fi=200, fo=200,
                     aliases=["block.water.ambient"][:0]))
    # cave drone
    out.append(sound("ambient.cave", 8000, 305, [
        osc("a", "saw", 55), osc("b", "saw", 55.4), osc("c", "sine", 41.2),
        {"id": "s", "type": "mix", "in": [{"src": "a", "gain": 0.4}, {"src": "b", "gain": 0.4}, {"src": "c", "gain": 0.6}]},
        osc("l", "sine", 0.17),
        filt("f", "lp", "s", {"src": "l", "base": 180, "depth": 90}, 1.5),
        noise("n", "brown"), filt("nf", "lp", "n", 140, 0.8),
        {"id": "m", "type": "mix", "in": [{"src": "f", "gain": 1.0}, {"src": "nf", "gain": 2.0}]},
        {"id": "o", "type": "reverb", "in": "m", "room": 0.9, "damp": 0.6, "mix": 0.4},
    ], "o", variants=1, peak=-8, fi=1200, fo=1200, ev={"loop": True}, aliases=["ambient.cave.cave"][:0]))
    # a single drip
    out.append(sound("ambient.drip", 450, 306, [
        osc("o1", "sine", SW(R(1500, 1900), R(900, 1100), 60, 0, True)), adsr("e1", 1, 90, r=10),
        gain("g1", "o1", "e1"), {"id": "o", "type": "reverb", "in": "g1", "room": 0.5, "damp": 0.4, "mix": 0.25},
    ], "o", variants=3, peak=-6, aliases=["block.pointed_dripstone.drip_water"]))
    return out


def music():
    nodes = [
        {"id": "m1", "type": "notes", "root": 57, "scale": [0, 2, 4, 7, 9], "octaves": 2, "step_ms": 1400, "density": 0.55,
         "attack_ms": 40, "decay_ms": 2600, "h2": 0.25, "h3": 0.08, "gain": 0.28},
        {"id": "m2", "type": "notes", "root": 45, "scale": [0, 7, 9], "octaves": 1, "step_ms": 4000, "density": 0.85,
         "attack_ms": 700, "decay_ms": 4500, "h2": 0.12, "h3": 0.0, "gain": 0.3},
        {"id": "mx", "type": "mix", "in": [{"src": "m1", "gain": 1.0}, {"src": "m2", "gain": 1.0}]},
        {"id": "o", "type": "reverb", "in": "mx", "room": 0.85, "damp": 0.5, "mix": 0.4},
    ]
    return [sound("music.ambient_loop", 32000, 401, nodes, "o", variants=1, peak=-6, bus="music", fi=1500, fo=1500, preview=0,
                  ev={"loop": True}, aliases=["music.game", "music.creative", "music.menu"])]


def uisounds():
    o = []
    o.append(sound("ui.button.click", 90, 501, [
        osc("s", "sine", SW(2200, 1300, 40, 0, True)), adsr("e", 0.5, 30, r=5), gain("sg", "s", "e"),
        noise("n"), filt("nf", "bp", "n", 3500, 2), adsr("ne", 0.3, 8, r=2), gain("ng", "nf", "ne"),
        mix("o", [("sg", 1.0), ("ng", 0.5)])], "o", variants=2, peak=-6, bus="ui", fo=10))
    o.append(sound("ui.hover", 70, 502, [
        osc("s", "sine", 1760), adsr("e", 4, 35, r=8), gain("o", "s", "e")], "o", peak=-14, bus="ui", fo=10))
    o.append(sound("ui.error", 420, 503, [
        osc("a", "square", 220), osc("b", "square", 165),
        adsr("ea", 3, 90, r=15, hold=20), adsr("eb", 3, 120, r=30, at=140, hold=40),
        gain("ga", "a", "ea"), gain("gb", "b", "eb"), mix("m", [("ga", 0.6), ("gb", 0.6)]),
        filt("o", "lp", "m", 1100, 0.8)], "o", peak=-6, bus="ui", fo=40))
    # level up: rising pentatonic arpeggio with a bell-ish tail
    nodes = []
    mixin = []
    for k, m in enumerate([69, 73, 76, 81]):
        f = 440.0 * 2 ** ((m - 69) / 12.0)
        nodes += [osc("a%d" % k, "sine", f), osc("b%d" % k, "sine", f * 2.0), adsr("e%d" % k, 3, 260, at=k * 95, r=40),
                  mix("v%d" % k, [("a%d" % k, 1.0), ("b%d" % k, 0.25)]), gain("g%d" % k, "v%d" % k, "e%d" % k)]
        mixin.append(("g%d" % k, 0.5))
    nodes.append(mix("m", mixin))
    nodes.append({"id": "o", "type": "reverb", "in": "m", "room": 0.5, "damp": 0.3, "mix": 0.2})
    o.append(sound("entity.player.levelup", 900, 504, nodes, "o", peak=-5, bus="sfx", fo=80))
    # xp ping: two quick rising notes, 3 variants with different seed pitch
    o.append(sound("entity.experience_orb.pickup", 260, 505, [
        osc("a", "sine", R(1250, 1500)), osc("b", "sine", R(1880, 2250)),
        adsr("ea", 2, 55, r=10), adsr("eb", 2, 90, at=55, r=20), gain("ga", "a", "ea"), gain("gb", "b", "eb"),
        mix("o", [("ga", 0.7), ("gb", 0.6)])], "o", variants=3, peak=-8, bus="sfx", fo=30))
    return o


def voice(sid, seed, dur, f0, f1, wave, form, fq, vib=0.0, noise_amt=0.15, peak=-4, variants=3, pitch_env=None, am=0.0):
    """A creature-ish voice: saw / pulse source, pitch glide, one band pass formant, breath noise."""
    nodes = [osc("src", wave, SW(R(f0 * 0.93, f0 * 1.07), R(f1 * 0.93, f1 * 1.07), dur, 0, True), width=0.35)]
    if vib:
        nodes[0]["pm"] = "vb"
        nodes[0]["pm_depth"] = 1.0
        nodes.insert(0, osc("vb", "sine", R(5, 8), scale=vib))
    nodes += [filt("fm", "bp", "src", R(form * 0.9, form * 1.1), fq), filt("fl", "lp", "src", form * 3.0, 0.7),
              mix("sm", [("fm", 1.6), ("fl", 0.5)]),
              noise("n", "pink"), filt("nf", "bp", "n", form, 1.2)]
    pts = [[0, 0, 0], [min(30, dur * 0.1), 1, 0], [dur * 0.75, 0.8, 0], [dur, 0, -2]]
    nodes += [{"id": "e", "type": "env", "points": pts}, mix("m2", [("sm", 1.0), ("nf", noise_amt * 3)]),
              gain("o", "m2", "e")]
    return sound(sid, dur, seed, nodes, "o", variants=variants, peak=peak, fo=max(10, int(dur * 0.08)))


def entities():
    o = []
    o.append(voice("entity.zombie.ambient", 601, 800, 110, 80, "saw", 520, 5, vib=0.01, noise_amt=0.2))
    o.append(voice("entity.zombie.hurt", 602, 330, 170, 110, "saw", 650, 4, noise_amt=0.2))
    o.append(voice("entity.zombie.death", 603, 1000, 140, 45, "saw", 480, 4, vib=0.008, noise_amt=0.25))
    o.append(voice("entity.cow.ambient", 611, 1300, 135, 120, "saw", 450, 3, vib=0.006, noise_amt=0.05))
    o.append(voice("entity.cow.hurt", 612, 500, 210, 150, "saw", 520, 3, noise_amt=0.06))
    o.append(voice("entity.cow.death", 613, 900, 180, 70, "saw", 430, 3, vib=0.01, noise_amt=0.06))
    o.append(voice("entity.pig.ambient", 621, 420, 230, 140, "pulse", 900, 4, noise_amt=0.15))
    o.append(voice("entity.pig.hurt", 622, 380, 420, 260, "pulse", 1100, 4, noise_amt=0.12))
    o.append(voice("entity.pig.death", 623, 650, 380, 90, "pulse", 800, 4, noise_amt=0.15))
    # chicken: three short chirps
    nodes = []
    mixin = []
    for k in range(3):
        nodes += [osc("c%d" % k, "pulse", SW(R(560, 700), R(900, 1100), 60, 0, True), width=0.3), adsr("ce%d" % k, 2, 45, at=k * 85, r=8),
                  gain("cg%d" % k, "c%d" % k, "ce%d" % k)]
        mixin.append(("cg%d" % k, 0.6))
    nodes += [mix("m", mixin), filt("o", "bp", "m", 1500, 1.5)]
    o.append(sound("entity.chicken.ambient", 400, 631, nodes, "o", variants=3, peak=-6, fo=30))
    o.append(voice("entity.chicken.hurt", 632, 220, 800, 1200, "pulse", 1800, 2, noise_amt=0.1, variants=2))
    o.append(voice("entity.chicken.death", 633, 500, 900, 300, "pulse", 1500, 2, noise_amt=0.1, variants=2))
    return o


def mc():
    sounds = []
    for mat in MATS:
        for act in ACTS:
            s = block(mat, act)
            sounds.append(s)
    sounds += ambience() + uisounds() + entities() + music()
    return {"format": "synth-recipe-1", "name": "mc", "rate": 44100, "license": "own", "buses": BUSES_MC, "sounds": sounds}


# ------------------------------------------------------------------------------------------------ OrientOS family
# One scale (D major pentatonic: D E F# A B), one timbre (sine + soft 2nd partial + a little FM shimmer, 6 ms attack,
# soft reverb): the "OrientOS family".
N = {"D3": 50, "A3": 57, "D4": 62, "E4": 64, "F#4": 66, "A4": 69, "B4": 71, "D5": 74, "E5": 76, "F#5": 78,
     "A5": 81, "B5": 83, "D6": 86, "E6": 88}


def hz(m):
    return 440.0 * 2 ** ((m - 69) / 12.0)


def seq(sid, seed, notes, dur, peak=-5, rev=0.2, bus="ui", attack=6, decay=None, tail=60, wave="sine", cutoff=None, bell=0.18,
        room=0.45, aliases=None, ev=None, fo=40):
    """notes: [(name, start ms, length ms, gain)]"""
    nodes = []
    mixin = []
    for k, (nm, t, ln, g) in enumerate(notes):
        f = hz(N[nm])
        nodes += [osc("a%d" % k, wave, f)]
        nodes += [osc("b%d" % k, "sine", f * 2.0), osc("m%d" % k, "sine", f * 3.01, scale=1.0)]
        nodes += [adsr("e%d" % k, attack, decay or ln, at=t, r=tail, dc=-3.5)]
        nodes += [mix("v%d" % k, [("a%d" % k, 1.0), ("b%d" % k, 0.22), ("m%d" % k, bell * 0.35)]), gain("g%d" % k, "v%d" % k, "e%d" % k)]
        mixin.append(("g%d" % k, g))
    nodes.append(mix("sum", mixin))
    last = "sum"
    if cutoff:
        nodes.append(filt("lp", "lp", "sum", cutoff, 0.7))
        last = "lp"
    if rev:
        nodes.append({"id": "o", "type": "reverb", "in": last, "room": room, "damp": 0.5, "mix": rev})
    else:
        nodes.append(gain("o", last, 1.0))
    return sound(sid, dur, seed, nodes, "o", peak=peak, bus=bus, fo=fo, aliases=aliases, ev=ev)


def orientos():
    s = []
    s.append(seq("login", 701, [("D5", 0, 280, 0.8), ("F#5", 120, 280, 0.8), ("A5", 240, 320, 0.8), ("D6", 380, 600, 0.9)], 1500, rev=0.3, bus="sfx"))
    s.append(seq("logout", 702, [("D6", 0, 220, 0.8), ("A5", 120, 220, 0.8), ("F#5", 240, 260, 0.8), ("D5", 380, 450, 0.9)], 1200, rev=0.3, bus="sfx"))
    s.append(seq("lock", 703, [("A4", 0, 130, 0.9), ("D4", 110, 240, 1.0)], 520, rev=0.15, cutoff=2500))
    s.append(seq("unlock", 704, [("D4", 0, 130, 0.9), ("A4", 100, 260, 1.0)], 520, rev=0.15, cutoff=3500))
    s.append(seq("notification", 705, [("A5", 0, 160, 0.8), ("D6", 130, 350, 1.0)], 900, rev=0.25))
    s.append(seq("message", 706, [("B5", 0, 110, 0.7), ("E6", 90, 260, 1.0)], 700, rev=0.2))
    s.append(seq("error", 707, [("D4", 0, 150, 1.0), ("D3", 140, 260, 1.0)], 650, rev=0.1, wave="tri", cutoff=1200, bell=0.0, attack=4))
    s.append(seq("warning", 708, [("A4", 0, 160, 0.9), ("F#4", 150, 160, 0.9), ("A4", 300, 280, 1.0)], 800, rev=0.12, wave="tri", cutoff=2200, bell=0.05))
    s.append(seq("success", 709, [("D5", 0, 150, 0.8), ("A5", 90, 150, 0.8), ("D6", 180, 480, 1.0)], 950, rev=0.28))
    # click / hover: very short
    s.append(sound("click", 60, 710, [
        osc("s", "sine", SW(2300, 1500, 25, 0, True)), adsr("e", 0.4, 22, r=4), gain("sg", "s", "e"),
        noise("n"), filt("nf", "bp", "n", 4000, 2.5), adsr("ne", 0.2, 5, r=2), gain("ng", "nf", "ne"),
        mix("o", [("sg", 1.0), ("ng", 0.3)])], "o", peak=-8, bus="ui", fo=8))
    s.append(sound("hover", 50, 711, [osc("s", "sine", hz(N["A5"])), adsr("e", 3, 24, r=6), gain("o", "s", "e")], "o", peak=-16, bus="ui", fo=8))
    s.append(seq("volume_ping", 712, [("A5", 0, 70, 1.0)], 260, rev=0.1, attack=3, tail=40, peak=-8))
    s.append(seq("usb_in", 713, [("E5", 0, 120, 0.9), ("B5", 110, 260, 1.0)], 650, rev=0.2, bus="sfx"))
    s.append(seq("usb_out", 714, [("B5", 0, 120, 0.9), ("E5", 110, 260, 1.0)], 650, rev=0.2, bus="sfx"))
    s.append(seq("mic_on", 715, [("D5", 0, 70, 0.8), ("A5", 70, 120, 1.0)], 380, rev=0.12, bus="sfx", attack=3))
    s.append(seq("mic_off", 716, [("A5", 0, 70, 0.8), ("D5", 70, 120, 1.0)], 380, rev=0.12, bus="sfx", attack=3))
    s.append(seq("shutdown", 717, [("D6", 0, 300, 0.7), ("A5", 220, 300, 0.7), ("F#5", 440, 320, 0.7), ("D5", 660, 400, 0.8),
                                   ("A4", 880, 500, 0.8), ("D4", 1100, 800, 0.9)], 1950, rev=0.35, bus="sfx", fo=300, tail=200, cutoff=4000))
    s.append(seq("boot", 718, [("D4", 0, 700, 0.8), ("A4", 140, 700, 0.7), ("D5", 280, 800, 0.7), ("F#5", 420, 800, 0.6),
                               ("A5", 560, 900, 0.6), ("D6", 700, 1100, 0.7)], 1950, rev=0.4, bus="sfx", fo=300, attack=40, tail=250, room=0.6))
    buses = [{"name": "master", "volume": 1.0}, {"name": "ui", "parent": "master", "volume": 0.8},
             {"name": "sfx", "parent": "master", "volume": 0.9}]
    return {"format": "synth-recipe-1", "name": "orientos", "rate": 44100, "license": "own", "buses": buses, "sounds": s}


def main():
    for name, doc in (("mc", mc()), ("orientos", orientos())):
        # strip the placeholder dropped by helper code paths
        path = os.path.join(HERE, name + ".json")
        with open(path, "w") as f:
            json.dump(doc, f, indent=1, sort_keys=False)
            f.write("\n")
        print("wrote", path, len(doc["sounds"]), "sounds")


if __name__ == "__main__":
    main()
