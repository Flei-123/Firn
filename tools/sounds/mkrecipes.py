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


def voice(sid, seed, dur, f0, f1, wave, form, fq, vib=0.0, noise_amt=0.15, peak=-4, variants=3, pitch_env=None, am=0.0, aliases=None):
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
    return sound(sid, dur, seed, nodes, "o", variants=variants, peak=peak, fo=max(10, int(dur * 0.08)), aliases=aliases)


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


# ------------------------------------------------------------------------------------------------ tier 2 mob voices (r164)
# One own voice per mob and call (ambient / hurt / death) instead of an alias to a related mob. All of them are the `voice()` archetype
# (source -> pitch glide -> formant band pass + breath noise); only the numbers differ, so every mob gets its own timbre without a new
# recipe type. Row = (dur ms, f0 Hz, f1 Hz, wave, formant Hz, formant Q, vibrato, noise). Derived events (_small, _baby, _water, _land,
# attack) are the same row with a factor (pitch, duration, formant) -- listed in MOB_DERIVED.
MOB_VOICES = {
    "husk": {"ambient": (900, 100, 75, "saw", 480, 5, 0.012, 0.3), "hurt": (340, 160, 100, "saw", 600, 4, 0.0, 0.3),
             "death": (1100, 130, 40, "saw", 440, 4, 0.01, 0.35)},
    "zombie_villager": {"ambient": (950, 150, 110, "saw", 700, 5, 0.02, 0.15), "hurt": (360, 220, 150, "saw", 800, 4, 0.0, 0.15),
                        "death": (1100, 190, 70, "saw", 650, 4, 0.012, 0.2)},
    "drowned": {"ambient": (1000, 95, 70, "saw", 380, 4, 0.02, 0.35), "hurt": (380, 150, 95, "saw", 450, 3, 0.0, 0.35),
                "death": (1200, 120, 35, "saw", 360, 3, 0.015, 0.4)},
    "horse": {"ambient": (1100, 330, 220, "saw", 900, 3, 0.03, 0.12), "hurt": (600, 700, 420, "saw", 1300, 3, 0.02, 0.1),
              "death": (1300, 520, 140, "saw", 1000, 3, 0.03, 0.12)},
    "donkey": {"ambient": (1300, 260, 520, "saw", 1100, 3, 0.02, 0.1), "hurt": (550, 600, 380, "saw", 1200, 3, 0.02, 0.1),
               "death": (1200, 480, 120, "saw", 1000, 3, 0.03, 0.1)},
    "mule": {"ambient": (1200, 240, 420, "saw", 1000, 3, 0.02, 0.1), "hurt": (520, 540, 340, "saw", 1100, 3, 0.02, 0.1),
             "death": (1150, 430, 110, "saw", 950, 3, 0.03, 0.1)},
    "llama": {"ambient": (800, 300, 380, "saw", 1500, 3, 0.04, 0.08), "hurt": (400, 520, 300, "saw", 1700, 3, 0.02, 0.08),
              "death": (900, 420, 130, "saw", 1400, 3, 0.03, 0.1)},
    "polar_bear": {"ambient": (1300, 90, 70, "saw", 300, 3, 0.01, 0.15), "hurt": (500, 150, 90, "saw", 380, 3, 0.0, 0.18),
                   "death": (1300, 110, 35, "saw", 320, 3, 0.012, 0.2)},
    "hoglin": {"ambient": (800, 85, 120, "saw", 360, 4, 0.02, 0.25), "hurt": (400, 160, 85, "saw", 500, 3, 0.0, 0.25),
               "death": (1000, 120, 40, "saw", 380, 3, 0.015, 0.3)},
    "sheep": {"ambient": (900, 380, 340, "saw", 1200, 4, 0.08, 0.05), "hurt": (450, 520, 360, "saw", 1300, 4, 0.06, 0.05),
              "death": (800, 430, 160, "saw", 1100, 4, 0.07, 0.06)},
    "piglin": {"ambient": (550, 200, 150, "pulse", 700, 4, 0.0, 0.2), "hurt": (400, 330, 200, "pulse", 900, 4, 0.0, 0.18),
               "death": (800, 260, 70, "pulse", 650, 4, 0.0, 0.2)},
    "parrot": {"ambient": (350, 900, 1300, "pulse", 2200, 2, 0.04, 0.05), "hurt": (200, 1500, 1000, "pulse", 2500, 2, 0.0, 0.05),
               "death": (500, 1300, 500, "pulse", 2000, 2, 0.02, 0.05)},
    "wither_skeleton": {"ambient": (900, 80, 65, "saw", 600, 5, 0.01, 0.25), "hurt": (350, 140, 90, "saw", 750, 5, 0.0, 0.3),
                        "death": (1000, 110, 45, "saw", 550, 5, 0.01, 0.3)},
    "silverfish": {"ambient": (200, 1400, 1700, "pulse", 3000, 3, 0.0, 0.3), "hurt": (150, 1800, 1200, "pulse", 3200, 3, 0.0, 0.3),
                   "death": (350, 1500, 600, "pulse", 2800, 3, 0.0, 0.3)},
    "bat": {"ambient": (140, 3400, 2600, "pulse", 4500, 4, 0.0, 0.05), "hurt": (160, 3800, 2400, "pulse", 4800, 4, 0.0, 0.05),
            "death": (300, 3200, 900, "pulse", 4200, 4, 0.0, 0.05)},
    "bee": {"hurt": (260, 260, 210, "saw", 900, 3, 0.04, 0.2), "death": (500, 280, 90, "saw", 800, 3, 0.05, 0.2)},
    "blaze": {"ambient": (1100, 120, 160, "saw", 800, 2, 0.02, 0.6), "hurt": (400, 220, 120, "saw", 1100, 2, 0.0, 0.6),
              "death": (1200, 180, 50, "saw", 700, 2, 0.02, 0.7)},
    "cat": {"ambient": (700, 560, 880, "saw", 1700, 4, 0.03, 0.08), "hurt": (350, 900, 520, "saw", 2000, 3, 0.0, 0.1),
            "death": (600, 800, 250, "saw", 1500, 3, 0.02, 0.1)},
    "ocelot": {"ambient": (600, 480, 700, "saw", 1500, 4, 0.03, 0.1), "hurt": (320, 800, 480, "saw", 1800, 3, 0.0, 0.12),
               "death": (550, 700, 220, "saw", 1400, 3, 0.02, 0.12)},
    "cod": {"ambient": (200, 500, 420, "pulse", 1000, 2, 0.0, 0.3), "hurt": (160, 700, 450, "pulse", 1200, 2, 0.0, 0.3),
            "death": (320, 600, 200, "pulse", 900, 2, 0.0, 0.3)},
    "salmon": {"ambient": (220, 460, 400, "pulse", 950, 2, 0.0, 0.3), "hurt": (170, 650, 420, "pulse", 1100, 2, 0.0, 0.3),
               "death": (340, 560, 190, "pulse", 880, 2, 0.0, 0.3)},
    "dolphin": {"ambient": (500, 2600, 3600, "sine", 3500, 3, 0.04, 0.05), "ambient_water": (600, 2200, 3200, "sine", 3000, 3, 0.05, 0.08),
                "hurt": (300, 3500, 1800, "sine", 3600, 3, 0.0, 0.05), "death": (700, 3000, 700, "sine", 3000, 3, 0.03, 0.06),
                "attack": (260, 2800, 3900, "sine", 3800, 3, 0.0, 0.08)},
    "ender_dragon": {"ambient": (1800, 75, 55, "saw", 280, 2, 0.015, 0.35), "hurt": (800, 130, 65, "saw", 320, 2, 0.0, 0.4),
                     "death": (2500, 100, 28, "saw", 260, 2, 0.02, 0.45)},
    "enderman": {"ambient": (900, 110, 240, "saw", 500, 3, 0.05, 0.2), "hurt": (450, 520, 180, "saw", 900, 3, 0.06, 0.3),
                 "death": (1200, 300, 60, "saw", 620, 3, 0.07, 0.3)},
    "fox": {"ambient": (350, 700, 1000, "saw", 2000, 4, 0.02, 0.1), "hurt": (250, 1100, 650, "saw", 2400, 3, 0.0, 0.12),
            "death": (500, 950, 300, "saw", 2000, 3, 0.02, 0.12)},
    "ghast": {"ambient": (1400, 450, 800, "saw", 1500, 3, 0.05, 0.12), "hurt": (600, 900, 450, "saw", 1700, 3, 0.04, 0.12),
              "death": (1800, 800, 150, "saw", 1400, 3, 0.06, 0.15)},
    "guardian": {"ambient": (1200, 130, 95, "pulse", 380, 3, 0.03, 0.25), "ambient_land": (1000, 160, 110, "pulse", 520, 3, 0.02, 0.3),
                 "attack": (900, 90, 300, "saw", 420, 3, 0.03, 0.2), "hurt": (400, 210, 120, "pulse", 450, 3, 0.0, 0.25),
                 "hurt_land": (350, 240, 140, "pulse", 560, 3, 0.0, 0.3), "death": (1300, 160, 40, "pulse", 380, 3, 0.02, 0.3),
                 "death_land": (1100, 180, 60, "pulse", 520, 3, 0.02, 0.35)},
    "iron_golem": {"hurt": (350, 130, 90, "pulse", 600, 4, 0.0, 0.1), "death": (900, 100, 45, "pulse", 500, 4, 0.0, 0.12),
                   "attack": (300, 80, 140, "saw", 700, 4, 0.0, 0.15)},
    "magma_cube": {"hurt": (260, 90, 60, "pulse", 300, 2, 0.0, 0.5), "death": (500, 80, 40, "pulse", 260, 2, 0.0, 0.5)},
    "panda": {"ambient": (800, 240, 200, "saw", 800, 3, 0.04, 0.1), "hurt": (500, 420, 260, "saw", 1000, 3, 0.03, 0.1),
              "death": (1000, 340, 90, "saw", 800, 3, 0.05, 0.12)},
    "phantom": {"ambient": (1200, 280, 380, "saw", 1100, 3, 0.07, 0.3), "hurt": (500, 500, 300, "saw", 1300, 3, 0.05, 0.3),
                "death": (1200, 420, 90, "saw", 1000, 3, 0.07, 0.35)},
    "pillager": {"ambient": (800, 160, 130, "saw", 700, 4, 0.02, 0.15), "hurt": (380, 250, 160, "saw", 850, 4, 0.0, 0.15),
                 "death": (900, 210, 70, "saw", 700, 4, 0.02, 0.18)},
    "rabbit": {"ambient": (200, 1000, 1200, "pulse", 2300, 3, 0.0, 0.15), "hurt": (200, 1500, 900, "pulse", 2600, 3, 0.0, 0.15),
               "death": (400, 1300, 400, "pulse", 2200, 3, 0.0, 0.15), "attack": (150, 800, 1600, "pulse", 2600, 3, 0.0, 0.2)},
    "slime": {"hurt": (240, 220, 140, "pulse", 520, 2, 0.06, 0.2), "death": (450, 200, 80, "pulse", 440, 2, 0.08, 0.2),
              "attack": (200, 150, 300, "pulse", 560, 2, 0.04, 0.2)},
    "snow_golem": {"ambient": (500, 360, 300, "pulse", 1100, 3, 0.02, 0.35), "hurt": (250, 500, 320, "pulse", 1300, 3, 0.0, 0.35),
                   "death": (600, 400, 140, "pulse", 1000, 3, 0.02, 0.4)},
    "squid": {"ambient": (500, 220, 180, "sine", 600, 2, 0.06, 0.3), "hurt": (280, 340, 220, "sine", 800, 2, 0.04, 0.3),
              "death": (700, 280, 90, "sine", 600, 2, 0.06, 0.35)},
    "strider": {"ambient": (500, 1100, 1500, "saw", 2200, 3, 0.05, 0.15), "hurt": (300, 1500, 900, "saw", 2400, 3, 0.0, 0.15),
                "death": (700, 1300, 350, "saw", 2000, 3, 0.04, 0.18)},
    "turtle": {"ambient_land": (500, 320, 260, "pulse", 800, 3, 0.04, 0.2), "hurt": (350, 420, 260, "pulse", 900, 3, 0.0, 0.2),
               "death": (800, 340, 110, "pulse", 750, 3, 0.03, 0.22)},
    "villager": {"ambient": (500, 200, 240, "pulse", 1000, 3, 0.03, 0.12), "hurt": (350, 300, 200, "pulse", 1200, 3, 0.0, 0.12),
                 "death": (900, 250, 90, "pulse", 950, 3, 0.03, 0.15)},
    "witch": {"ambient": (900, 220, 330, "saw", 1300, 3, 0.07, 0.1), "hurt": (500, 420, 260, "saw", 1500, 3, 0.05, 0.1),
              "death": (1000, 350, 110, "saw", 1200, 3, 0.07, 0.12)},
    "wither": {"ambient": (1400, 85, 60, "saw", 340, 2, 0.02, 0.4), "hurt": (550, 150, 80, "saw", 420, 2, 0.0, 0.45),
               "death": (2200, 110, 30, "saw", 300, 2, 0.02, 0.5)},
    "wolf": {"ambient": (600, 260, 190, "saw", 800, 3, 0.04, 0.12), "hurt": (400, 600, 340, "saw", 1100, 3, 0.02, 0.12),
             "death": (900, 450, 130, "saw", 900, 3, 0.04, 0.15)},
}
# new event -> (mob, call, pitch factor, duration factor, formant factor)
MOB_DERIVED = {
    "entity.drowned.ambient_water": ("drowned", "ambient", 0.9, 1.0, 0.55), "entity.drowned.hurt_water": ("drowned", "hurt", 0.9, 1.0, 0.55),
    "entity.drowned.death_water": ("drowned", "death", 0.9, 1.0, 0.55),
    "entity.magma_cube.hurt_small": ("magma_cube", "hurt", 1.6, 0.7, 1.2), "entity.magma_cube.death_small": ("magma_cube", "death", 1.6, 0.7, 1.2),
    "entity.slime.hurt_small": ("slime", "hurt", 1.6, 0.7, 1.2), "entity.slime.death_small": ("slime", "death", 1.6, 0.7, 1.2),
    "entity.turtle.hurt_baby": ("turtle", "hurt", 1.7, 0.7, 1.3), "entity.turtle.death_baby": ("turtle", "death", 1.7, 0.7, 1.3),
    "entity.polar_bear.ambient_baby": ("polar_bear", "ambient", 1.8, 0.6, 1.4),
    "entity.hoglin.attack": ("hoglin", "hurt", 0.8, 0.8, 0.9),
    "entity.zombie.attack_wooden_door": ("zombie", "hurt", 0.7, 0.8, 0.8), "entity.zombie.attack_iron_door": ("zombie", "hurt", 0.55, 0.9, 1.2),
}


def mob_voices():
    o = []
    seed = 12000
    for mob in sorted(MOB_VOICES):
        for call in sorted(MOB_VOICES[mob]):
            dur, f0, f1, wave, form, fq, vib, nz = MOB_VOICES[mob][call]
            o.append(voice("entity.%s.%s" % (mob, call), seed, dur, f0, f1, wave, form, fq, vib=vib, noise_amt=nz, variants=2))
            seed += 3
    base = dict(MOB_VOICES)
    base["zombie"] = {"hurt": (330, 170, 110, "saw", 650, 4, 0.0, 0.2)}
    for sid in sorted(MOB_DERIVED):
        mob, call, pf, df, ff = MOB_DERIVED[sid]
        dur, f0, f1, wave, form, fq, vib, nz = base[mob][call]
        o.append(voice(sid, seed, int(dur * df), f0 * pf, f1 * pf, wave, form * ff, fq, vib=vib, noise_amt=nz, variants=2))
        seed += 3
    # strider steps (lava: bubbling squelch), tier 2 steps that had no sound at all
    o.append(perc("entity.strider.step", seed, 150, nz=[("pink", "bp", 1200, 0.9, 30, 0.8, 0)], body=(160, 90, 55, 0.8, 0), var=2, peak=-10))
    seed += 3
    o.append(crackle("entity.strider.step_lava", seed, 260, 8, 900, color="pink", dec=18, var=2, peak=-10))
    return o


# ------------------------------------------------------------------------------------------------ interaction / effect sounds
# Archetypes (all synthesised): perc = noise bursts + sine body + ringing partials; whoosh = swept band noise; crackle = random
# grains; door = creak (saw through a band pass) + latch click / thud. Gaps in the id table: tools/mc/audio/coverage.py (tier 1).
def perc(sid, seed, dur, nz=(), body=None, parts=(), var=2, peak=-4, aliases=None, bus="sfx", fo=None):
    """nz = [(color, kind, cut, q, decay_ms, gain, at_ms)], body = (f0, f1, decay_ms, gain, at_ms), parts = [(freq, gain, decay_ms, at_ms)]"""
    nodes, mixin = [], []
    for i, (color, kind, cut, q, dec, g, at) in enumerate(nz):
        nodes += [noise("n%d" % i, color), filt("nf%d" % i, kind, "n%d" % i, R(cut * 0.88, cut * 1.12), q),
                  adsr("ne%d" % i, 0.8, dec, r=3, at=at), gain("ng%d" % i, "nf%d" % i, "ne%d" % i)]
        mixin.append(("ng%d" % i, g))
    if body:
        f0, f1, dec, g, at = body
        nodes += [osc("bo", "sine", SW(f0, f1, dec * 0.8, -2)), adsr("be", 1.2, dec, r=6, at=at), gain("bg", "bo", "be")]
        mixin.append(("bg", g))
    for i, (f, g, dec, at) in enumerate(parts):
        nodes += [osc("pm%d" % i, "sine", R(f * 0.98, f * 1.02)), adsr("pe%d" % i, 0.5, dec, r=5, at=at), gain("pg%d" % i, "pm%d" % i, "pe%d" % i)]
        mixin.append(("pg%d" % i, g))
    nodes.append(mix("o", mixin))
    return sound(sid, dur, seed, nodes, "o", variants=var, peak=peak, bus=bus, fo=fo or max(6, dur // 10), aliases=aliases)


def whoosh(sid, seed, dur, c0, c1, color="white", kind="bp", q=1.0, att=15, var=2, peak=-6, aliases=None, level=1.0, bus="sfx", sub=None):
    """band noise with a swept cutoff; sub = (f0, f1) adds a sine sweep underneath (explosions, falls)"""
    nodes = [noise("n", color), filt("nf", kind, "n", SW(c0, c1, dur, 0, True), q),
             {"id": "e", "type": "env", "points": [[0, 0, 0], [att, 1, 0], [dur * 0.55, 0.7, 0], [dur, 0, -2]]}, gain("ng", "nf", "e")]
    mixin = [("ng", level)]
    if sub:
        nodes += [osc("so", "sine", SW(sub[0], sub[1], dur * 0.8, -2)), adsr("se", 3, dur * 0.4, r=20), gain("sg", "so", "se")]
        mixin.append(("sg", 0.9))
    nodes.append(mix("o", mixin))
    return sound(sid, dur, seed, nodes, "o", variants=var, peak=peak, bus=bus, fo=max(8, dur // 12), aliases=aliases)


def crackle(sid, seed, dur, n, cut, color="white", dec=14, var=2, peak=-8, aliases=None, kind="bp", q=1.0, bus="sfx", pitch=None):
    """n random grains (fixed per recipe seed): fire, bone rattle, crunch, lava pops; pitch = sine blip per grain (bubbles)"""
    import random
    rnd = random.Random(seed)
    pts, ts = [[0, 0, 0]], sorted(rnd.uniform(2, dur - dec * 2 - 4) for _ in range(n))
    last = 0.0
    nodes = [noise("n", color), filt("nf", kind, "n", R(cut * 0.8, cut * 1.2), q)]
    mixin = []
    for t in ts:
        t = max(t, last + 3)
        pts += [[t, 0, 0], [t + 1.5, rnd.uniform(0.4, 1.0), 0], [t + 1.5 + dec, 0, -3]]
        last = t + 1.5 + dec + 1
    nodes += [{"id": "e", "type": "env", "points": pts}, gain("ng", "nf", "e")]
    mixin.append(("ng", 1.0))
    if pitch:
        nodes += [osc("po", "sine", SW(pitch, pitch * 2.2, dec * 2, -1)), {"id": "pe", "type": "env", "points": pts}, gain("pg", "po", "pe")]
        mixin.append(("pg", 0.6))
    nodes.append(mix("o", mixin))
    return sound(sid, int(last + 12), seed, nodes, "o", variants=var, peak=peak, bus=bus, fo=12, aliases=aliases)


def door(sid, seed, opening, metal=False, dur=360, aliases=None, deep=1.0):
    """creak + latch click (open) / thud + click (close); metal rings instead of creaking wood"""
    f0, f1 = (150, 280) if opening else (280, 140)
    form = 1900 if metal else 650
    nodes = [osc("cr", "saw", SW(R(f0 * 0.9, f0 * 1.1) if False else f0, f1, dur * 0.55, 0, True)),
             filt("cf", "bp", "cr", R(form * 0.85, form * 1.15), 6 if metal else 3.5),
             adsr("ce", 12, dur * 0.4, r=40, at=0 if opening else 10, peak=0.55), gain("cg", "cf", "ce"),
             noise("kn"), filt("kf", "bp", "kn", 4200 if metal else 2600, 1.8), adsr("ke", 0.4, 9, r=2, at=(dur * 0.6 if opening else dur * 0.5)),
             gain("kg", "kf", "ke")]
    mixin = [("cg", 1.0), ("kg", 0.9)]
    if not opening or not metal:
        nodes += [osc("tb", "sine", SW(110 * deep, 55 * deep, 70, -2)), adsr("te", 1.5, 70, r=8, at=(dur * 0.5 if not opening else dur * 0.62)),
                  gain("tg", "tb", "te")]
        mixin.append(("tg", 0.8 if not opening else 0.3))
    if metal:
        for k, (f, g) in enumerate([(820, 0.5), (1330, 0.35)]):
            nodes += [osc("rp%d" % k, "sine", R(f * 0.98, f * 1.02)), adsr("rq%d" % k, 0.5, 160, r=10, at=(dur * 0.5 if not opening else dur * 0.6)),
                      gain("rg%d" % k, "rp%d" % k, "rq%d" % k)]
            mixin.append(("rg%d" % k, g * 0.6))
    nodes.append(mix("o", mixin))
    return sound(sid, dur, seed, nodes, "o", variants=2, peak=-5, fo=30, aliases=aliases)


def effects():
    o = []
    # --- doors, gates, containers
    for k, (nm, metal, dur, deep, al) in enumerate([
            ("wooden_door", False, 360, 1.0, []), ("iron_door", True, 420, 0.8, []), ("wooden_trapdoor", False, 300, 1.2, []),
            ("iron_trapdoor", True, 340, 1.0, []), ("fence_gate", False, 330, 1.1, []), ("chest", False, 380, 0.9, []),
            ("ender_chest", True, 400, 0.7, []), ("barrel", False, 320, 1.3, []), ("shulker_box", False, 340, 0.8, [])]):
        for j, act in enumerate(("open", "close")):
            o.append(door("block.%s.%s" % (nm, act), 8100 + k * 10 + j, act == "open", metal, dur, aliases=al, deep=deep))
    o.append(perc("block.chest.locked", 8190, 220, nz=[("white", "bp", 2800, 3, 14, 0.7, 0), ("white", "bp", 2300, 3, 14, 0.6, 60)],
                  body=(180, 110, 50, 0.6, 0), var=2, peak=-6))
    # --- switches
    o.append(perc("block.lever.click", 8200, 120, nz=[("white", "bp", 3000, 2, 8, 0.8, 0), ("white", "bp", 2200, 2, 10, 0.6, 35)],
                  body=(900, 420, 25, 0.5, 0), var=2, peak=-6))
    for k, (nm, cut, f) in enumerate([("stone_button", 2600, 1100), ("wooden_button", 1800, 760), ("stone_pressure_plate", 1500, 520),
                                       ("wooden_pressure_plate", 1300, 420)]):
        for j, act in enumerate(("click_on", "click_off")):
            sc = 1.0 if j == 0 else 0.8
            o.append(perc("block.%s.%s" % (nm, act), 8210 + k * 10 + j, 110, nz=[("white", "bp", cut * sc, 1.6, 9, 0.9, 0)],
                          body=(f * sc, f * sc * 0.5, 30, 0.6, 0), var=2, peak=-7))
    # --- arrows
    o.append(whoosh("entity.arrow.shoot", 8300, 260, 900, 2600, "white", "bp", 1.2, att=8, var=2, peak=-6, level=0.5))
    o[-1]["nodes"] += [osc("tw", "sine", SW(210, 150, 140, -1)), adsr("twe", 0.8, 120, r=20), gain("twg", "tw", "twe"),
                       mix("o2", [("o", 1.0), ("twg", 0.8)])]
    o[-1]["out"] = "o2"
    o.append(perc("entity.arrow.hit", 8310, 140, nz=[("white", "bp", 2000, 1.5, 22, 0.8, 0)], body=(240, 110, 70, 1.0, 0),
                  parts=[(1300, 0.15, 60, 0)], var=3, peak=-5))
    o.append(perc("entity.arrow.hit_player", 8320, 260, nz=[("white", "hp", 3500, 0.8, 10, 0.6, 0)], body=(190, 90, 90, 0.9, 0),
                  parts=[(1650, 0.5, 200, 0), (2480, 0.3, 140, 0)], var=2, peak=-6))
    # --- mobs
    o.append(crackle("entity.skeleton.ambient", 8400, 700, 7, 3200, "white", 12, var=3, peak=-8, q=3))
    o.append(voice("entity.skeleton.hurt", 8401, 260, 330, 220, "pulse", 1200, 6, noise_amt=0.3, variants=2, peak=-5))
    o.append(crackle("entity.skeleton.death", 8402, 900, 16, 2600, "white", 20, var=2, peak=-6, q=2.5))
    o.append(perc("entity.skeleton.step", 8403, 120, nz=[("white", "bp", 3300, 3, 14, 0.8, 0)], body=(420, 240, 40, 0.6, 0),
                  parts=[(1500, 0.3, 40, 0)], var=3, peak=-9))
    o.append(whoosh("entity.creeper.primed", 8410, 1500, 3200, 5200, "white", "hp", 0.7, att=500, var=1, peak=-8, level=0.9))
    o.append(voice("entity.creeper.hurt", 8411, 300, 260, 160, "square", 800, 4, noise_amt=0.3, variants=2, peak=-5))
    o.append(voice("entity.creeper.death", 8412, 700, 210, 70, "square", 700, 4, noise_amt=0.3, variants=2, peak=-5))
    o.append(crackle("entity.spider.ambient", 8420, 600, 10, 4800, "white", 18, var=3, peak=-9, q=2))
    o.append(voice("entity.spider.hurt", 8421, 280, 700, 1000, "saw", 1600, 3, noise_amt=0.3, variants=2, peak=-6))
    o.append(voice("entity.spider.death", 8422, 700, 900, 260, "saw", 1500, 3, noise_amt=0.3, variants=2, peak=-6))
    o.append(perc("entity.spider.step", 8423, 90, nz=[("white", "bp", 4200, 2, 8, 0.7, 0), ("white", "bp", 3600, 2, 8, 0.5, 18)],
                  var=3, peak=-12))
    # --- fire / lava / water
    o.append(crackle("block.fire.ambient", 8500, 1400, 22, 1800, "pink", 16, var=3, peak=-10, kind="bp", q=0.8))
    o.append(whoosh("block.fire.extinguish", 8501, 600, 6500, 2500, "white", "bp", 0.7, att=20, var=2, peak=-7, aliases=["entity.generic.extinguish_fire"]))
    o.append(crackle("block.lava.ambient", 8510, 1500, 8, 400, "brown", 90, var=3, peak=-9, kind="lp", q=0.7, pitch=120))
    o.append(perc("block.lava.pop", 8511, 200, nz=[("pink", "bp", 900, 1.2, 30, 0.5, 0)], body=(170, 520, 60, 0.9, 0), var=3, peak=-8))
    o.append(whoosh("block.lava.extinguish", 8512, 700, 5200, 1800, "white", "bp", 0.7, att=25, var=2, peak=-7, sub=(140, 60)))
    o.append(whoosh("entity.generic.splash", 8520, 450, 4200, 900, "pink", "bp", 0.7, att=10, var=3, peak=-5, aliases=["entity.player.splash"]))
    o.append(whoosh("entity.player.splash.high_speed", 8521, 650, 3600, 700, "pink", "bp", 0.6, att=8, var=2, peak=-4, sub=(130, 60)))
    o.append(whoosh("entity.generic.swim", 8522, 380, 700, 1900, "pink", "bp", 1.0, att=60, var=3, peak=-8, aliases=["entity.player.swim"]))
    o.append(whoosh("entity.generic.burn", 8523, 260, 3800, 2000, "white", "bp", 0.8, att=10, var=2, peak=-7))
    # --- player + generic entity sounds
    o.append(voice("entity.player.hurt", 8600, 300, 200, 140, "saw", 650, 4, noise_amt=0.18, variants=3, peak=-5,
                   aliases=["entity.generic.hurt"]))
    o.append(voice("entity.player.death", 8601, 900, 190, 55, "saw", 520, 4, vib=0.008, noise_amt=0.2, variants=2, peak=-5,
                   aliases=["entity.generic.death"]))
    o.append(voice("entity.player.hurt_on_fire", 8602, 380, 260, 330, "saw", 800, 3, vib=0.03, noise_amt=0.35, variants=2, peak=-6))
    o.append(voice("entity.player.hurt_freeze", 8603, 380, 300, 240, "saw", 900, 4, vib=0.05, noise_amt=0.25, variants=2, peak=-6))
    o.append(voice("entity.player.hurt_drown", 8604, 520, 210, 130, "saw", 500, 3, vib=0.04, noise_amt=0.45, variants=2, peak=-6))
    o.append(voice("entity.player.hurt_sweet_berry_bush", 8605, 220, 280, 200, "saw", 700, 4, noise_amt=0.2, variants=2, peak=-7))
    o.append(voice("entity.player.burp", 8606, 420, 120, 80, "saw", 380, 3, vib=0.02, noise_amt=0.1, variants=2, peak=-6))
    o.append(whoosh("entity.player.breath", 8607, 700, 900, 600, "pink", "lp", 0.7, att=250, var=2, peak=-12))
    o.append(whoosh("entity.player.attack.weak", 8610, 220, 1200, 3200, "white", "bp", 1.0, att=30, var=3, peak=-8))
    o.append(whoosh("entity.player.attack.strong", 8611, 300, 800, 2400, "white", "bp", 0.9, att=40, var=3, peak=-6, sub=(180, 90)))
    o.append(whoosh("entity.player.attack.sweep", 8612, 380, 700, 3600, "white", "bp", 0.8, att=100, var=3, peak=-7))
    o.append(perc("entity.player.attack.crit", 8613, 320, nz=[("white", "hp", 2600, 0.8, 25, 0.9, 0)], body=(260, 110, 110, 1.0, 0),
                  parts=[(1800, 0.4, 180, 0), (2700, 0.25, 120, 0)], var=2, peak=-4))
    o.append(perc("entity.player.attack.knockback", 8614, 300, nz=[("pink", "bp", 1200, 1.0, 45, 0.9, 0)], body=(150, 60, 140, 1.2, 0),
                  var=2, peak=-4))
    o.append(perc("entity.player.attack.nodamage", 8615, 180, nz=[("pink", "bp", 1500, 1.0, 30, 0.8, 0)], body=(200, 110, 70, 0.6, 0),
                  var=3, peak=-8))
    o.append(perc("entity.player.small_fall", 8620, 200, nz=[("pink", "bp", 900, 0.8, 50, 0.8, 0)], body=(110, 55, 100, 1.2, 0), var=2,
                  peak=-5, aliases=["entity.generic.small_fall"]))
    o.append(perc("entity.player.big_fall", 8621, 420, nz=[("pink", "bp", 700, 0.8, 90, 1.0, 0), ("white", "bp", 400, 0.8, 120, 0.5, 70)],
                  body=(90, 38, 220, 1.4, 0), var=2, peak=-3, aliases=["entity.generic.big_fall"]))
    o.append(crackle("entity.generic.eat", 8630, 420, 6, 2400, "pink", 22, var=3, peak=-8, q=1.2))
    o.append(crackle("entity.generic.drink", 8631, 460, 3, 350, "brown", 90, var=2, peak=-8, kind="lp", q=0.7, pitch=240))
    o.append(whoosh("entity.generic.explode", 8632, 1500, 2200, 220, "brown", "lp", 0.7, att=4, var=3, peak=-2, sub=(70, 28)))
    o.append(perc("entity.item.pickup", 8640, 160, body=(520, 1450, 70, 0.9, 0), parts=[(1900, 0.3, 90, 10)], var=3, peak=-9))
    o.append(perc("entity.item.break", 8641, 280, nz=[("white", "bp", 2500, 1.2, 40, 0.8, 0), ("white", "bp", 2000, 1.2, 40, 0.6, 50)],
                  body=(260, 120, 80, 0.7, 0), var=2, peak=-6))
    o.append(whoosh("entity.tnt.primed", 8650, 1000, 4000, 2000, "white", "hp", 0.7, att=60, var=1, peak=-8))
    o.append(perc("entity.lightning_bolt.impact", 8651, 900, nz=[("white", "hp", 1800, 0.7, 120, 1.0, 0)], body=(120, 36, 600, 1.2, 0),
                  var=2, peak=-2))
    return o


# ------------------------------------------------------------------------------------------------ coverage gaps (tier 1, z1)
# Items, containers, note block instruments, portals, firework, toasts, ambience. Names that only differ by a variant of the same
# action are ALIASES of one sound (an alias costs an engine event, no sample: limit 512 events / 512 samples).
def tone(sid, seed, dur, f, wave="sine", harm=(), lp=None, att=2, dec=200, vib=0.0, nz=None, sweep=None, var=1, peak=-5,
         aliases=None, bus="sfx", q=0.7, fo=None, gmain=1.0):
    """pitched hit: main oscillator (optional low pass, vibrato, pitch sweep) + sine partials [(mult, gain, decay_ms)] + noise click
    nz = (color, kind, cut, q, decay_ms, gain)"""
    nodes = []
    fq = SW(sweep[0], sweep[1], sweep[2], -1) if sweep else R(f * 0.99, f * 1.01)
    m0 = osc("m0", wave, fq, width=0.4)
    if vib:
        nodes.append(osc("vb", "sine", R(5, 7), scale=vib))
        m0["pm"] = "vb"
        m0["pm_depth"] = 1.0
    nodes.append(m0)
    src = "m0"
    if lp:
        nodes.append(filt("mf", "lp", "m0", lp, q))
        src = "mf"
    nodes += [adsr("me", att, dec, r=8), gain("mg", src, "me")]
    mixin = [("mg", gmain)]
    for i, (mult, g, d) in enumerate(harm):
        nodes += [osc("h%d" % i, "sine", R(f * mult * 0.995, f * mult * 1.005)), adsr("he%d" % i, att, d, r=6), gain("hg%d" % i, "h%d" % i, "he%d" % i)]
        mixin.append(("hg%d" % i, g))
    if nz:
        color, kind, cut, qq, d, g = nz
        nodes += [noise("nc", color), filt("ncf", kind, "nc", R(cut * 0.9, cut * 1.1), qq), adsr("nce", 0.4, d, r=2), gain("ncg", "ncf", "nce")]
        mixin.append(("ncg", g))
    nodes.append(mix("o", mixin))
    return sound(sid, dur, seed, nodes, "o", variants=var, peak=peak, bus=bus, fo=fo or max(8, dur // 10), aliases=aliases)


def loopify(s, fade=150):
    s["event"]["loop"] = True
    s["fade_in_ms"] = fade
    s["fade_out_ms"] = fade
    return s


def al(prefix, names):
    return [prefix + n for n in names]


def coverage_sounds():
    o = []
    # --- items
    o.append(perc("item.armor.equip_generic", 9000, 240, nz=[("pink", "bp", 1800, 1.0, 50, 0.9, 0), ("white", "bp", 3000, 2, 10, 0.4, 40)],
                  body=(200, 120, 60, 0.6, 0), var=2, peak=-7, aliases=al("item.armor.equip_", ["leather", "elytra", "turtle"])))
    o.append(perc("item.armor.equip_iron", 9001, 380, nz=[("white", "hp", 4000, 1.0, 12, 0.6, 0), ("pink", "bp", 2200, 1.2, 40, 0.5, 30)],
                  parts=[(1200, 0.5, 200, 0), (1900, 0.3, 150, 0), (2800, 0.2, 100, 10)], var=1, peak=-7,
                  aliases=al("item.armor.equip_", ["chain", "gold", "diamond", "netherite"])))
    o.append(crackle("item.bucket.fill", 9010, 520, 4, 400, "brown", 70, var=2, peak=-7, kind="lp", q=0.7, pitch=300,
                     aliases=al("item.bucket.fill_", ["axolotl", "fish", "lava", "powder_snow", "tadpole"])))
    o.append(whoosh("item.bucket.empty", 9011, 520, 1500, 500, "pink", "bp", 0.8, att=40, var=2, peak=-7,
                    aliases=al("item.bucket.empty_", ["axolotl", "fish", "lava", "powder_snow", "tadpole"])))
    o.append(crackle("item.bottle.fill", 9012, 380, 3, 500, "brown", 60, var=2, peak=-8, kind="lp", q=0.7, pitch=520,
                     aliases=["item.bottle.fill_dragonbreath"]))
    o.append(whoosh("item.bottle.empty", 9013, 320, 900, 400, "pink", "bp", 0.8, att=30, var=1, peak=-8))
    o.append(perc("item.flintandsteel.use", 9020, 260, nz=[("white", "hp", 4500, 1.0, 10, 0.9, 0), ("white", "hp", 5500, 1.2, 8, 0.7, 45),
                                                            ("white", "bp", 2500, 1.0, 60, 0.4, 80)], parts=[(2600, 0.2, 40, 0)], var=2, peak=-7))
    o.append(whoosh("item.firecharge.use", 9021, 450, 600, 2200, "pink", "bp", 0.8, att=40, var=1, peak=-7))
    o.append(perc("item.shield.block", 9030, 280, nz=[("white", "bp", 1600, 1.5, 30, 0.9, 0)], body=(280, 140, 80, 1.0, 0),
                  parts=[(900, 0.4, 200, 0), (1500, 0.3, 150, 0)], var=2, peak=-5))
    o.append(perc("item.shield.break", 9031, 450, nz=[("white", "bp", 2000, 1.2, 50, 0.9, 0), ("white", "bp", 1500, 1.2, 60, 0.7, 70),
                                                       ("white", "hp", 3000, 1.0, 40, 0.5, 140)], body=(200, 70, 200, 1.0, 0), var=1, peak=-4))
    o.append(whoosh("item.trident.throw", 9040, 350, 600, 2800, "white", "bp", 1.0, att=20, var=1, peak=-6, aliases=["item.trident.return"]))
    o.append(perc("item.trident.hit", 9041, 240, nz=[("white", "bp", 2200, 1.5, 20, 0.8, 0)], body=(300, 120, 80, 1.0, 0),
                  parts=[(1100, 0.3, 120, 0)], var=1, peak=-5, aliases=["item.trident.hit_ground"]))
    o.append(whoosh("item.trident.riptide_1", 9042, 700, 400, 4000, "white", "bp", 0.9, att=120, var=1, peak=-6, sub=(120, 300),
                    aliases=["item.trident.riptide_2", "item.trident.riptide_3"]))
    # --- blocks
    o.append(perc("block.anvil.use", 9100, 600, nz=[("white", "hp", 3000, 0.7, 10, 0.6, 0)],
                  parts=[(520, 0.8, 350, 0), (1120, 0.6, 260, 0), (1650, 0.4, 200, 0), (2400, 0.25, 120, 0)], var=1, peak=-6))
    o.append(perc("block.anvil.land", 9101, 600, nz=[("white", "bp", 2000, 1.0, 25, 0.8, 0)], body=(130, 60, 120, 1.1, 0),
                  parts=[(480, 0.6, 300, 0), (1030, 0.5, 240, 0), (1530, 0.3, 180, 0)], var=2, peak=-4))
    o.append(perc("block.anvil.destroy", 9102, 700, nz=[("white", "bp", 2200, 1.0, 40, 0.8, 0), ("white", "hp", 3500, 0.8, 30, 0.6, 90)],
                  body=(160, 60, 160, 1.0, 0), parts=[(420, 0.5, 400, 0), (960, 0.4, 300, 90), (1480, 0.3, 200, 90)], var=1, peak=-4))
    o.append(seq("block.beacon.activate", 9110, [("D4", 0, 300, 0.7), ("A4", 130, 300, 0.7), ("D5", 260, 320, 0.8), ("F#5", 390, 340, 0.8),
                                                 ("A5", 520, 700, 0.9)], 1500, rev=0.35, bus="sfx", peak=-6, aliases=["block.beacon.power_select"]))
    o.append(seq("block.beacon.deactivate", 9111, [("A5", 0, 260, 0.8), ("F#5", 140, 280, 0.8), ("D5", 280, 320, 0.8), ("A4", 420, 380, 0.8),
                                                   ("D4", 560, 700, 0.9)], 1500, rev=0.35, bus="sfx", peak=-6))
    o.append(seq("block.beacon.ambient", 9112, [("D3", 0, 2100, 0.8), ("A3", 0, 2100, 0.5)], 2600, rev=0.3, bus="sfx", peak=-12, attack=350,
                 tail=300, fo=300))
    o.append(perc("block.bell.use", 9120, 1600, nz=[("white", "hp", 3500, 0.8, 12, 0.5, 0)],
                  parts=[(880, 1.0, 1200, 0), (1480, 0.5, 900, 0), (2210, 0.3, 600, 0), (3400, 0.15, 300, 0)], var=1, peak=-5,
                  aliases=["block.bell.resonate"]))
    o.append(crackle("block.brewing_stand.brew", 9130, 900, 9, 500, "brown", 50, var=1, peak=-8, kind="lp", q=0.7, pitch=350))
    o.append(crackle("block.bubble_column.bubble_pop", 9131, 350, 2, 600, "pink", 40, var=2, peak=-8, kind="lp", q=0.7, pitch=700,
                     aliases=al("block.bubble_column.", ["upwards_ambient", "upwards_inside", "whirlpool_ambient", "whirlpool_inside"])))
    o.append(perc("block.composter.fill", 9140, 220, nz=[("pink", "bp", 1200, 0.8, 40, 0.9, 0), ("pink", "bp", 900, 0.8, 40, 0.6, 60)],
                  var=2, peak=-8, aliases=[]))
    o.append(perc("block.composter.fill_success", 9141, 320, nz=[("pink", "bp", 1300, 0.8, 40, 0.5, 0)], body=(300, 700, 120, 0.8, 0),
                  parts=[(900, 0.4, 100, 40)], var=1, peak=-7, aliases=["block.composter.ready"]))
    o.append(whoosh("block.composter.empty", 9142, 400, 1500, 600, "pink", "lp", 0.7, att=30, var=1, peak=-8))
    o.append(perc("block.dispenser.dispense", 9150, 220, nz=[("white", "bp", 2000, 1.5, 15, 0.9, 0), ("white", "bp", 1500, 1.5, 25, 0.7, 60)],
                  body=(160, 90, 50, 0.7, 0), var=1, peak=-7))
    o.append(whoosh("block.dispenser.launch", 9151, 300, 700, 2500, "white", "bp", 1.0, att=15, var=1, peak=-7))
    o.append(perc("block.dispenser.fail", 9152, 160, nz=[("white", "bp", 2400, 1.6, 12, 0.8, 0), ("white", "bp", 2000, 1.6, 12, 0.6, 55)],
                  var=1, peak=-9))
    o.append(seq("block.enchantment_table.use", 9160, [("E5", 0, 400, 0.5), ("A5", 100, 400, 0.5), ("B5", 200, 400, 0.5), ("E6", 350, 600, 0.6),
                                                       ("D6", 500, 700, 0.5)], 1500, rev=0.5, bus="sfx", peak=-8, room=0.6))
    o.append(perc("block.piston.extend", 9170, 350, nz=[("brown", "lp", 800, 0.7, 150, 1.0, 0)], body=(100, 55, 180, 1.2, 0), var=1, peak=-5))
    o.append(perc("block.piston.contract", 9171, 300, nz=[("pink", "bp", 600, 0.8, 100, 0.8, 0)], body=(70, 130, 150, 1.2, 0), var=1, peak=-5))
    o.append(whoosh("block.portal.ambient", 9180, 2500, 300, 700, "brown", "lp", 0.8, att=800, var=1, peak=-10, sub=(70, 110)))
    o.append(whoosh("block.portal.trigger", 9181, 1800, 300, 2500, "pink", "bp", 0.8, att=600, var=1, peak=-8, sub=(60, 200)))
    o.append(whoosh("block.portal.travel", 9182, 1500, 2500, 200, "pink", "bp", 0.8, att=100, var=1, peak=-6, sub=(300, 70)))
    o.append(perc("block.tripwire.click_on", 9190, 90, nz=[("white", "bp", 3500, 2, 8, 0.8, 0)], body=(1200, 700, 20, 0.5, 0), var=1, peak=-8,
                  aliases=["block.tripwire.attach"]))
    o.append(perc("block.tripwire.click_off", 9191, 90, nz=[("white", "bp", 2800, 2, 8, 0.8, 0)], body=(900, 500, 20, 0.5, 0), var=1, peak=-8,
                  aliases=["block.tripwire.detach"]))
    # --- note block instruments (base pitch around F#4; the game transposes by the pitch field)
    nb = "block.note_block."
    o.append(tone(nb + "harp", 9200, 600, 370, harm=[(2, 0.3, 300), (3, 0.1, 200)], dec=450, att=3))
    o.append(tone(nb + "bass", 9201, 420, 92.5, wave="saw", lp=700, dec=320, att=3, harm=[(2, 0.3, 200)]))
    o.append(tone(nb + "bell", 9202, 1200, 740, harm=[(2.76, 0.4, 700), (5.4, 0.2, 400)], dec=900, att=1, nz=("white", "hp", 5000, 0.8, 8, 0.2)))
    o.append(tone(nb + "chime", 9203, 1000, 1480, harm=[(2.0, 0.4, 600), (3.0, 0.25, 400)], dec=800, att=1))
    o.append(tone(nb + "flute", 9204, 550, 740, att=45, dec=420, harm=[(2, 0.12, 300)], nz=("pink", "bp", 2200, 1.2, 380, 0.15), vib=0.004))
    o.append(tone(nb + "guitar", 9205, 700, 185, wave="saw", lp=1800, dec=500, att=1, harm=[(2, 0.35, 300), (3, 0.2, 200)],
                  nz=("white", "bp", 2500, 1.0, 12, 0.3)))
    o.append(tone(nb + "pling", 9206, 500, 740, wave="square", lp=3200, dec=330, att=1, harm=[(2, 0.25, 200)]))
    o.append(tone(nb + "xylophone", 9207, 420, 1480, harm=[(4.0, 0.25, 90)], dec=260, att=0.5, nz=("white", "bp", 3000, 1.5, 6, 0.3)))
    o.append(tone(nb + "iron_xylophone", 9208, 800, 740, harm=[(2.76, 0.5, 500), (5.4, 0.25, 300)], dec=600, att=0.5,
                  nz=("white", "hp", 4500, 1.0, 6, 0.3)))
    o.append(tone(nb + "cow_bell", 9209, 450, 560, wave="square", lp=2400, dec=300, att=0.5, harm=[(1.51, 0.6, 250)]))
    o.append(tone(nb + "didgeridoo", 9210, 600, 92.5, wave="saw", lp=420, dec=520, att=30, q=1.5, vib=0.01))
    o.append(tone(nb + "bit", 9211, 380, 740, wave="square", lp=4500, dec=250, att=1))
    o.append(tone(nb + "banjo", 9212, 450, 370, wave="saw", lp=2600, dec=260, att=0.5, harm=[(2, 0.4, 160)], nz=("white", "bp", 3000, 1.2, 10, 0.4)))
    o.append(perc(nb + "basedrum", 9213, 380, nz=[("white", "bp", 900, 1.0, 12, 0.5, 0)], body=(150, 45, 280, 1.3, 0), var=1, peak=-4))
    o.append(perc(nb + "snare", 9214, 280, nz=[("white", "bp", 2200, 0.8, 140, 1.0, 0)], body=(230, 150, 70, 0.7, 0), var=1, peak=-5))
    o.append(perc(nb + "hat", 9215, 120, nz=[("white", "hp", 7000, 0.8, 45, 1.0, 0)], var=1, peak=-8))
    # --- entities
    o.append(whoosh("entity.boat.paddle_water", 9300, 450, 500, 900, "pink", "bp", 0.8, att=100, var=1, peak=-8))
    o.append(perc("entity.boat.paddle_land", 9301, 250, nz=[("pink", "bp", 800, 0.8, 60, 0.9, 0)], body=(140, 80, 90, 1.0, 0), var=1, peak=-8))
    o.append(whoosh("entity.snowball.throw", 9310, 220, 1500, 700, "white", "bp", 1.0, att=15, var=2, peak=-8,
                    aliases=al("entity.", ["egg.throw", "ender_pearl.throw", "fishing_bobber.throw", "splash_potion.throw"])))
    o.append(whoosh("entity.firework_rocket.launch", 9320, 900, 1200, 4500, "white", "bp", 0.8, att=80, var=1, peak=-7,
                    aliases=["entity.firework_rocket.shoot"]))
    o.append(whoosh("entity.firework_rocket.blast", 9321, 600, 2500, 300, "white", "lp", 0.7, att=3, var=2, peak=-4, sub=(200, 60),
                    aliases=al("entity.firework_rocket.", ["large_blast", "blast_far", "large_blast_far"])))
    o.append(crackle("entity.firework_rocket.twinkle", 9322, 700, 14, 5500, "white", 8, var=1, peak=-8, kind="hp", q=0.8,
                     aliases=["entity.firework_rocket.twinkle_far"]))
    o.append(whoosh("entity.fishing_bobber.retrieve", 9330, 300, 900, 2500, "pink", "bp", 1.0, att=20, var=1, peak=-8))
    o.append(perc("entity.item_frame.add_item", 9340, 140, nz=[("pink", "bp", 2000, 1.0, 20, 0.8, 0)], body=(400, 250, 40, 0.6, 0), var=1,
                  peak=-8, aliases=["entity.item_frame.rotate_item", "entity.item_frame.remove_item"]))
    o.append(loopify(whoosh("entity.minecart.riding", 9350, 1200, 300, 500, "brown", "bp", 1.0, att=200, var=1, peak=-10,
                            aliases=["entity.minecart.inside", "entity.minecart.inside.underwater"])))
    o.append(perc("enchant.thorns.hit", 9360, 160, nz=[("white", "hp", 3000, 1.0, 15, 0.8, 0)], body=(300, 150, 50, 0.6, 0), parts=[(1800, 0.4, 80, 0)],
                  var=1, peak=-7))
    # --- UI
    o.append(seq("ui.toast.in", 9400, [("A5", 0, 100, 0.8), ("E6", 80, 300, 1.0)], 700, rev=0.2, peak=-8))
    o.append(seq("ui.toast.out", 9401, [("E6", 0, 100, 0.8), ("A5", 80, 280, 1.0)], 700, rev=0.2, peak=-8))
    o.append(seq("ui.toast.challenge_complete", 9402, [("D5", 0, 200, 0.8), ("F#5", 150, 200, 0.8), ("A5", 300, 200, 0.8), ("D6", 450, 900, 1.0)],
                 1800, rev=0.3, peak=-6))
    # --- mob steps (Tier 2 stand-ins: the other mobs alias these)
    o.append(perc("entity.zombie.step", 9450, 140, nz=[("pink", "bp", 900, 0.9, 35, 0.9, 0)], body=(120, 70, 60, 0.8, 0), var=1, peak=-10))
    o.append(perc("entity.cow.step", 9451, 160, nz=[("pink", "bp", 700, 0.9, 40, 0.9, 0)], body=(100, 60, 70, 1.0, 0), var=1, peak=-10))
    # --- ambience
    o.append(whoosh("ambient.mood", 9500, 2200, 500, 150, "brown", "lp", 0.7, att=700, var=1, peak=-9, sub=(70, 45),
                    aliases=[b + s for b in ("ambient.basalt_deltas.", "ambient.crimson_forest.", "ambient.nether_wastes.", "ambient.soul_sand_valley.",
                                             "ambient.warped_forest.") for s in ("additions", "mood")]))
    o.append(whoosh("ambient.underwater.enter", 9510, 600, 4000, 400, "pink", "lp", 0.7, att=20, var=1, peak=-7))
    o.append(whoosh("ambient.underwater.exit", 9511, 600, 400, 4000, "pink", "lp", 0.7, att=20, var=1, peak=-7))
    return o


# events that are a stand-in on a sound which already exists (no new sample)
# ------------------------------------------------------------------------------------------------ tier 3 (r166)
# The 558 rare events (tools/mc/audio/coverage.py tier 3). Same archetypes as before (voice / perc / whoosh / tone / crackle), driven by tables.
# Own seed range 30000+ (ns()) so NO existing recipe changes (their hashes stay). Rare variants of an action are ALIASES (an alias costs an
# engine event, no sample). Order of the tables = order of seeds: only append at the END of a table.
_T3 = [30000]


def ns():
    _T3[0] += 3
    return _T3[0]


def Nz(kind, cut, dec, g=0.8, at=0, color="white", q=1.5):
    return (color, kind, cut, q, dec, g, at)


# event -> (dur ms, f0 Hz, f1 Hz, wave, formant Hz, formant Q, vibrato, noise)
T3_VOICES = {
    "entity.allay.ambient_with_item": (450, 1000, 1400, "sine", 2800, 3, 0.05, 0.03),
    "entity.allay.ambient_without_item": (400, 800, 1000, "sine", 2400, 3, 0.05, 0.03),
    "entity.allay.hurt": (200, 1600, 1000, "sine", 3000, 3, 0.0, 0.03),
    "entity.allay.death": (500, 1400, 500, "sine", 2600, 3, 0.03, 0.03),
    "entity.axolotl.attack": (200, 500, 800, "pulse", 1300, 3, 0.0, 0.15),
    "entity.axolotl.death": (450, 700, 250, "pulse", 1100, 3, 0.02, 0.15),
    "entity.axolotl.hurt": (250, 900, 600, "pulse", 1300, 3, 0.0, 0.15),
    "entity.axolotl.idle_air": (400, 700, 900, "pulse", 1500, 3, 0.04, 0.1),
    "entity.axolotl.idle_water": (500, 450, 600, "sine", 900, 3, 0.05, 0.15),
    "entity.bat.takeoff": (180, 2400, 4000, "pulse", 4400, 4, 0.0, 0.08),
    "entity.camel.ambient": (1000, 150, 200, "saw", 600, 3, 0.03, 0.15),
    "entity.camel.death": (1300, 300, 80, "saw", 700, 3, 0.03, 0.15),
    "entity.camel.hurt": (500, 430, 260, "saw", 800, 3, 0.0, 0.15),
    "entity.camel.dash": (500, 200, 320, "saw", 700, 3, 0.03, 0.2),
    "entity.camel.dash_ready": (300, 260, 340, "saw", 800, 3, 0.0, 0.15),
    "entity.cat.stray_ambient": (800, 520, 820, "saw", 1600, 4, 0.03, 0.1),
    "entity.cat.beg_for_food": (600, 600, 950, "saw", 1700, 4, 0.05, 0.08),
    "entity.cat.purr": (1400, 50, 46, "saw", 200, 2, 0.0, 0.5),
    "entity.cat.purreow": (900, 520, 900, "saw", 1600, 3, 0.04, 0.1),
    "entity.donkey.angry": (700, 650, 280, "saw", 1100, 3, 0.05, 0.1),
    "entity.dolphin.play": (450, 2800, 4200, "sine", 3600, 3, 0.05, 0.06),
    "entity.enderman.scream": (900, 300, 1100, "saw", 1000, 2, 0.06, 0.3),
    "entity.enderman.stare": (1100, 420, 330, "saw", 800, 3, 0.05, 0.25),
    "entity.evoker.ambient": (800, 190, 160, "pulse", 850, 4, 0.02, 0.15),
    "entity.evoker.hurt": (380, 320, 200, "pulse", 1000, 4, 0.0, 0.15),
    "entity.evoker.death": (1000, 260, 70, "pulse", 800, 4, 0.02, 0.18),
    "entity.evoker.celebrate": (900, 200, 420, "saw", 900, 3, 0.1, 0.12),
    "entity.evoker.prepare_attack": (1000, 150, 320, "saw", 700, 3, 0.04, 0.2),
    "entity.evoker.prepare_summon": (1100, 120, 280, "saw", 600, 3, 0.05, 0.3),
    "entity.evoker.prepare_wololo": (1200, 180, 360, "saw", 800, 3, 0.12, 0.2),
    "entity.fox.aggro": (500, 900, 1500, "saw", 2200, 3, 0.03, 0.15),
    "entity.fox.screech": (700, 1400, 2300, "saw", 2400, 3, 0.06, 0.15),
    "entity.fox.sleep": (900, 300, 230, "sine", 700, 3, 0.03, 0.4),
    "entity.frog.ambient": (350, 150, 240, "pulse", 520, 3, 0.0, 0.2),
    "entity.frog.death": (500, 130, 60, "pulse", 450, 3, 0.02, 0.25),
    "entity.frog.hurt": (250, 200, 120, "pulse", 520, 3, 0.0, 0.25),
    "entity.frog.lay_spawn": (900, 120, 90, "pulse", 420, 3, 0.05, 0.3),
    "entity.ghast.scream": (1200, 900, 500, "saw", 1700, 3, 0.08, 0.15),
    "entity.ghast.warn": (700, 450, 720, "saw", 1400, 3, 0.05, 0.12),
    "entity.goat.ambient": (750, 420, 340, "saw", 1400, 4, 0.12, 0.05),
    "entity.goat.death": (800, 520, 160, "saw", 1300, 4, 0.1, 0.06),
    "entity.goat.hurt": (400, 680, 430, "saw", 1500, 4, 0.06, 0.06),
    "entity.horse.breathe": (600, 200, 190, "sine", 500, 2, 0.0, 0.8),
    "entity.hostile.death": (700, 200, 60, "saw", 500, 3, 0.01, 0.3),
    "entity.hostile.hurt": (350, 260, 150, "saw", 600, 3, 0.0, 0.3),
    "entity.panda.cant_breed": (500, 320, 220, "saw", 800, 3, 0.04, 0.1),
    "entity.panda.aggressive_ambient": (700, 170, 250, "saw", 700, 3, 0.05, 0.15),
    "entity.panda.worried_ambient": (700, 430, 320, "sine", 1000, 3, 0.05, 0.1),
    "entity.puffer_fish.ambient": (300, 700, 600, "pulse", 1400, 3, 0.0, 0.2),
    "entity.ravager.roar": (1800, 80, 190, "saw", 420, 2, 0.03, 0.4),
    "entity.ravager.stunned": (900, 130, 90, "saw", 500, 3, 0.06, 0.3),
    "entity.shulker.ambient": (600, 300, 260, "pulse", 1000, 3, 0.03, 0.15),
    "entity.shulker.death": (600, 260, 90, "pulse", 800, 3, 0.02, 0.2),
    "entity.shulker.hurt": (250, 360, 220, "pulse", 1000, 3, 0.0, 0.2),
    "entity.sniffer.idle": (800, 110, 150, "saw", 420, 3, 0.03, 0.25),
    "entity.sniffer.hurt": (400, 220, 140, "saw", 500, 3, 0.0, 0.25),
    "entity.sniffer.death": (1000, 180, 60, "saw", 420, 3, 0.02, 0.3),
    "entity.sniffer.happy": (600, 130, 200, "saw", 450, 3, 0.04, 0.2),
    "entity.sniffer.searching": (700, 100, 130, "saw", 400, 3, 0.03, 0.3),
    "entity.vex.ambient": (500, 900, 1100, "saw", 1800, 3, 0.06, 0.12),
    "entity.vex.charge": (600, 600, 1600, "saw", 1800, 3, 0.05, 0.15),
    "entity.vex.death": (500, 1000, 300, "saw", 1500, 3, 0.04, 0.15),
    "entity.vex.hurt": (250, 1300, 800, "saw", 1900, 3, 0.0, 0.15),
    "entity.villager.celebrate": (800, 230, 400, "pulse", 1100, 3, 0.06, 0.12),
    "entity.villager.no": (500, 260, 170, "pulse", 950, 3, 0.02, 0.12),
    "entity.villager.trade": (450, 240, 320, "pulse", 1050, 3, 0.02, 0.12),
    "entity.villager.yes": (400, 210, 310, "pulse", 1000, 3, 0.02, 0.12),
    "entity.vindicator.ambient": (800, 150, 120, "saw", 650, 4, 0.02, 0.2),
    "entity.vindicator.celebrate": (900, 160, 300, "saw", 700, 4, 0.08, 0.2),
    "entity.vindicator.death": (900, 210, 70, "saw", 650, 4, 0.02, 0.2),
    "entity.vindicator.hurt": (380, 250, 150, "saw", 800, 4, 0.0, 0.2),
    "entity.warden.agitated": (1000, 70, 95, "saw", 250, 2, 0.03, 0.35),
    "entity.warden.ambient": (1500, 60, 52, "saw", 230, 2, 0.01, 0.35),
    "entity.warden.angry": (1000, 65, 110, "saw", 260, 2, 0.04, 0.4),
    "entity.warden.death": (2000, 70, 28, "saw", 240, 2, 0.02, 0.45),
    "entity.warden.hurt": (500, 110, 60, "saw", 280, 2, 0.0, 0.4),
    "entity.ender_dragon.growl": (1500, 90, 60, "saw", 300, 2, 0.02, 0.4),
    "entity.warden.listening": (900, 75, 85, "saw", 240, 2, 0.02, 0.3),
    "entity.warden.listening_angry": (900, 70, 100, "saw", 260, 2, 0.04, 0.35),
    "entity.warden.roar": (2200, 55, 110, "saw", 250, 2, 0.03, 0.45),
    "entity.wolf.growl": (800, 115, 90, "saw", 600, 3, 0.05, 0.2),
    "entity.wolf.howl": (2400, 380, 520, "sine", 900, 3, 0.03, 0.08),
    "entity.wolf.whine": (700, 750, 420, "saw", 1300, 3, 0.05, 0.1),
    "entity.wither.spawn": (3500, 45, 130, "saw", 320, 2, 0.02, 0.45),
    "block.sculk_shrieker.shriek": (2000, 1300, 2700, "saw", 2300, 2, 0.08, 0.35),
    # imitation bases (not events)
    "imitate.creeper": (500, 400, 250, "pulse", 900, 2, 0.0, 0.7),
    "imitate.skeleton": (600, 200, 160, "pulse", 1200, 4, 0.0, 0.4),
    "imitate.spider": (400, 700, 500, "saw", 1500, 2, 0.0, 0.6),
}
T3_LOOP_VOICES = {
    "entity.bat.loop": (500, 3200, 3800, "saw", 4200, 4, 0.0, 0.05),
    "entity.bee.loop": (400, 200, 215, "saw", 700, 3, 0.04, 0.25),
    "entity.bee.loop_aggressive": (400, 250, 300, "saw", 900, 3, 0.06, 0.3),
}
# new event -> (base event/row, pitch factor, duration factor, formant factor)   (resolved in order: a base may be an earlier derived one)
T3_DERIVED = [
    ("entity.endermite.ambient", "entity.silverfish.ambient", 0.85, 1.3, 0.9), ("entity.endermite.hurt", "entity.silverfish.hurt", 0.85, 1.3, 0.9),
    ("entity.endermite.death", "entity.silverfish.death", 0.85, 1.3, 0.9),
    ("entity.elder_guardian.ambient", "entity.guardian.ambient", 0.6, 1.4, 0.7), ("entity.elder_guardian.ambient_land", "entity.guardian.ambient_land", 0.6, 1.4, 0.7),
    ("entity.elder_guardian.curse", "entity.guardian.attack", 0.5, 2.2, 0.6), ("entity.elder_guardian.death", "entity.guardian.death", 0.6, 1.4, 0.7),
    ("entity.elder_guardian.death_land", "entity.guardian.death_land", 0.6, 1.4, 0.7), ("entity.elder_guardian.hurt", "entity.guardian.hurt", 0.6, 1.3, 0.7),
    ("entity.elder_guardian.hurt_land", "entity.guardian.hurt_land", 0.6, 1.3, 0.7),
    ("entity.goat.screaming.ambient", "entity.goat.ambient", 1.7, 0.8, 1.5), ("entity.goat.screaming.death", "entity.goat.death", 1.7, 0.8, 1.5),
    ("entity.goat.screaming.hurt", "entity.goat.hurt", 1.7, 0.8, 1.5),
    ("entity.hoglin.angry", "entity.hoglin.hurt", 0.9, 1.3, 0.9), ("entity.hoglin.converted_to_zombified", "entity.hoglin.death", 1.1, 0.8, 1.0),
    ("entity.hoglin.retreat", "entity.hoglin.ambient", 1.3, 0.7, 1.1),
    ("entity.horse.angry", "entity.horse.hurt", 0.85, 1.3, 0.9), ("entity.husk.converted_to_zombie", "entity.husk.death", 1.1, 0.7, 1.0),
    ("entity.illusioner.ambient", "entity.evoker.ambient", 0.85, 1.0, 0.9), ("entity.illusioner.hurt", "entity.evoker.hurt", 0.85, 1.0, 0.9),
    ("entity.illusioner.death", "entity.evoker.death", 0.85, 1.0, 0.9), ("entity.illusioner.prepare_blindness", "entity.evoker.prepare_attack", 0.9, 1.0, 0.9),
    ("entity.illusioner.prepare_mirror", "entity.evoker.prepare_summon", 0.9, 1.0, 0.9),
    ("entity.llama.angry", "entity.llama.hurt", 0.8, 1.5, 0.9), ("entity.mule.angry", "entity.donkey.angry", 0.9, 1.0, 0.95),
    ("entity.piglin.admiring_item", "entity.piglin.ambient", 1.1, 1.4, 0.9), ("entity.piglin.angry", "entity.piglin.hurt", 0.85, 1.5, 0.9),
    ("entity.piglin.celebrate", "entity.piglin.ambient", 1.25, 1.5, 1.0), ("entity.piglin.jealous", "entity.piglin.ambient", 0.8, 1.3, 1.0),
    ("entity.piglin.retreat", "entity.piglin.hurt", 1.15, 0.9, 1.1), ("entity.piglin.converted_to_zombified", "entity.piglin.death", 1.0, 1.3, 0.9),
    ("entity.piglin_brute.ambient", "entity.piglin.ambient", 0.75, 1.1, 0.9), ("entity.piglin_brute.angry", "entity.piglin.hurt", 0.7, 1.5, 0.9),
    ("entity.piglin_brute.death", "entity.piglin.death", 0.75, 1.2, 0.9), ("entity.piglin_brute.hurt", "entity.piglin.hurt", 0.75, 1.0, 0.9),
    ("entity.piglin_brute.converted_to_zombified", "entity.piglin.death", 0.75, 1.3, 0.9),
    ("entity.pillager.celebrate", "entity.pillager.ambient", 1.2, 1.2, 1.0), ("entity.polar_bear.warning", "entity.polar_bear.hurt", 0.8, 1.8, 1.0),
    ("entity.puffer_fish.death", "entity.cod.death", 0.8, 1.2, 0.9), ("entity.puffer_fish.hurt", "entity.cod.hurt", 0.8, 1.0, 0.9),
    ("entity.ravager.ambient", "entity.hoglin.ambient", 0.6, 1.5, 0.9), ("entity.ravager.attack", "entity.hoglin.hurt", 0.55, 1.2, 0.9),
    ("entity.ravager.celebrate", "entity.hoglin.ambient", 0.5, 1.6, 0.9), ("entity.ravager.death", "entity.hoglin.death", 0.55, 1.4, 0.9),
    ("entity.ravager.hurt", "entity.hoglin.hurt", 0.6, 1.1, 0.9),
    ("entity.glow_squid.ambient", "entity.squid.ambient", 1.25, 1.0, 1.1), ("entity.glow_squid.death", "entity.squid.death", 1.25, 1.0, 1.1),
    ("entity.glow_squid.hurt", "entity.squid.hurt", 1.25, 1.0, 1.1),
    ("entity.skeleton_horse.ambient", "entity.horse.ambient", 1.4, 0.8, 1.3), ("entity.skeleton_horse.death", "entity.horse.death", 1.4, 0.8, 1.3),
    ("entity.skeleton_horse.hurt", "entity.horse.hurt", 1.4, 0.8, 1.3),
    ("entity.strider.happy", "entity.strider.ambient", 1.1, 0.7, 1.0), ("entity.strider.retreat", "entity.strider.hurt", 1.15, 0.9, 1.0),
    ("entity.stray.ambient", "entity.wither_skeleton.ambient", 1.25, 0.9, 1.2), ("entity.stray.hurt", "entity.wither_skeleton.hurt", 1.25, 1.0, 1.2),
    ("entity.stray.death", "entity.wither_skeleton.death", 1.25, 1.0, 1.2),
    ("entity.tadpole.death", "entity.frog.death", 1.9, 0.5, 1.4), ("entity.tadpole.hurt", "entity.frog.hurt", 1.9, 0.6, 1.4),
    ("entity.tropical_fish.ambient", "entity.cod.ambient", 1.3, 1.0, 1.2), ("entity.tropical_fish.death", "entity.cod.death", 1.3, 1.0, 1.2),
    ("entity.tropical_fish.hurt", "entity.cod.hurt", 1.3, 1.0, 1.2),
    ("entity.wandering_trader.ambient", "entity.villager.ambient", 0.8, 1.2, 0.9), ("entity.wandering_trader.death", "entity.villager.death", 0.8, 1.1, 0.9),
    ("entity.wandering_trader.hurt", "entity.villager.hurt", 0.8, 1.1, 0.9), ("entity.wandering_trader.no", "entity.villager.no", 0.8, 1.1, 0.9),
    ("entity.wandering_trader.yes", "entity.villager.yes", 0.8, 1.1, 0.9), ("entity.wandering_trader.trade", "entity.villager.trade", 0.8, 1.1, 0.9),
    ("entity.witch.celebrate", "entity.witch.ambient", 1.2, 1.4, 1.1),
    ("entity.zoglin.ambient", "entity.hoglin.ambient", 0.9, 1.0, 0.95), ("entity.zoglin.angry", "entity.hoglin.hurt", 0.85, 1.3, 0.95),
    ("entity.zoglin.attack", "entity.hoglin.attack", 0.85, 1.0, 0.95), ("entity.zoglin.death", "entity.hoglin.death", 0.85, 1.0, 0.95),
    ("entity.zoglin.hurt", "entity.hoglin.hurt", 0.85, 1.0, 0.95),
    ("entity.zombie.converted_to_drowned", "entity.drowned.ambient", 1.0, 1.3, 0.8), ("entity.zombie.infect", "entity.zombie_villager.ambient", 0.8, 1.2, 0.9),
    ("entity.zombie_horse.ambient", "entity.horse.ambient", 0.75, 1.1, 0.8), ("entity.zombie_horse.death", "entity.horse.death", 0.75, 1.1, 0.8),
    ("entity.zombie_horse.hurt", "entity.horse.hurt", 0.75, 1.1, 0.8),
    ("entity.zombified_piglin.ambient", "entity.piglin.ambient", 0.8, 1.2, 0.9), ("entity.zombified_piglin.angry", "entity.piglin.hurt", 0.8, 1.4, 0.9),
    ("entity.zombified_piglin.death", "entity.piglin.death", 0.8, 1.2, 0.9), ("entity.zombified_piglin.hurt", "entity.piglin.hurt", 0.8, 1.0, 0.9),
    ("entity.zombie_villager.converted", "entity.zombie_villager.ambient", 1.3, 1.3, 1.1), ("entity.zombie_villager.cure", "entity.zombie_villager.hurt", 1.2, 1.6, 1.0),
]
# parrot imitations: the mob's ambient voice, pitched up and short
T3_IMITATE = {
    "blaze": "entity.blaze.ambient", "creeper": "imitate.creeper", "drowned": "entity.drowned.ambient", "elder_guardian": "entity.elder_guardian.ambient",
    "ender_dragon": "entity.ender_dragon.ambient", "endermite": "entity.endermite.ambient", "evoker": "entity.evoker.ambient", "ghast": "entity.ghast.ambient",
    "guardian": "entity.guardian.ambient", "hoglin": "entity.hoglin.ambient", "husk": "entity.husk.ambient", "illusioner": "entity.illusioner.ambient",
    "magma_cube": "entity.magma_cube.hurt", "phantom": "entity.phantom.ambient", "piglin": "entity.piglin.ambient", "piglin_brute": "entity.piglin_brute.ambient",
    "pillager": "entity.pillager.ambient", "ravager": "entity.ravager.ambient", "shulker": "entity.shulker.ambient", "silverfish": "entity.silverfish.ambient",
    "skeleton": "imitate.skeleton", "slime": "entity.slime.hurt", "spider": "imitate.spider", "stray": "entity.stray.ambient", "vex": "entity.vex.ambient",
    "vindicator": "entity.vindicator.ambient", "warden": "entity.warden.ambient", "witch": "entity.witch.ambient", "wither": "entity.wither.ambient",
    "wither_skeleton": "entity.wither_skeleton.ambient", "zoglin": "entity.zoglin.ambient", "zombie": "entity.zombie.ambient",
    "zombie_villager": "entity.zombie_villager.ambient",
}


def t3_voices():
    rows = {}
    for mob in MOB_VOICES:
        for call in MOB_VOICES[mob]:
            rows["entity.%s.%s" % (mob, call)] = MOB_VOICES[mob][call]
    for sid, (mob, call, pf, df, ff) in MOB_DERIVED.items():
        if mob in MOB_VOICES:
            d_, a_, b_, w_, fo_, fq_, vb_, nz_ = MOB_VOICES[mob][call]
            rows[sid] = (int(d_ * df), a_ * pf, b_ * pf, w_, fo_ * ff, fq_, vb_, nz_)
    rows["entity.zombie.hurt"] = (330, 170, 110, "saw", 650, 4, 0.0, 0.2)
    rows["entity.zombie.ambient"] = (900, 140, 100, "saw", 600, 4, 0.02, 0.2)
    rows.update(T3_VOICES)
    o = []
    for name, (dur, f0, f1, wave, form, fq, vib, nz) in T3_VOICES.items():
        if not name.startswith("imitate."):
            o.append(voice(name, ns(), dur, f0, f1, wave, form, fq, vib=vib, noise_amt=nz, variants=2, peak=-6 if wave == "sine" else -4))
    for name, (dur, f0, f1, wave, form, fq, vib, nz) in T3_LOOP_VOICES.items():
        o.append(loopify(voice(name, ns(), dur, f0, f1, wave, form, fq, vib=vib, noise_amt=nz, variants=1), 60))
    for new, base, pf, df, ff in T3_DERIVED:
        dur, f0, f1, wave, form, fq, vib, nz = rows[base]
        rows[new] = (int(dur * df), f0 * pf, f1 * pf, wave, form * ff, fq, vib, nz)
        o.append(voice(new, ns(), rows[new][0], rows[new][1], rows[new][2], wave, rows[new][4], fq, vib=vib, noise_amt=nz, variants=1,
                       peak=-6 if wave == "sine" else -4))
    for mob, base in T3_IMITATE.items():
        dur, f0, f1, wave, form, fq, vib, nz = rows[base]
        pf = max(2.0, min(8.0, 520.0 / f0))
        o.append(voice("entity.parrot.imitate." + mob, ns(), int(dur * 0.6), f0 * pf, f1 * pf, wave, min(form * 1.8, 5200), fq, vib=vib, noise_amt=nz, variants=1))
    return o


# name -> (dur, [noise layers Nz()], body (f0, f1, decay, gain, at) | None, [partials (f, gain, decay, at)], peak)
def _Pc(*a):
    return a


T3_PERC = [
    ("item.axe.strip", 220, [Nz("bp", 1800, 60, 0.8, 0, "white", 1.2), Nz("bp", 900, 50, 0.6, 60)], (200, 110, 60, 0.6, 0), [], -6),
    ("item.axe.scrape", 300, [Nz("bp", 2400, 120, 0.7, 0, "pink", 1.0)], None, [], -8),
    ("item.axe.wax_off", 260, [Nz("hp", 3000, 70, 0.6, 0, "white", 0.8), Nz("bp", 1600, 60, 0.5, 90)], None, [], -8),
    ("item.shovel.flatten", 240, [Nz("bp", 600, 90, 0.9, 0, "brown", 0.8)], (140, 80, 80, 0.8, 0), [], -6),
    ("item.hoe.till", 220, [Nz("bp", 800, 70, 0.8, 0, "brown", 0.9)], (180, 100, 60, 0.7, 0), [], -6),
    ("item.crop.plant", 180, [Nz("bp", 700, 60, 0.7, 0, "brown", 1.5)], (150, 90, 40, 0.5, 0), [], -8),
    ("block.growing_plant.crop", 200, [Nz("hp", 3500, 40, 0.6, 0, "pink"), Nz("bp", 900, 50, 0.6, 50, "brown")], None, [], -9),
    ("item.bone_meal.use", 250, [Nz("hp", 4000, 30, 0.5, 0, "white", 0.8), Nz("bp", 2500, 40, 0.5, 80), Nz("bp", 3000, 40, 0.4, 150)], None, [], -9),
    ("item.dye.use", 260, [Nz("bp", 1800, 160, 0.7, 0, "pink", 0.7)], None, [], -9),
    ("item.ink_sac.use", 280, [Nz("bp", 1200, 150, 0.8, 0, "brown", 0.7)], (300, 120, 90, 0.4, 0), [], -8),
    ("item.glow_ink_sac.use", 300, [Nz("bp", 1200, 150, 0.8, 0, "brown", 0.7)], (300, 120, 90, 0.4, 0), [(1800, 0.25, 150, 60)], -8),
    ("item.honeycomb.wax_on", 260, [Nz("bp", 1500, 100, 0.7, 0, "pink", 0.8)], (260, 160, 80, 0.4, 40), [], -8),
    ("item.honey_bottle.drink", 560, [Nz("bp", 500, 80, 0.7, 0, "brown", 0.8), Nz("bp", 450, 80, 0.7, 170, "brown", 0.8), Nz("bp", 400, 90, 0.7, 340, "brown", 0.8)],
     (180, 110, 60, 0.7, 0), [], -7),
    ("item.book.page_turn", 220, [Nz("hp", 3500, 90, 0.8, 0, "white", 0.7), Nz("bp", 5000, 50, 0.4, 110)], None, [], -9),
    ("item.book.put", 160, [Nz("bp", 1200, 40, 0.8)], (170, 100, 40, 0.7, 0), [], -7),
    ("item.bundle.insert", 200, [Nz("bp", 1400, 80, 0.8, 0, "pink", 0.8)], (240, 140, 40, 0.4, 60), [], -8),
    ("item.bundle.remove_one", 200, [Nz("bp", 1700, 70, 0.8, 0, "pink", 0.8)], (280, 170, 40, 0.4, 20), [], -8),
    ("item.bundle.drop_contents", 340, [Nz("bp", 1100, 200, 0.8, 0, "pink", 0.8), Nz("bp", 1500, 80, 0.5, 120, "pink")], (220, 120, 60, 0.4, 40), [], -8),
    ("block.cake.add_candle", 180, [Nz("bp", 1300, 50, 0.7, 0, "pink")], (320, 200, 40, 0.5, 0), [], -8),
    ("entity.chicken.egg", 200, [Nz("bp", 2000, 20, 0.5)], (400, 200, 60, 0.8, 0), [], -7),
    ("entity.cow.milk", 420, [Nz("bp", 900, 70, 0.7, 0, "pink", 0.9), Nz("bp", 1000, 70, 0.7, 130, "pink", 0.9), Nz("bp", 1100, 70, 0.7, 260, "pink", 0.9)], None, [], -9),
    ("block.chiseled_bookshelf.insert", 200, [Nz("bp", 1100, 50, 0.8), Nz("hp", 4000, 40, 0.3, 30)], (230, 130, 50, 0.7, 0), [], -7),
    ("block.chiseled_bookshelf.insert.enchanted", 400, [Nz("bp", 1100, 50, 0.8), Nz("hp", 4000, 40, 0.3, 30)], (230, 130, 50, 0.7, 0), [(2600, 0.25, 300, 40), (3900, 0.15, 250, 60)], -7),
    ("block.chiseled_bookshelf.pickup", 200, [Nz("bp", 1300, 50, 0.8), Nz("hp", 4500, 40, 0.3, 20)], (260, 160, 40, 0.6, 0), [], -7),
    ("block.chiseled_bookshelf.pickup.enchanted", 400, [Nz("bp", 1300, 50, 0.8), Nz("hp", 4500, 40, 0.3, 20)], (260, 160, 40, 0.6, 0), [(3100, 0.25, 300, 30), (4100, 0.15, 250, 50)], -7),
    ("block.decorated_pot.shatter", 420, [Nz("hp", 3000, 90, 0.9, 0, "white", 0.8), Nz("bp", 2000, 60, 0.6, 70)], None, [(2200, 0.3, 120, 0), (3100, 0.2, 90, 20)], -6),
    ("block.comparator.click", 120, [Nz("bp", 2600, 8, 0.8), Nz("bp", 2000, 10, 0.6, 30)], (800, 380, 25, 0.5, 0), [], -7),
    ("block.metal_pressure_plate.click_on", 120, [Nz("bp", 3500, 10, 0.8)], (1400, 700, 40, 0.5, 0), [(2600, 0.3, 80, 0)], -7),
    ("block.metal_pressure_plate.click_off", 110, [Nz("bp", 3000, 9, 0.8)], (1100, 560, 40, 0.5, 0), [(2300, 0.3, 70, 0)], -8),
    ("block.pointed_dripstone.land", 300, [Nz("bp", 1500, 30, 0.8)], (200, 120, 50, 0.8, 0), [(2200, 0.3, 150, 0)], -6),
    ("block.big_dripleaf.tilt_down", 260, [Nz("bp", 800, 100, 0.7, 0, "pink", 0.9)], (200, 110, 60, 0.7, 0), [], -8),
    ("block.big_dripleaf.tilt_up", 260, [Nz("bp", 1000, 100, 0.7, 0, "pink", 0.9)], (260, 150, 50, 0.6, 0), [], -8),
    ("entity.evoker_fangs.attack", 260, [Nz("bp", 3000, 30, 0.9)], (350, 120, 60, 0.8, 0), [(900, 0.3, 100, 0)], -5),
    ("block.pumpkin.carve", 260, [Nz("bp", 3000, 40, 0.8), Nz("bp", 2500, 40, 0.6, 70)], (260, 160, 50, 0.4, 0), [], -7),
    ("entity.sheep.shear", 200, [Nz("bp", 5000, 30, 0.8), Nz("bp", 4500, 30, 0.7, 80)], None, [], -8),
    ("block.sign.waxed_interact_fail", 120, [Nz("bp", 1800, 12, 0.8), Nz("bp", 1500, 12, 0.7, 45)], (300, 180, 30, 0.5, 0), [], -8),
    ("entity.iron_golem.damage", 320, [Nz("bp", 2800, 40, 0.8), Nz("bp", 2000, 60, 0.6, 90)], (200, 100, 70, 0.8, 0), [(900, 0.3, 200, 0), (1500, 0.2, 140, 0)], -5),
    ("entity.iron_golem.repair", 420, [Nz("bp", 3000, 30, 0.5)], None, [(1100, 0.5, 300, 0), (2200, 0.25, 220, 0)], -7),
    ("entity.horse.armor", 240, [Nz("bp", 3200, 40, 0.7), Nz("bp", 2600, 40, 0.6, 70)], None, [(1700, 0.3, 120, 0)], -8),
    ("entity.horse.saddle", 240, [Nz("bp", 900, 70, 0.8, 0, "pink", 0.9)], (200, 100, 50, 0.6, 0), [], -7),
    ("entity.horse.jump", 260, [Nz("bp", 700, 60, 0.6, 0, "pink", 0.9)], (170, 80, 80, 0.8, 0), [], -6),
    ("entity.horse.land", 300, [Nz("bp", 600, 90, 0.8, 0, "brown", 0.9)], (130, 60, 120, 1.0, 0), [], -5),
    ("entity.horse.gallop", 420, [Nz("bp", 1500, 25, 0.8, 0), Nz("bp", 1400, 25, 0.8, 110), Nz("bp", 1500, 25, 0.8, 210), Nz("bp", 1400, 25, 0.8, 320)],
     (150, 90, 30, 0.8, 0), [(170, 0.7, 40, 110), (150, 0.7, 40, 210), (160, 0.7, 40, 320)], -7),
    ("entity.donkey.chest", 280, [Nz("bp", 1200, 40, 0.7), Nz("bp", 3000, 30, 0.4, 60)], (220, 120, 50, 0.7, 0), [(1900, 0.2, 100, 60)], -7),
    ("entity.horse.eat", 520, [Nz("bp", 1100, 30, 0.8, 0, "brown", 1.5), Nz("bp", 1000, 30, 0.7, 130, "brown", 1.5), Nz("bp", 1200, 30, 0.8, 260, "brown", 1.5),
                              Nz("bp", 1000, 30, 0.7, 390, "brown", 1.5)], None, [], -9),
    ("entity.fox.eat", 360, [Nz("bp", 2400, 20, 0.8, 0, "pink", 2.0), Nz("bp", 2200, 20, 0.7, 90, "pink", 2.0), Nz("bp", 2500, 20, 0.7, 180, "pink", 2.0)], None, [], -10),
    ("entity.fox.bite", 200, [Nz("bp", 2000, 15, 0.8), Nz("bp", 1700, 15, 0.5, 40)], (250, 150, 40, 0.8, 0), [], -6),
    ("entity.llama.swag", 260, [Nz("bp", 900, 90, 0.8, 0, "pink", 0.8)], (180, 100, 60, 0.4, 0), [], -9),
    ("entity.llama.spit", 220, [Nz("bp", 2500, 40, 0.7, 0, "white", 1.0)], (900, 400, 60, 0.5, 0), [], -8),
    ("entity.camel.sit", 380, [Nz("bp", 700, 120, 0.8, 0, "brown", 0.9)], (120, 60, 150, 1.0, 0), [], -6),
    ("entity.camel.stand", 380, [Nz("bp", 800, 120, 0.8, 0, "brown", 0.9)], (140, 70, 130, 1.0, 40), [], -6),
    ("entity.camel.step_sand", 220, [Nz("bp", 600, 90, 0.9, 0, "brown", 0.8)], (110, 60, 80, 0.8, 0), [], -8),
    ("entity.pig.saddle", 240, [Nz("bp", 900, 70, 0.8, 0, "pink", 0.9)], (220, 110, 50, 0.6, 0), [(1500, 0.15, 50, 40)], -7),
    ("entity.rabbit.jump", 140, [Nz("bp", 700, 40, 0.6, 0, "pink", 1.0)], (200, 100, 40, 0.7, 0), [], -10),
    ("entity.cod.flop", 220, [Nz("bp", 1500, 30, 0.8, 0, "pink", 1.0), Nz("bp", 1300, 30, 0.6, 90, "pink", 1.0)], (260, 120, 40, 0.5, 0), [], -9),
    ("entity.bee.sting", 140, [Nz("bp", 4000, 20, 0.8)], (1500, 700, 50, 0.5, 0), [], -8),
    ("entity.bee.pollinate", 260, [Nz("bp", 1500, 50, 0.2)], None, [(660, 0.5, 200, 0), (990, 0.3, 160, 40)], -12),
    ("block.beehive.shear", 200, [Nz("bp", 4500, 30, 0.8), Nz("bp", 4000, 30, 0.7, 70)], (240, 140, 40, 0.3, 0), [], -8),
    ("block.beehive.work", 320, [Nz("bp", 1800, 50, 0.4, 0, "pink", 1.0)], None, [(190, 0.4, 250, 0)], -14),
    ("entity.turtle.lay_egg", 380, [Nz("bp", 700, 120, 0.8, 0, "brown", 0.9), Nz("bp", 900, 60, 0.5, 150, "brown", 0.9)], (140, 70, 100, 0.8, 0), [], -7),
    ("entity.turtle.egg_crack", 160, [Nz("bp", 2500, 15, 0.8)], (700, 400, 20, 0.4, 0), [], -8),
    ("entity.turtle.egg_break", 240, [Nz("bp", 2200, 30, 0.8), Nz("bp", 3000, 25, 0.5, 40)], (500, 250, 40, 0.5, 0), [], -7),
    ("entity.turtle.egg_hatch", 320, [Nz("bp", 2000, 40, 0.8), Nz("bp", 2800, 40, 0.5, 80)], (400, 200, 60, 0.5, 0), [(1600, 0.2, 160, 80)], -7),
    ("entity.turtle.shamble", 240, [Nz("bp", 700, 80, 0.8, 0, "brown", 0.9)], (150, 80, 80, 0.8, 0), [], -9),
    ("block.sniffer_egg.plop", 260, [Nz("bp", 700, 90, 0.8, 0, "brown", 0.9)], (180, 90, 90, 0.9, 0), [], -7),
    ("block.sniffer_egg.crack", 200, [Nz("bp", 2200, 20, 0.8)], (600, 300, 30, 0.4, 0), [], -8),
    ("block.sniffer_egg.hatch", 340, [Nz("bp", 2000, 40, 0.8), Nz("bp", 2800, 40, 0.5, 90)], (350, 170, 70, 0.6, 0), [], -7),
    ("entity.sniffer.drop_seed", 200, [Nz("bp", 1200, 40, 0.7, 0, "pink", 1.0)], (300, 150, 40, 0.5, 0), [(1900, 0.2, 100, 20)], -9),
    ("block.frogspawn.hatch", 240, [Nz("bp", 1200, 80, 0.7, 0, "pink", 1.0)], (400, 200, 60, 0.6, 0), [], -9),
    ("entity.frog.eat", 200, [Nz("bp", 1600, 30, 0.7, 0, "pink", 1.2)], (500, 250, 50, 0.7, 0), [], -8),
    ("entity.frog.long_jump", 240, [Nz("bp", 900, 60, 0.6, 0, "pink", 1.0)], (300, 140, 70, 0.8, 0), [], -8),
    ("entity.frog.step", 140, [Nz("bp", 900, 25, 0.8, 0, "pink", 1.2)], (250, 130, 30, 0.7, 0), [], -11),
    ("entity.ravager.step", 340, [Nz("bp", 500, 100, 0.8, 0, "brown", 0.9)], (100, 45, 160, 1.0, 0), [], -5),
    ("entity.warden.step", 420, [Nz("bp", 400, 140, 0.8, 0, "brown", 0.9)], (80, 35, 200, 1.0, 0), [], -4),
    ("entity.warden.attack_impact", 520, [Nz("bp", 500, 160, 0.9, 0, "brown", 0.9)], (90, 35, 260, 1.0, 0), [(55, 0.6, 300, 0)], -3),
    ("entity.warden.heartbeat", 700, [Nz("bp", 300, 80, 0.4, 0, "brown", 0.9)], (65, 40, 120, 1.0, 0), [(50, 0.8, 120, 220)], -4),
    ("entity.warden.nearby_close", 900, [Nz("bp", 300, 80, 0.4, 0, "brown", 0.9)], (65, 40, 120, 1.0, 0), [(50, 0.8, 120, 300)], -7),
    ("entity.warden.nearby_closer", 720, [Nz("bp", 300, 80, 0.5, 0, "brown", 0.9)], (65, 40, 120, 1.0, 0), [(50, 0.8, 120, 240)], -5),
    ("entity.warden.nearby_closest", 560, [Nz("bp", 300, 80, 0.6, 0, "brown", 0.9)], (65, 40, 120, 1.0, 0), [(50, 0.9, 120, 180)], -3),
    ("entity.wither.break_block", 600, [Nz("bp", 800, 300, 0.9, 0, "brown", 0.7)], (120, 40, 300, 1.0, 0), [], -3),
    ("entity.zombie.break_wooden_door", 520, [Nz("bp", 1200, 150, 0.9, 0, "white", 0.8), Nz("bp", 900, 120, 0.8, 80, "white", 0.8)], (140, 70, 150, 1.0, 0), [], -4),
    ("entity.zombie.destroy_egg", 200, [Nz("bp", 2200, 40, 0.8, 0, "pink", 1.2)], (500, 250, 40, 0.4, 0), [], -8),
    ("block.sculk_sensor.clicking", 100, [Nz("bp", 3200, 8, 0.8)], (1100, 560, 20, 0.5, 0), [], -8),
    ("block.sculk_sensor.clicking_stop", 100, [Nz("bp", 2400, 8, 0.7)], (800, 400, 20, 0.5, 0), [], -10),
    ("block.smithing_table.use", 360, [Nz("bp", 3000, 40, 0.8), Nz("bp", 2600, 40, 0.8, 160)], None, [(1700, 0.4, 200, 0), (1500, 0.4, 200, 160)], -6),
    ("item.crossbow.quick_charge_1", 260, [Nz("bp", 2800, 10, 0.8), Nz("bp", 2400, 10, 0.7, 70), Nz("bp", 3000, 12, 0.8, 150)], (300, 200, 20, 0.2, 0), [], -8),
    ("item.crossbow.quick_charge_2", 300, [Nz("bp", 2800, 10, 0.8), Nz("bp", 2400, 10, 0.7, 60), Nz("bp", 3000, 10, 0.7, 120), Nz("bp", 2600, 12, 0.8, 190)], (320, 210, 20, 0.2, 0), [], -8),
    ("item.crossbow.quick_charge_3", 340, [Nz("bp", 2800, 10, 0.8), Nz("bp", 2400, 10, 0.7, 50), Nz("bp", 3000, 10, 0.7, 100), Nz("bp", 2600, 10, 0.7, 150),
                                           Nz("bp", 3200, 12, 0.8, 220)], (340, 220, 20, 0.2, 0), [], -8),
    ("item.crossbow.loading_start", 200, [Nz("bp", 1500, 40, 0.7), Nz("bp", 2500, 12, 0.7, 90)], (300, 200, 50, 0.4, 0), [], -8),
    ("item.crossbow.loading_middle", 160, [Nz("bp", 1800, 30, 0.7), Nz("bp", 2800, 10, 0.6, 70)], (350, 220, 40, 0.4, 0), [], -8),
    ("item.crossbow.loading_end", 140, [Nz("bp", 3000, 10, 0.9), Nz("bp", 2200, 20, 0.6, 40)], (500, 250, 30, 0.6, 0), [], -7),
    ("entity.villager.work_armorer", 520, [Nz("bp", 2800, 40, 0.8), Nz("bp", 2600, 40, 0.8, 240)], (400, 200, 60, 0.4, 0), [(1800, 0.4, 200, 0), (1600, 0.4, 200, 240)], -6),
    ("entity.villager.work_weaponsmith", 560, [Nz("bp", 3200, 40, 0.8), Nz("bp", 3000, 40, 0.8, 200), Nz("bp", 3400, 40, 0.8, 380)], (300, 150, 60, 0.4, 0),
     [(2400, 0.35, 220, 0), (2100, 0.35, 220, 200), (2600, 0.3, 220, 380)], -6),
    ("entity.villager.work_butcher", 420, [Nz("bp", 900, 50, 0.8), Nz("bp", 800, 50, 0.8, 180)], (180, 90, 60, 0.9, 0), [(160, 0.7, 60, 180)], -6),
    ("entity.villager.work_cleric", 520, [Nz("bp", 800, 60, 0.4, 0, "pink", 1.0)], None, [(500, 0.4, 100, 0), (700, 0.4, 100, 130), (900, 0.4, 100, 260)], -10),
    ("entity.villager.work_fletcher", 340, [Nz("bp", 1800, 25, 0.8), Nz("bp", 1600, 25, 0.7, 120)], (400, 200, 30, 0.6, 0), [], -8),
    ("entity.villager.work_leatherworker", 360, [Nz("bp", 1000, 90, 0.8, 0, "pink", 0.8), Nz("bp", 900, 80, 0.7, 160, "pink", 0.8)], (200, 110, 50, 0.4, 0), [], -8),
    ("entity.villager.work_mason", 420, [Nz("bp", 1400, 40, 0.8), Nz("bp", 1300, 40, 0.8, 200)], (260, 130, 60, 0.8, 0), [(1900, 0.25, 120, 0)], -6),
    ("entity.slime.squish", 260, [Nz("bp", 700, 80, 0.7, 0, "pink", 0.9)], (250, 90, 110, 1.0, 0), [], -7),
    ("entity.slime.jump", 240, [Nz("bp", 800, 60, 0.5, 0, "pink", 0.9)], (110, 320, 90, 0.9, 0), [], -7),
    ("entity.slime.squish_small", 160, [Nz("bp", 1000, 50, 0.7, 0, "pink", 0.9)], (400, 150, 70, 0.9, 0), [], -8),
    ("entity.slime.jump_small", 150, [Nz("bp", 1100, 40, 0.5, 0, "pink", 0.9)], (200, 520, 60, 0.9, 0), [], -8),
    ("entity.magma_cube.squish", 280, [Nz("bp", 500, 100, 0.8, 0, "brown", 0.9)], (200, 70, 130, 1.0, 0), [], -6),
    ("entity.magma_cube.jump", 260, [Nz("bp", 600, 70, 0.6, 0, "brown", 0.9)], (90, 260, 100, 0.9, 0), [], -6),
    ("entity.magma_cube.squish_small", 180, [Nz("bp", 800, 60, 0.7, 0, "brown", 0.9)], (320, 120, 80, 0.9, 0), [], -7),
    ("block.sweet_berry_bush.pick_berries", 220, [Nz("bp", 3000, 70, 0.8, 0, "pink", 0.8), Nz("bp", 2200, 60, 0.6, 60, "pink", 0.8)], None, [], -9),
    ("entity.shulker.open", 300, [Nz("bp", 1400, 50, 0.7), Nz("bp", 2000, 30, 0.6, 90)], (240, 140, 80, 0.8, 0), [(900, 0.2, 120, 0)], -7),
    ("entity.shulker.close", 260, [Nz("bp", 1300, 40, 0.8, 0), Nz("bp", 2200, 20, 0.5, 80)], (200, 90, 70, 0.9, 0), [], -6),
    ("entity.shulker.hurt_closed", 200, [Nz("bp", 1800, 30, 0.8)], (300, 140, 60, 0.8, 0), [(700, 0.3, 120, 0)], -7),
    ("entity.shulker_bullet.hit", 240, [Nz("bp", 2500, 40, 0.8), Nz("bp", 1500, 60, 0.5, 20)], (500, 200, 70, 0.7, 0), [(1300, 0.3, 120, 0)], -7),
    ("entity.shulker_bullet.hurt", 200, [Nz("bp", 3000, 25, 0.8)], (700, 300, 50, 0.6, 0), [(1800, 0.3, 100, 0)], -8),
    ("entity.puffer_fish.sting", 160, [Nz("bp", 3500, 20, 0.8)], (900, 400, 40, 0.5, 0), [], -8),
    ("item.brush.brushing.sand.complete", 280, [Nz("bp", 2400, 60, 0.7, 0, "pink", 0.8)], (180, 100, 60, 0.5, 40), [(1200, 0.15, 100, 40)], -9),
    ("item.brush.brushing.gravel.complete", 300, [Nz("bp", 1500, 70, 0.8, 0, "brown", 0.8), Nz("bp", 2500, 20, 0.5, 60)], (200, 110, 60, 0.6, 40), [], -9),
]

# name -> (dur, cutoff0, cutoff1, color, kind, q, attack, peak, level, sub | None)   noise sweeps
T3_WHOOSH = [
    ("block.candle.extinguish", 350, 5000, 2500, "white", "hp", 0.8, 10, -9, 0.6, None),
    ("item.chorus_fruit.teleport", 600, 500, 3500, "pink", "bp", 1.2, 40, -6, 0.7, (300, 1400)),
    ("entity.enderman.teleport", 350, 400, 3000, "white", "bp", 1.5, 10, -6, 0.7, (200, 1200)),
    ("entity.fox.sniff", 300, 2500, 4000, "white", "hp", 0.8, 40, -12, 0.5, None),
    ("entity.fox.spit", 200, 2500, 1500, "white", "bp", 1.0, 5, -8, 1.0, None),
    ("entity.dolphin.jump", 500, 800, 2200, "white", "bp", 0.9, 40, -8, 0.8, None),
    ("entity.squid.squirt", 300, 1500, 500, "pink", "bp", 0.9, 5, -7, 1.0, None),
    ("entity.ender_dragon.flap", 600, 200, 500, "pink", "lp", 0.7, 80, -5, 1.0, (60, 40)),
    ("entity.phantom.flap", 400, 300, 700, "pink", "bp", 0.8, 60, -8, 0.8, None),
    ("entity.phantom.swoop", 700, 500, 2200, "white", "bp", 0.9, 150, -6, 0.9, None),
    ("entity.parrot.fly", 200, 1800, 2800, "white", "bp", 1.0, 30, -10, 0.8, None),
    ("entity.ghast.shoot", 700, 300, 1800, "white", "bp", 0.8, 20, -5, 0.8, (120, 50)),
    ("entity.blaze.shoot", 500, 800, 2800, "white", "bp", 0.9, 10, -6, 0.7, (300, 150)),
    ("entity.ender_dragon.shoot", 700, 300, 1200, "pink", "bp", 0.8, 20, -5, 0.8, (150, 60)),
    ("entity.wither.shoot", 600, 400, 2000, "white", "bp", 0.8, 10, -5, 0.8, (180, 70)),
    ("entity.snow_golem.shoot", 180, 2000, 3500, "white", "bp", 1.0, 5, -8, 1.0, None),
    ("entity.shulker.shoot", 300, 600, 2400, "white", "bp", 1.5, 5, -7, 0.6, (500, 1500)),
    ("entity.evoker.cast_spell", 900, 500, 3000, "pink", "bp", 1.5, 80, -7, 0.8, (220, 440)),
    ("entity.illusioner.mirror_move", 400, 2000, 800, "white", "bp", 1.2, 10, -8, 1.0, None),
    ("entity.warden.sniff", 600, 2200, 1000, "white", "bp", 0.8, 100, -9, 0.5, None),
    ("entity.warden.sonic_boom", 700, 3000, 300, "white", "bp", 1.2, 5, -3, 1.0, (900, 40)),
    ("entity.sniffer.scenting", 500, 1800, 2600, "white", "hp", 0.8, 80, -10, 0.6, None),
    ("entity.sniffer.sniffing", 400, 2500, 3500, "white", "hp", 0.8, 50, -10, 0.6, None),
    ("particle.soul_escape", 800, 900, 2400, "pink", "bp", 1.8, 200, -10, 0.5, (300, 500)),
    ("block.sculk.charge", 600, 300, 2500, "pink", "bp", 1.5, 100, -9, 0.7, (150, 300)),
    ("block.redstone_torch.burnout", 500, 6000, 2500, "white", "hp", 0.8, 5, -10, 0.7, None),
    ("block.smoker.smoke", 700, 3500, 2000, "white", "hp", 0.7, 100, -14, 0.5, None),
    ("entity.mooshroom.convert", 700, 500, 2500, "pink", "bp", 1.0, 20, -7, 0.8, (200, 600)),
    ("entity.wandering_trader.disappeared", 600, 600, 2800, "pink", "bp", 1.0, 30, -7, 0.7, (300, 1000)),
    ("entity.wandering_trader.reappeared", 600, 2800, 600, "pink", "bp", 1.0, 30, -7, 0.7, (1000, 300)),
    ("block.end_gateway.spawn", 1200, 200, 3000, "pink", "bp", 1.0, 200, -6, 0.8, (70, 300)),
    ("block.end_portal.spawn", 1500, 150, 2000, "pink", "bp", 1.0, 250, -6, 0.8, (50, 200)),
    ("entity.ender_eye.launch", 400, 800, 2600, "white", "bp", 1.0, 10, -8, 0.6, (500, 1800)),
    ("entity.cat.hiss", 400, 3000, 5000, "white", "hp", 0.7, 10, -8, 1.0, None),
    ("entity.panda.pre_sneeze", 400, 1000, 3000, "white", "bp", 1.0, 300, -12, 0.8, None),
    ("entity.panda.sneeze", 250, 3500, 1500, "white", "bp", 0.8, 5, -5, 0.8, (300, 150)),
    ("block.honey_block.slide", 600, 300, 200, "brown", "bp", 0.8, 50, -10, 0.9, None),
    ("block.grindstone.use", 520, 2500, 3500, "white", "bp", 3.0, 30, -9, 1.0, None),
    ("entity.frog.tongue", 150, 2000, 4000, "white", "bp", 1.0, 5, -9, 1.0, None),
    ("item.crossbow.shoot", 260, 800, 2200, "white", "bp", 1.2, 8, -6, 0.5, (220, 150)),
    ("block.beehive.enter", 280, 900, 1500, "pink", "bp", 1.0, 40, -12, 0.8, None),
    ("block.beehive.exit", 280, 1500, 900, "pink", "bp", 1.0, 20, -12, 0.8, None),
    ("item.elytra.flying", 900, 600, 1800, "pink", "bp", 0.7, 200, -12, 0.7, None),
    ("entity.puffer_fish.blow_up", 500, 400, 1500, "white", "bp", 0.9, 80, -8, 0.8, (200, 500)),
    ("entity.puffer_fish.blow_out", 400, 1500, 500, "white", "bp", 0.9, 10, -8, 0.8, (500, 200)),
    ("entity.witch.throw", 250, 800, 2000, "white", "bp", 1.0, 8, -8, 0.8, (300, 200)),
]

# name -> dict of tone() arguments (sid and seed added)
T3_TONES = [
    ("block.amethyst_block.chime", dict(dur=900, f=1568, harm=[(2.0, 0.4, 700), (3.01, 0.25, 400)], dec=700, nz=("white", "hp", 6000, 1, 10, 0.3), var=2, peak=-8)),
    ("block.amethyst_block.resonate", dict(dur=1300, f=1046, harm=[(1.5, 0.5, 1000), (2.0, 0.4, 900), (3.0, 0.2, 600)], dec=1000, vib=0.004, var=2, peak=-8)),
    ("block.chorus_flower.grow", dict(dur=400, f=520, harm=[(1.5, 0.3, 300)], dec=300, peak=-10)),
    ("block.chorus_flower.death", dict(dur=500, f=400, sweep=(400, 200, 400), dec=400, peak=-8)),
    ("block.conduit.activate", dict(dur=1000, f=220, sweep=(110, 440, 900), harm=[(2.0, 0.4, 800)], dec=900, att=60, peak=-7)),
    ("block.conduit.deactivate", dict(dur=1000, f=220, sweep=(440, 110, 900), harm=[(2.0, 0.4, 800)], dec=900, att=20, peak=-7)),
    ("block.conduit.ambient", dict(dur=3000, f=110, harm=[(2.0, 0.3, 2500), (3.0, 0.15, 2500)], dec=2600, att=600, vib=0.004, peak=-14)),
    ("block.conduit.ambient.short", dict(dur=1500, f=110, harm=[(2.0, 0.3, 1200), (3.0, 0.15, 1200)], dec=1300, att=300, vib=0.004, peak=-14)),
    ("block.conduit.attack.target", dict(dur=500, f=880, sweep=(880, 300, 400), harm=[(1.5, 0.3, 300)], dec=400, peak=-7)),
    ("item.lodestone_compass.lock", dict(dur=600, f=660, harm=[(2.0, 0.3, 400), (3.0, 0.2, 300)], dec=450, peak=-8)),
    ("item.totem.use", dict(dur=1800, f=330, sweep=(300, 900, 1000), harm=[(1.5, 0.5, 1500), (2.0, 0.4, 1400), (2.5, 0.2, 1200)], att=100, dec=1500,
                            nz=("white", "bp", 3000, 1, 300, 0.3), peak=-5)),
    ("item.spyglass.use", dict(dur=250, f=1200, sweep=(900, 1400, 200), dec=150, nz=("white", "hp", 4000, 1, 8, 0.4), peak=-10)),
    ("item.spyglass.stop_using", dict(dur=250, f=1200, sweep=(1400, 900, 200), dec=150, nz=("white", "hp", 4000, 1, 8, 0.4), peak=-10)),
    ("block.respawn_anchor.charge", dict(dur=1200, f=110, wave="saw", lp=1200, sweep=(110, 330, 1000), harm=[(2.0, 0.3, 900)], dec=1000, att=40, peak=-7)),
    ("block.respawn_anchor.deplete", dict(dur=1100, f=110, wave="saw", lp=1000, sweep=(330, 80, 900), harm=[(2.0, 0.3, 800)], dec=900, att=20, peak=-7)),
    ("block.respawn_anchor.set_spawn", dict(dur=1000, f=440, harm=[(1.5, 0.5, 800), (2.0, 0.3, 800)], dec=800, peak=-7)),
    ("block.respawn_anchor.ambient", dict(dur=3000, f=80, wave="saw", lp=400, harm=[(2.0, 0.2, 2500)], dec=2600, att=800, vib=0.006, peak=-16)),
    ("block.end_portal_frame.fill", dict(dur=500, f=700, harm=[(1.5, 0.4, 350)], dec=300, nz=("white", "bp", 2500, 2, 30, 0.8), peak=-7)),
    ("entity.allay.item_given", dict(dur=300, f=1300, sweep=(1000, 1600, 200), dec=220, vib=0.01, peak=-10)),
    ("entity.allay.item_taken", dict(dur=300, f=1300, sweep=(1600, 1000, 200), dec=220, vib=0.01, peak=-10)),
    ("entity.allay.item_thrown", dict(dur=250, f=1500, sweep=(1200, 1800, 150), dec=150, peak=-10)),
    ("entity.tadpole.grow_up", dict(dur=400, f=500, sweep=(400, 800, 300), harm=[(1.5, 0.3, 250)], dec=300, peak=-9)),
    ("event.raid.horn", dict(dur=3000, f=233, wave="saw", lp=900, att=150, dec=2400, harm=[(1.5, 0.3, 2000), (2.0, 0.5, 2200)], vib=0.006, fo=400, peak=-5)),
    ("entity.warden.sonic_charge", dict(dur=2800, f=300, wave="saw", lp=2500, sweep=(200, 1400, 2800), harm=[(2.0, 0.3, 2400)], att=400, dec=2500, vib=0.01, peak=-6)),
    ("block.pointed_dripstone.drip_water_into_cauldron", dict(dur=300, f=1200, sweep=(900, 1500, 80), harm=[(2.0, 0.2, 120)], dec=150, peak=-11)),
    ("block.pointed_dripstone.drip_lava_into_cauldron", dict(dur=360, f=650, sweep=(500, 800, 100), dec=200, nz=("brown", "bp", 700, 1.5, 120, 0.4), peak=-11)),
    ("block.pointed_dripstone.drip_lava", dict(dur=260, f=800, sweep=(600, 1000, 60), dec=120, nz=("pink", "bp", 1000, 1.5, 40, 0.3), peak=-11)),
    ("block.sculk_catalyst.bloom", dict(dur=700, f=90, sweep=(90, 200, 200), harm=[(2.0, 0.4, 500)], dec=500, nz=("pink", "bp", 800, 1.5, 300, 0.4), peak=-7)),
    ("block.beehive.drip", dict(dur=260, f=550, sweep=(420, 700, 70), harm=[(2.0, 0.2, 100)], dec=140, nz=("pink", "bp", 800, 1.5, 40, 0.3), peak=-11)),
]
T3_GOAT_HORN = [329.6, 392.0, 440.0, 493.9, 523.3, 587.3, 659.3, 784.0]

# name -> (dur, grains n, cutoff, color, decay, pitch|None)
T3_CRACKLE = [
    ("block.candle.ambient", 700, 3, 3500, "white", 12, None),
    ("block.sculk.spread", 400, 4, 700, "brown", 30, None),
    ("entity.warden.dig", 1000, 14, 500, "brown", 40, None),
    ("entity.warden.emerge", 1800, 22, 400, "brown", 60, None),
    ("entity.warden.tendril_clicks", 700, 10, 3500, "white", 6, None),
    ("entity.wolf.pant", 900, 5, 2500, "white", 90, None),
    ("entity.wolf.shake", 500, 14, 3000, "white", 10, None),
    ("entity.sniffer.digging", 700, 8, 600, "brown", 50, None),
    ("entity.sniffer.digging_stop", 300, 3, 600, "brown", 40, None),
    ("item.brush.brushing.generic", 600, 8, 3500, "pink", 15, None),
    ("item.brush.brushing.sand", 600, 9, 3000, "pink", 18, None),
    ("item.brush.brushing.gravel", 600, 9, 1800, "brown", 22, None),
    ("entity.skeleton.converted_to_stray", 800, 12, 2500, "white", 20, None),
]

# music discs: name -> (root note, scale, step ms, density, attack, decay, h2)
T3_DISCS = [
    ("13", 52, [0, 3, 5, 7, 10], 900, 0.5, 30, 1800, 0.2), ("cat", 60, [0, 4, 7, 9], 420, 0.8, 10, 900, 0.3),
    ("blocks", 57, [0, 2, 3, 7, 9], 300, 0.9, 5, 500, 0.4), ("chirp", 64, [0, 2, 4, 7, 9], 260, 0.85, 5, 600, 0.35),
    ("far", 62, [0, 2, 5, 7, 9], 700, 0.6, 20, 1600, 0.25), ("mall", 60, [0, 4, 7, 11], 380, 0.8, 8, 900, 0.3),
    ("mellohi", 55, [0, 3, 7, 10], 600, 0.6, 30, 1800, 0.2), ("stal", 50, [0, 3, 5, 6, 7, 10], 520, 0.6, 20, 1400, 0.25),
    ("strad", 65, [0, 2, 4, 5, 7, 9, 11], 340, 0.9, 6, 700, 0.35), ("ward", 48, [0, 2, 3, 7, 8], 800, 0.55, 40, 2000, 0.2),
    ("11", 45, [0, 1, 6], 1100, 0.35, 20, 2400, 0.1), ("wait", 59, [0, 2, 4, 7, 9], 450, 0.75, 12, 1100, 0.3),
    ("pigstep", 47, [0, 3, 5, 7, 10], 280, 0.9, 4, 700, 0.3), ("otherside", 56, [0, 2, 3, 5, 7, 8], 500, 0.7, 15, 1500, 0.25),
    ("5", 51, [0, 2, 3, 6, 7], 650, 0.6, 25, 1700, 0.15), ("relic", 58, [0, 3, 5, 7, 9], 560, 0.65, 18, 1600, 0.25),
]


def t3_disc(name, root, scale, step, density, attack, decay, h2):
    nodes = [
        {"id": "m1", "type": "notes", "root": root + 12, "scale": scale, "octaves": 2, "step_ms": step, "density": density,
         "attack_ms": attack, "decay_ms": decay, "h2": h2, "h3": 0.08, "gain": 0.3},
        {"id": "m2", "type": "notes", "root": root - 12, "scale": scale[:3], "octaves": 1, "step_ms": step * 4, "density": 0.9,
         "attack_ms": attack * 4, "decay_ms": decay * 2, "h2": 0.1, "h3": 0.0, "gain": 0.3},
        {"id": "mx", "type": "mix", "in": [{"src": "m1", "gain": 1.0}, {"src": "m2", "gain": 0.8}]},
        {"id": "o", "type": "reverb", "in": "mx", "room": 0.6, "damp": 0.5, "mix": 0.25},
    ]
    return sound("music_disc." + name, 14000, ns(), nodes, "o", variants=1, peak=-7, bus="music", fi=100, fo=600, preview=0)


def tier3_sounds():
    o = t3_voices()
    for name, dur, nz, body, parts, peak in T3_PERC:
        o.append(perc(name, ns(), dur, nz=nz, body=body, parts=parts, var=1, peak=peak))
    for name, dur, c0, c1, color, kind, q, att, peak, level, sub in T3_WHOOSH:
        o.append(whoosh(name, ns(), dur, c0, c1, color, kind, q, att=att, var=1, peak=peak, level=level, sub=sub))
        if name == "item.elytra.flying":
            loopify(o[-1], 150)
    for name, kw in T3_TONES:
        kw = dict(kw)
        o.append(tone(name, ns(), kw.pop("dur"), kw.pop("f"), **kw))
        if name in ("block.conduit.ambient", "block.conduit.ambient.short", "block.respawn_anchor.ambient"):
            loopify(o[-1], 300)
    for k, f in enumerate(T3_GOAT_HORN):
        o.append(tone("item.goat_horn.sound.%d" % k, ns(), 2200, f, wave="saw", lp=2200, att=80, dec=1800, vib=0.008, harm=[(2.0, 0.3, 1400)], fo=300, peak=-6))
    for name, dur, n, cut, color, dec, pitch in T3_CRACKLE:
        o.append(crackle(name, ns(), dur, n, cut, color=color, dec=dec, var=1, peak=-8, pitch=pitch))
    o.append(crackle("entity.blaze.burn", ns(), 700, 14, 1800, color="white", dec=14, var=1, peak=-8))
    for name, root, scale, step, density, attack, decay, h2 in T3_DISCS:
        o.append(t3_disc(name, root, scale, step, density, attack, decay, h2))
    o.append(sound("intentionally_empty", 50, ns(), [noise("o")], "o", variants=1, peak=-50, fi=1, fo=1))
    return o


# existing/new target event -> new alias events (resolved against every event name, see mc()); a tuple = first candidate that exists
T3_ALIASES = [
    (("block.wooden_door.open",), ["block.%s_door.open" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_door.close",), ["block.%s_door.close" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_trapdoor.open",), ["block.%s_trapdoor.open" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_trapdoor.close",), ["block.%s_trapdoor.close" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_button.click_on",), ["block.%s_button.click_on" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_button.click_off",), ["block.%s_button.click_off" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_pressure_plate.click_on",), ["block.%s_pressure_plate.click_on" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wooden_pressure_plate.click_off",), ["block.%s_pressure_plate.click_off" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.fence_gate.open",), ["block.%s_fence_gate.open" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.fence_gate.close",), ["block.%s_fence_gate.close" % w for w in ("bamboo_wood", "cherry_wood", "nether_wood")]),
    (("block.wood.hit",), ["entity.armor_stand.hit"]), (("block.wood.fall",), ["entity.armor_stand.fall"]),
    (("block.wood.place",), ["entity.armor_stand.place", "entity.glow_item_frame.place"]),
    (("block.wood.break",), ["entity.armor_stand.break", "entity.glow_item_frame.break"]),
    (("entity.item_frame.add_item", "block.wood.place"), ["entity.glow_item_frame.add_item"]),
    (("entity.item_frame.remove_item", "block.wood.break"), ["entity.glow_item_frame.remove_item"]),
    (("entity.item_frame.rotate_item", "block.wood.hit"), ["entity.glow_item_frame.rotate_item"]),
    (("block.fire.ambient",), ["block.blastfurnace.fire_crackle"]),
    (("entity.horse.eat",), ["entity.goat.eat", "entity.camel.eat", "entity.donkey.eat", "entity.mule.eat", "entity.llama.eat",
                                                         "entity.strider.eat", "entity.sniffer.eat", "entity.goat.screaming.eat"]),
    (("entity.fox.eat",), ["entity.cat.eat", "entity.parrot.eat", "entity.panda.eat", "entity.dolphin.eat", "entity.mooshroom.eat"]),
    (("entity.cow.milk",), ["entity.goat.milk", "entity.goat.screaming.milk", "entity.mooshroom.milk", "entity.mooshroom.suspicious_milk"]),
    (("entity.sheep.shear",), ["entity.snow_golem.shear", "entity.mooshroom.shear"]),
    (("entity.fox.bite",), ["entity.panda.bite", "entity.phantom.bite"]),
    (("entity.donkey.chest",), ["entity.mule.chest", "entity.llama.chest"]),
    (("entity.horse.saddle",), ["entity.camel.saddle", "entity.strider.saddle"]),
    (("entity.cow.step",), ["entity.camel.step", "entity.sniffer.step", "entity.goat.step"]),
    (("entity.zombie.step",), ["entity.zoglin.step", "entity.piglin_brute.step"]),
    (("entity.spider.step",), ["entity.endermite.step"]), (("entity.skeleton.step",), ["entity.stray.step"]),
    (("entity.generic.splash",), ["entity.dolphin.splash", "entity.axolotl.splash", "entity.hostile.splash", "entity.skeleton_horse.step_water",
                                  "entity.skeleton_horse.gallop_water", "entity.skeleton_horse.jump_water", "entity.skeleton_horse.swim"]),
    (("entity.generic.swim", "entity.generic.splash"), ["entity.dolphin.swim", "entity.axolotl.swim", "entity.hostile.swim", "entity.turtle.swim",
                                                        "entity.drowned.swim", "entity.fish.swim"]),
    (("entity.skeleton_horse.ambient",), ["entity.skeleton_horse.ambient_water"]),
    (("entity.squid.squirt",), ["entity.glow_squid.squirt"]),
    (("entity.enderman.teleport",), ["entity.fox.teleport", "entity.shulker.teleport"]),
    (("entity.cod.flop",), ["entity.salmon.flop", "entity.tropical_fish.flop", "entity.puffer_fish.flop", "entity.guardian.flop", "entity.elder_guardian.flop",
                            "entity.tadpole.flop"]),
    (("entity.arrow.shoot",), ["entity.skeleton.shoot", "entity.drowned.shoot", "entity.experience_bottle.throw", "entity.lingering_potion.throw"]),
    (("entity.arrow.hit",), ["item.crossbow.hit"]),
    (("entity.evoker.cast_spell",), ["entity.illusioner.cast_spell"]),
    (("item.honey_bottle.drink",), ["entity.wandering_trader.drink_milk", "entity.wandering_trader.drink_potion", "entity.witch.drink"]),
    (("item.hoe.till",), ["entity.villager.work_farmer"]), (("entity.generic.splash",), ["entity.villager.work_fisherman"]),
    (("item.book.page_turn",), ["entity.villager.work_librarian", "entity.villager.work_cartographer"]),
    (("entity.sheep.shear",), ["entity.villager.work_shepherd"]), (("block.grindstone.use",), ["entity.villager.work_toolsmith"]),
    (("item.crop.plant",), ["item.nether_wart.plant"]),
    (("block.glass.break",), ["entity.ender_eye.death"]),
    (("entity.generic.big_fall",), ["entity.hostile.big_fall"]), (("entity.generic.small_fall",), ["entity.hostile.small_fall"]),
    (("entity.turtle.shamble",), ["entity.turtle.shamble_baby"]),
    (("block.wood.break", "entity.item_frame.break"), ["entity.goat.horn_break", "entity.goat.screaming.horn_break"]),
    (("block.sweet_berry_bush.pick_berries",), ["block.cave_vines.pick_berries"]),
    (("item.goat_horn.sound.0",), ["item.goat_horn.play"]),
    (("entity.player.big_fall", "entity.generic.big_fall"), ["entity.goat.prepare_ram", "entity.goat.ram_impact", "entity.goat.screaming.prepare_ram",
                                                           "entity.goat.screaming.ram_impact", "entity.goat.long_jump", "entity.goat.screaming.long_jump"]),
]


MUSIC_NAMES = (["music.overworld." + x for x in ("deep_dark", "dripstone_caves", "grove", "jagged_peaks", "lush_caves", "swamp", "forest",
                "old_growth_taiga", "meadow", "cherry_grove", "frozen_peaks", "snowy_slopes", "stony_peaks", "flower_forest", "desert", "badlands",
                "jungle", "sparse_jungle", "bamboo_jungle")]
               + ["music.nether." + x for x in ("basalt_deltas", "crimson_forest", "nether_wastes", "soul_sand_valley", "warped_forest")]
               + ["music.credits", "music.dragon", "music.end", "music.under_water"])
EXTRA_ALIASES = {
    "music.ambient_loop": MUSIC_NAMES,
    "ambient.cave": [b + ".loop" for b in ("ambient.basalt_deltas", "ambient.crimson_forest", "ambient.nether_wastes", "ambient.soul_sand_valley",
                                           "ambient.warped_forest", "ambient.underwater")],
    "ambient.drip": ["ambient.underwater.loop.additions", "ambient.underwater.loop.additions.rare", "ambient.underwater.loop.additions.ultra_rare"],
    # Tier 2: ambient / hurt / death of every mob has its own voice (mob_voices()); only steps and note block imitations stay aliases
    "entity.zombie.ambient": ["block.note_block.imitate.zombie"],
    "entity.zombie.death": ["block.note_block.imitate.ender_dragon"],
    "entity.zombie.step": [f"entity.{m}.step" for m in ("husk", "zombie_villager", "drowned", "piglin", "wither_skeleton", "iron_golem")],
    "entity.cow.step": ["entity.horse.step", "entity.horse.step_wood", "entity.llama.step",
                        "entity.wolf.step", "entity.hoglin.step", "entity.polar_bear.step", "entity.panda.step",
                        "entity.parrot.step"],
    "entity.spider.step": ["entity.silverfish.step"],
    "block.fire.ambient": ["block.campfire.crackle", "block.furnace.fire_crackle"],
    "block.wood.place": ["entity.item_frame.place", "entity.painting.place", "entity.leash_knot.place"],
    "block.wood.break": ["entity.item_frame.break", "entity.painting.break", "entity.leash_knot.break"],
    "block.glass.break": ["entity.splash_potion.break"],
    "entity.generic.explode": ["entity.dragon_fireball.explode"],
    "entity.generic.splash": ["entity.fishing_bobber.splash"],
    "entity.lightning_bolt.impact": ["item.trident.thunder"],
    "ui.button.click": ["ui.loom.select_pattern", "ui.stonecutter.select_recipe"],
    "entity.item.pickup": ["ui.loom.take_result", "ui.stonecutter.take_result", "ui.cartography_table.take_result"],
    "entity.skeleton.ambient": ["block.note_block.imitate.skeleton", "block.note_block.imitate.wither_skeleton"],
    "entity.creeper.hurt": ["block.note_block.imitate.creeper"],
    "entity.pig.ambient": ["block.note_block.imitate.piglin"],
}


# ------------------------------------------------------------------------------------------------ mob steps e2 (own voices)
# pig / sheep / chicken used to alias the cow step. Own seed range 40000+ (explicit, NO ns()), appended at the END of mc() so no earlier
# recipe changes. Order = order of seeds: only append at the END.
def mob_steps_e2():
    o = []
    # pig: two hard hoof taps (higher and drier than the cow), a little click
    o.append(perc("entity.pig.step", 40000, 110, nz=[("white", "bp", 1800, 1.6, 14, 0.8, 0), ("pink", "bp", 800, 1.0, 30, 0.6, 0)],
                  body=(170, 95, 45, 0.7, 0), var=3, peak=-11))
    # sheep: soft, wool-padded thud, almost no click
    o.append(perc("entity.sheep.step", 40003, 150, nz=[("brown", "bp", 450, 0.8, 50, 0.9, 0)], body=(95, 55, 65, 0.9, 0), var=3, peak=-13))
    # chicken: tiny scratch-tap, very short and bright, quiet
    o.append(perc("entity.chicken.step", 40006, 60, nz=[("white", "bp", 3200, 1.8, 9, 0.8, 0)], body=(420, 260, 18, 0.35, 0), var=2, peak=-16))
    return o


def mc():
    sounds = []
    for mat in MATS:
        for act in ACTS:
            s = block(mat, act)
            sounds.append(s)
    sounds += ambience() + uisounds() + entities() + effects() + music() + coverage_sounds() + mob_voices() + tier3_sounds() + mob_steps_e2()
    byid = {x["id"]: x for x in sounds}
    for sid, names in EXTRA_ALIASES.items():
        byid[sid].setdefault("aliases", []).extend(names)
    owner = {}
    for x in sounds:
        for nm in [x["id"]] + x.get("aliases", []):
            owner.setdefault(nm, x)
    for cands, names in T3_ALIASES:
        tgt = next((owner[c] for c in cands if c in owner), None)
        assert tgt is not None, "tier 3 alias target missing: %r" % (cands,)
        tgt.setdefault("aliases", []).extend(names)
    seen = set()
    for x in sounds:
        for nm in [x["id"]] + x.get("aliases", []):
            assert nm not in seen, "duplicate event name " + nm
            seen.add(nm)
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
