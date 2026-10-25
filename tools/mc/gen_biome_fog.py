#!/usr/bin/env python3
import io
import json
import os
import sys
import zipfile


def inner_jar(jar):
    z = zipfile.ZipFile(jar)
    for n in z.namelist():
        if n.startswith('META-INF/versions/') and n.endswith('.jar'):
            return zipfile.ZipFile(io.BytesIO(z.read(n)))
    return z


def main():
    jar, out = sys.argv[1], sys.argv[2]
    jz = inner_jar(jar)
    rows = []
    for n in sorted(jz.namelist()):
        if n.startswith('data/minecraft/worldgen/biome/') and n.endswith('.json'):
            eff = json.loads(jz.read(n))['effects']
            rows.append((os.path.basename(n)[:-5], eff['water_fog_color'] & 0xFFFFFF))
    data = ';'.join('%s:%06X' % r for r in rows)
    src = '// SPDX-License-Identifier: MPL-2.0\n'
    src += 'import std.core\n\nexport { BF_DATA, BF_LEN }\n\n'
    src += 'const BF_LEN: usize = %d\n' % len(data)
    src += 'const BF_DATA: str = "%s"\n' % data
    open(out, 'w').write(src)
    print('biomes', len(rows))


main()
