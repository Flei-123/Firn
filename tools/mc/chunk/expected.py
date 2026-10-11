#!/usr/bin/env python3
# Computes the dump that demos/mcclient/selftest.fi must print, from the same
# LCG, with an independent implementation of packing, FNV-1a 64 and SHA-256.
# Usage: python3 -I unit_expected.py
import hashlib

state = 12345


def nxt():
    global state
    state = (state * 1103515245 + 12345) % 2147483648
    return state >> 8


def fnv(h, data):
    for b in data:
        h = ((h ^ b) * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return h


OFF = 14695981039346656037


def hash16(vals):
    h = OFF
    for v in vals:
        h = fnv(h, bytes((v & 255, v >> 8)))
    return h


hm = {}
for name in ("WORLD_SURFACE", "MOTION_BLOCKING"):
    hm[name] = [nxt() % 385 for _ in range(256)]

sections = []
# section 0: air + biome 1
sections.append((0, [0] * 4096, [1] * 64))
# section 1
pal = [100 + 7 * j for j in range(16)]
blocks = [pal[nxt() % 16] for _ in range(4096)]
pal_b = [1 + 4 * j for j in range(4)]
biomes = [pal_b[nxt() % 4] for _ in range(64)]
sections.append((4000, blocks, biomes))
# section 2
pal = [1000 + 13 * j for j in range(200)]
blocks = [pal[nxt() % 200] for _ in range(4096)]
biomes = [nxt() % 50 for _ in range(64)]
sections.append((4096, blocks, biomes))
# section 3
blocks = [nxt() % 20000 for _ in range(4096)]
pal_b = [3 + j for j in range(2)]
biomes = [pal_b[nxt() % 2] for _ in range(64)]
sections.append((4095, blocks, biomes))
light = bytes(nxt() % 256 for _ in range(3 * 2048))

x, z = -3, 7
out = []
out.append("mcdump 1\nprotocol 763\nchunks 1\n")
out.append("C %d %d sections=%d blockentities=%d\n" % (x, z, len(sections), 2))
for i, (cnt, blocks, biomes) in enumerate(sections):
    out.append("S %d %d %d count=%d distinct=%d blocks=%016x biomes=%016x\n" % (
        x, z, i, cnt, len(set(blocks)), hash16(blocks), hash16(biomes)))
for name in sorted(hm):
    out.append("H %d %d %s %016x\n" % (x, z, name, hash16(hm[name])))
out.append("L %d %d sky=2 block=1 h=%016x\n" % (x, z, fnv(OFF, light)))
body = "".join(out)
body += "checksum sha256 %s\n" % hashlib.sha256(body.encode()).hexdigest()
print(body, end="")
