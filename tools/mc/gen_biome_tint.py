#!/usr/bin/env python3
"""Generates lib/mc/mc_biome_tint.fi: biome name -> grass / foliage / water colour (0xRRGGBB).

Sources (all from files on disk, nothing typed in by hand except the 1.18 rename map):
  - minecraft-data tints.json 1.20 (explicit colours: swamp, dark forest, badlands, cherry grove, mangrove, water per biome)
  - minecraft-data tints.json 1.16.2 / 1.17 (climate colours of the OLD biome names; those versions listed the colormap result per biome)
  - server jar 1.20.1 data/minecraft/worldgen/biome/*.json (temperature / downfall of the biomes that did not exist in 1.16)
A biome that has neither an explicit colour nor an old name gets an inverse distance blend of the known climate colours at its
(temperature, temperature * downfall) point -- an APPROXIMATION of the vanilla colormap (a pack colormap would be exact).

usage: gen_biome_tint.py <minecraft-data/data/pc> <server-1.20.1.jar> <out.fi>
"""
import json, sys, os, zipfile, io

RENAMES = {  # 1.18 name -> pre-1.18 name (same biome, renamed)
    'snowy_plains': 'snowy_tundra', 'old_growth_birch_forest': 'tall_birch_forest', 'old_growth_pine_taiga': 'giant_tree_taiga',
    'old_growth_spruce_taiga': 'giant_spruce_taiga', 'sparse_jungle': 'jungle_edge', 'stony_shore': 'stone_shore',
    'windswept_hills': 'mountains', 'windswept_forest': 'wooded_mountains', 'windswept_gravelly_hills': 'gravelly_mountains',
    'windswept_savanna': 'shattered_savanna',
}


def rgb(c):
    return c & 0xFFFFFF  # signed ints from 1.17 (alpha 0xFF) -> plain 24 bit


def load_tints(path, kind):
    t = json.load(open(path))[kind]['data']
    out = {}
    for e in t:
        for k in e['keys']:
            out[k] = rgb(e['color'])
    return out


def inner_jar(jar):
    z = zipfile.ZipFile(jar)
    for n in z.namelist():
        if n.startswith('META-INF/versions/') and n.endswith('.jar'):
            return zipfile.ZipFile(io.BytesIO(z.read(n)))
    return z


def main():
    mcd, jar, outp = sys.argv[1], sys.argv[2], sys.argv[3]
    jz = inner_jar(jar)
    clim = {}
    jeff = {}  # 1.20.1 biome effects (exact: explicit colours, grass_color_modifier, water colour)
    for n in jz.namelist():
        if n.startswith('data/minecraft/worldgen/biome/') and n.endswith('.json'):
            j = json.loads(jz.read(n))
            clim[os.path.basename(n)[:-5]] = (j['temperature'], j['downfall'])
            jeff[os.path.basename(n)[:-5]] = j['effects']
    kinds = ('grass', 'foliage', 'water')
    t20 = {k: json.load(open(mcd + '/1.20/tints.json'))[k]['data'] for k in kinds}
    explicit = {k: {} for k in kinds}  # 1.20 explicit (non zero) colours
    climate_names = {k: set() for k in kinds}
    for k in kinds:
        for e in t20[k]:
            for n in e['keys']:
                if e['color'] != 0:
                    explicit[k][n] = rgb(e['color'])
                else:
                    climate_names[k].add(n)
    # biomes of the 1.20.1 jar: its effects replace the minecraft-data explicit list (that list gives dark_forest the bare modifier term and
    # mangrove_swamp the swamp foliage colour)
    for n, ef in jeff.items():
        for k, key in (('grass', 'grass_color'), ('foliage', 'foliage_color'), ('water', 'water_color')):
            explicit[k].pop(n, None)
            if key in ef:
                explicit[k][n] = ef[key] & 0xFFFFFF
        if ef.get('grass_color_modifier') == 'swamp':
            explicit['grass'][n] = 0x6A7039  # vanilla: noise picks 6A7039 / 4C763C per column; the constant is the common one
    old = {k: {} for k in kinds}
    for ver in ('1.16.2', '1.17'):
        for k in kinds:
            for n, c in load_tints(mcd + '/' + ver + '/tints.json', k).items():
                old[k].setdefault(n, c)
    names = set(clim)
    for ver in ('1.16.2', '1.17', '1.20'):
        for b in json.load(open(mcd + '/' + ver + '/biomes.json')):
            names.add(b['name'])
    # anchors for the blend: biomes with a known climate colour AND climate values
    anchors = {k: [] for k in ('grass', 'foliage')}
    for k in anchors:
        for n, c in old[k].items():
            src = n
            if src in clim and n not in explicit[k]:
                t, d = clim[src]
                anchors[k].append((min(max(t, 0.0), 1.0), min(max(t, 0.0), 1.0) * min(max(d, 0.0), 1.0), c))

    def blend(k, t, d):
        t = min(max(t, 0.0), 1.0)
        d = t * min(max(d, 0.0), 1.0)
        num = [0.0, 0.0, 0.0]
        den = 0.0
        for (at, ad, c) in anchors[k]:
            w = 1.0 / (1e-4 + (at - t) ** 2 + (ad - d) ** 2)
            den += w
            for i in range(3):
                num[i] += w * ((c >> (16 - 8 * i)) & 255)
        return (int(num[0] / den + 0.5) << 16) | (int(num[1] / den + 0.5) << 8) | int(num[2] / den + 0.5)

    default_water = 0x3F76E4
    rows = []
    approx = []
    for n in sorted(names):
        row = []
        for k in kinds:
            if n in explicit[k]:
                c = explicit[k][n]
            elif n in old[k]:
                c = old[k][n]
            elif n in RENAMES and RENAMES[n] in old[k]:
                c = old[k][RENAMES[n]]
            elif k == 'water':
                c = default_water
            elif n in clim:
                c = blend(k, clim[n][0], clim[n][1])
                approx.append((k, n))
            else:
                c = old['grass'].get('plains', 0x91BD59) if k == 'grass' else old['foliage'].get('plains', 0x77AB2F)
                approx.append((k, n))
            row.append(c)
        rows.append((n, row))
    # colormap lookup cell per biome of the jar (vanilla GrassColor.get / FoliageColor.get: x = (int)((1 - t) * 255), y = (int)((1 - rain * t) * 255)
    # with t = clamp(temperature), rain = clamp(downfall), both FLOAT values widened to double)
    import struct
    f32 = lambda v: struct.unpack('f', struct.pack('f', v))[0]
    crow = []
    for n in sorted(jeff):
        t = min(max(f32(clim[n][0]), 0.0), 1.0)
        r = min(max(f32(clim[n][1]), 0.0), 1.0) * t
        x = int((1.0 - t) * 255.0)
        y = int((1.0 - r) * 255.0)
        fl = (1 if n in explicit['grass'] else 0) | (2 if n in explicit['foliage'] else 0) | (4 if jeff[n].get('grass_color_modifier') == 'dark_forest' else 0)
        crow.append('%s:%02X%02X:%X' % (n, x, y, fl))
    cdata = ';'.join(crow)
    data = ';'.join('%s:%06X:%06X:%06X' % (n, r[0], r[1], r[2]) for n, r in rows)
    src = '// SPDX-License-Identifier: MPL-2.0\n'
    src += '// lib/mc/mc_biome_tint.fi -- GENERATED by tools/mc/gen_biome_tint.py, do not edit. Biome name -> grass / foliage / water colour.\n'
    src += '// Format: "name:GGGGGG:FFFFFF:WWWWWW;..." (hex 0xRRGGBB). Colours of biomes that had no explicit value are a blend of the known\n'
    src += '// climate colours (approximation of the vanilla colormap): %s\n' % ', '.join('%s/%s' % (k[0], n) for k, n in approx)
    src += 'import std.core\n\nexport { BT_DATA, BT_LEN, BT_CLIM, BT_CLIM_LEN }\n\n'
    src += 'const BT_LEN: usize = %d\n' % len(data)
    src += 'const BT_DATA: str = "%s"\n' % data
    src += '// Colormap cells of the 1.20.1 biomes: "name:XXYY:F" (hex x, y of the 256x256 grass.png / foliage.png cell, F bit0 grass colour explicit, bit1 foliage\n'
    src += '// explicit, bit2 grass modifier dark_forest). Biomes without a cell here (older names) use the table colours above.\n'
    src += 'const BT_CLIM_LEN: usize = %d\n' % len(cdata)
    src += 'const BT_CLIM: str = "%s"\n' % cdata
    open(outp, 'w').write(src)
    print('biomes', len(rows), 'approximated', len(approx), 'chars', len(data))
    print(approx)


main()
