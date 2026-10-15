#!/usr/bin/env python3 -I
# SPDX-License-Identifier: MPL-2.0
"""check_wire.py <version> <capture.jsonl> <sound_pkts output> -- compares lib/mc/mc_audio.fi's decode of REAL server packets
(tools/mc/audio/sounds_live.sh, one JSON line each: packet name, body hex, node-minecraft-protocol's parse) with that parse.

Registered sounds / stop sounds / world events: every field must equal nmp's. Inline-named sounds (the playsound command):
nmp 1.20.1 mis-parses the holder (known minecraft-data type gap), so these are checked against the console commands that sent
them (name, category, volume, pitch, offsets relative to the first one). Exit 0 = all equal.
"""
import json
import struct
import sys

CATS = ["master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient", "voice"]
# playsound commands of sounds_live.sh: name -> (category, volume, pitch, dx, dy, dz in blocks relative to ambient.cave)
INLINE = {
    "minecraft:ambient.cave": (8, 1.0, 1.0, 0, 0, 0),
    "minecraft:block.stone.break": (4, 0.75, 0.5, 4, 0, -2),
    "minecraft:entity.pig.ambient": (6, 1.0, 2.0, 0, 0, 0),
    "minecraft:block.note_block.harp": (2, 3.0, 1.5, 1, 2, 3),
    "minecraft:entity.zombie.hurt": (5, 0.25, 0.8, -8, 0, 0),
    "minecraft:ui.button.click": (0, 1.0, 1.0, 0, 0, 0),
    "custom:not.in.registry": (0, 1.0, 1.0, 0, 0, 0),
}


def fx(v):
    return int(struct.unpack(">f", struct.pack(">f", v))[0] * 65536.0)


def main():
    ver, cap, out = sys.argv[1:4]
    pkts = [json.loads(l) for l in open(cap) if l.startswith("{")]
    lines = [l.rstrip("\n") for l in open(out) if l.strip()]
    # sound_pkts skips packets without a hex body and other names; build the same filtered list
    ref = [p for p in pkts if p["hex"] and p["pkt"] in ("sound_effect", "named_sound_effect", "entity_sound_effect", "stop_sound", "world_event")]
    errs = []
    if len(ref) != len(lines):
        errs.append("packet count %d != decoded lines %d" % (len(ref), len(lines)))
    base = None
    n_reg = n_inl = n_stop = n_ev = 0
    for i, (p, l) in enumerate(zip(ref, lines)):
        f = l.split()
        kv = dict(x.split("=", 1) for x in f[3:] if "=" in x)
        if f[0] == "BAD":
            errs.append("#%d %s decoded as broken: %s" % (i, p["pkt"], p["hex"][:60]))
            continue
        d = p["p"]
        if p["pkt"] in ("sound_effect", "named_sound_effect", "entity_sound_effect"):
            inline = kv["id"] == "-1"
            if p["pkt"] == "entity_sound_effect":
                if int(kv["ent"]) != d["entityId"]:
                    errs.append("#%d entity id" % i)
            if inline:
                n_inl += 1
                nm = kv["name"]
                exp = INLINE.get(nm)
                if exp is None:
                    errs.append("#%d unexpected inline name %s" % (i, nm))
                    continue
                cat, vol, pit, dx, dy, dz = exp
                if int(kv["cat"]) != cat or int(kv["vol"]) != fx(vol) or int(kv["pitch"]) != fx(pit):
                    errs.append("#%d %s cat/vol/pitch %s" % (i, nm, l))
                pos = (int(kv["x8"]), int(kv["y8"]), int(kv["z8"]))
                if nm == "minecraft:ambient.cave" and base is None:
                    base = pos
                if base is not None and nm != "minecraft:ambient.cave" or (nm == "minecraft:ambient.cave" and False):
                    # offsets relative to the first cave sound (the player stands still); commands use ~ so y/x/z in whole blocks
                    got = tuple(pos[k] - base[k] for k in range(3))
                    if got != (dx * 8, dy * 8, dz * 8):
                        errs.append("#%d %s offset %s != %s" % (i, nm, got, (dx * 8, dy * 8, dz * 8)))
            else:
                n_reg += 1
                s = d["sound"] if "sound" in d else d  # <= 1.18.2: soundId sits directly in the packet (no holder)
                if int(kv["id"]) != s["soundId"]:
                    errs.append("#%d id %s != nmp %s" % (i, kv["id"], s["soundId"]))
                cat = CATS.index(d["soundCategory"]) if isinstance(d["soundCategory"], str) else d["soundCategory"]
                if int(kv["cat"]) != cat:
                    errs.append("#%d category" % i)
                if p["pkt"] != "entity_sound_effect":
                    if (int(kv["x8"]), int(kv["y8"]), int(kv["z8"])) != (d["x"], d["y"], d["z"]):
                        errs.append("#%d position" % i)
                if int(kv["vol"]) != fx(d["volume"]) or int(kv["pitch"]) != fx(d["pitch"]):
                    errs.append("#%d volume/pitch" % i)
        elif p["pkt"] == "stop_sound":
            n_stop += 1
            exp_name = d.get("sound", "-")
            exp_cat = d.get("source", -1) if d["flags"] & 1 else -1
            if int(kv["flags"]) != d["flags"] or int(kv["cat"]) != exp_cat or kv["name"] != exp_name:
                errs.append("#%d stop sound %s vs %s" % (i, l, d))
        elif p["pkt"] == "world_event":
            n_ev += 1
            loc = d["location"]
            if (int(kv["effect"]), int(kv["x"]), int(kv["y"]), int(kv["z"]), int(kv["data"])) != (d["effectId"], loc["x"], loc["y"], loc["z"], d["data"]):
                errs.append("#%d world event %s vs %s" % (i, l, d))
    for e in errs[:20]:
        print("  FAIL", e)
    print("%s: %d packets: %d registered, %d inline, %d stop, %d world events: %s" %
          (ver, len(ref), n_reg, n_inl, n_stop, n_ev, "ok" if not errs else "%d errors" % len(errs)))
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main())
