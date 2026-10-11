#!/usr/bin/env python3
"""tools/mc/texture/check.py -- independent check of mc_texture.
  * block-state -> tile table for 1.20.1 re-derived from minecraft-data blocks.json with its own
    transcription of the rule list in docs/mc/texture.md (state ranges come from minStateId/maxStateId)
  * atlas: plain-noise tiles + dye/hash tiles + the test and unknown tiles are recomputed pixel-exact
    with a Python hash; patterned tiles (grass side, logs, planks, leaves, ...) are checked
    structurally (alpha holes, border, tile origin layout), all other texels must be opaque
usage: check.py <atlas.bin> <tiles.bin> <blocks.json>   [--bad  = run the counter-checks]
"""
import json
import struct
import sys

M = 0xFFFFFFFF


def hash3(a, b, c):
    h = (a * 0x9E3779B1 + b * 0x85EBCA6B + c * 0xC2B2AE35) & M
    h ^= h >> 15
    h = (h * 0x2C1B3C6D) & M
    h ^= h >> 12
    h = (h * 0x297A2D39) & M
    h ^= h >> 15
    return h


def noise8(t, x, y, k):
    return (hash3((t + k * 977) & M, x & M, y & M) >> 8) & 255


def clamp(v):
    return 0 if v < 0 else 255 if v > 255 else v


def noisy(t, x, y, r, g, b, amp, extra=0):
    d = (noise8(t, x, y, 0) * amp) // 128 - amp + extra
    return (clamp(r + d), clamp(g + d), clamp(b + d), 255)


DYE = [(233, 236, 236), (240, 118, 19), (189, 68, 179), (58, 175, 217), (248, 198, 39), (112, 185, 25),
       (237, 141, 172), (62, 68, 71), (142, 142, 134), (21, 137, 145), (137, 50, 184), (53, 57, 157),
       (114, 71, 40), (84, 109, 27), (160, 39, 34), (20, 21, 25)]
PLAIN = {1: (125, 125, 125, 14), 2: (134, 96, 67, 14), 3: (95, 159, 53, 16), 5: (219, 207, 142, 8),
         6: (131, 127, 126, 28), 13: (60, 60, 60, 40), 14: (245, 248, 250, 4), 15: (150, 190, 250, 10),
         19: (230, 100, 20, 50), 21: (110, 50, 50, 20), 22: (160, 166, 178, 6), 23: (80, 80, 86, 12),
         24: (152, 94, 67, 8)}


def pixel_plain(t, x, y):
    if t in PLAIN:
        r, g, b, a = PLAIN[t]
        return noisy(t, x, y, r, g, b, a)
    if 25 <= t <= 40:
        r, g, b = DYE[t - 25]
        return noisy(t, x, y, r, g, b, 6)
    if 41 <= t <= 56:
        i = t - 41
        return noisy(t, x, y, 60 + (i * 53) % 160, 60 + (i * 97) % 160, 60 + (i * 151) % 160, 14)
    if t == 57:
        return (255, 0, 255, 255) if ((x // 8) + (y // 8)) % 2 == 0 else (0, 0, 0, 255)
    if t == 58:
        if x in (0, 15) or y in (0, 15) or (2 <= x <= 3 and 2 <= y <= 3):
            return (0, 0, 0, 255)
        if y < 8:
            return (220, 40, 40, 255) if x < 8 else (40, 200, 40, 255)
        return (40, 40, 220, 255) if x < 8 else (235, 235, 235, 255)
    return None


DYE_PREFIX = [('white_', 0), ('orange_', 1), ('magenta_', 2), ('light_blue_', 3), ('yellow_', 4), ('lime_', 5),
              ('pink_', 6), ('light_gray_', 8), ('gray_', 7), ('cyan_', 9), ('purple_', 10), ('blue_', 11),
              ('brown_', 12), ('green_', 13), ('red_', 14), ('black_', 15)]


def fnv16(name):
    h = 2166136261
    for ch in name.encode():
        h = ((h ^ ch) * 16777619) & M
    return (h ^ (h >> 16)) & 15


def tiles_for(name, solid):
    z = [0] * 6
    one = lambda t: [t] * 6
    if name in ('air', 'cave_air', 'void_air', 'barrier', 'light', 'structure_void'):
        return z
    if name == 'grass_block':
        return [4, 4, 2, 3, 4, 4]
    exact = {'stone': 1, 'stone_bricks': 1, 'smooth_stone': 1, 'dirt': 2, 'coarse_dirt': 2, 'rooted_dirt': 2,
             'farmland': 2, 'dirt_path': 2, 'sand': 5, 'gravel': 6, 'cobblestone': 12, 'mossy_cobblestone': 12,
             'bedrock': 13, 'snow_block': 14, 'snow': 14, 'powder_snow': 14, 'ice': 15, 'packed_ice': 15,
             'blue_ice': 15, 'water': 11, 'bubble_column': 11, 'lava': 19, 'glass': 16, 'sandstone': 17,
             'cut_sandstone': 17, 'smooth_sandstone': 17, 'chiseled_sandstone': 17, 'bricks': 20,
             'netherrack': 21, 'clay': 22, 'deepslate': 23, 'terracotta': 24}
    if name in exact:
        return one(exact[name])
    if name.endswith('_planks'):
        return one(9)
    if name.endswith(('_log', '_wood', '_stem', '_hyphae')):
        return [7, 7, 8, 8, 7, 7]
    if name.endswith('_leaves'):
        return one(10)
    if name.endswith('_ore'):
        return one(18)
    if name.endswith('_glass'):
        return one(16)
    dye = -1
    for p, i in DYE_PREFIX:
        if name.startswith(p):
            dye = i
            break
    if name.endswith('_terracotta'):
        return one(25 + dye) if dye >= 0 else one(24)
    if dye >= 0 and solid:
        return one(25 + dye)
    if solid:
        return one(41 + fnv16(name))
    return z


def run(atlas, tiles, blocks, mutate=None):
    errs = []

    def err(m):
        if len(errs) < 12:
            errs.append(m)
    if len(atlas) != 256 * 256 * 4:
        return ['atlas size']
    px = lambda t, x, y: tuple(atlas[(((t - 1) // 16 * 16 + y) * 256 + ((t - 1) % 16) * 16 + x) * 4:][:4])
    for t in range(1, 59):
        for y in range(16):
            for x in range(16):
                ref = pixel_plain(t, x, y)
                got = px(t, x, y)
                if ref is not None and ref != got:
                    err('tile %d (%d,%d): %r != %r' % (t, x, y, got, ref))
                    break
            else:
                continue
            break
    # unused tiles stay empty
    for t in range(59, 257):
        if any(px(t, x, y) != (0, 0, 0, 0) for x in range(0, 16, 5) for y in range(0, 16, 5)):
            err('tile %d should be empty' % t)
    # structural checks of the patterned tiles
    nleaf = sum(1 for y in range(16) for x in range(16) if px(10, x, y)[3] == 0)
    if not (10 <= nleaf <= 120):
        err('leaves: %d transparent texels' % nleaf)
    if px(16, 8, 4)[3] != 0 or px(16, 0, 5)[3] != 255 or px(16, 15, 15)[3] != 255:
        err('glass tile layout')
    if any(px(11, x, y)[3] != 190 for x in range(16) for y in range(16)):
        err('water alpha')
    if px(4, 3, 0)[1] <= px(4, 3, 0)[0] or px(4, 3, 15)[0] <= px(4, 3, 15)[2] + 30:
        err('grass side: green top / dirt bottom expected')
    if any(px(t, x, y)[3] != 255 for t in (1, 2, 3, 4, 5, 7, 8, 9, 12, 17, 18, 20) for x in range(16) for y in range(16)):
        err('solid tiles must be opaque')
    # block tables
    nst = 24135
    if len(tiles) != nst * 12 + nst:
        return errs + ['tiles size %d' % len(tiles)]
    exp_t = {}
    exp_o = {}
    for b in blocks:
        ft = tiles_for(b['name'], b['boundingBox'] == 'block')
        op = 1 if (b['boundingBox'] == 'block' and not b['transparent']) else 0
        for s in range(b['minStateId'], b['maxStateId'] + 1):
            exp_t[s] = ft
            exp_o[s] = op
    for s in range(nst):
        got = list(struct.unpack_from('<6H', tiles, s * 12))
        if s not in exp_t:
            err('state %d unknown' % s)
            continue
        if got != exp_t[s]:
            err('state %d: tiles %r != %r' % (s, got, exp_t[s]))
        if tiles[nst * 12 + s] != exp_o[s]:
            err('state %d: opaque %d != %d' % (s, tiles[nst * 12 + s], exp_o[s]))
    return errs


def main():
    atlas = bytearray(open(sys.argv[1], 'rb').read())
    tiles = bytearray(open(sys.argv[2], 'rb').read())
    blocks = json.load(open(sys.argv[3]))
    errs = run(atlas, tiles, blocks)
    if errs:
        for e in errs:
            print('  FAIL ', e)
        return 1
    nz = sum(1 for s in range(24135) if struct.unpack_from('<6H', tiles, s * 12)[0] != 0)
    print('  states with a visible face: %d of 24135; atlas 256x256, 58 tiles checked' % nz)
    bad = 0
    a2 = bytearray(atlas)
    a2[((0 * 256) + 5) * 4] ^= 0x10  # stone texel
    t2 = bytearray(tiles)
    t2[12 * 1 + 4] ^= 1  # stone state 1, face +y
    t3 = bytearray(tiles)
    t3[24135 * 12 + 1] = 0  # stone opaque flag
    for name, A, T in (('atlas texel', a2, tiles), ('tile table', atlas, t2), ('opaque flag', atlas, t3)):
        if run(A, T, blocks):
            print('  ok    counter-check %s: damaged data rejected' % name)
        else:
            print('  FAIL  counter-check %s: damaged data accepted' % name)
            bad += 1
    return 1 if bad else 0


sys.exit(main())
