#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/rename.py — wendet `names.tsv` (und `names_file.tsv`)
# auf den Quelltext an. Getroffen werden NUR Bezeichner im Code; Kommentare,
# Zeichenketten und Zeichenliterale bleiben unangetastet (das ist Etappe B).
#
#   python3 tools/english/rename.py            # alles
#   python3 tools/english/rename.py compiler/src lib/std   # nur dort
#   python3 tools/english/rename.py --probe ...            # nur zeigen
#
# Symbolische Verweise (bin/*.fi -> lib/firnc1/*.fi) werden uebersprungen,
# sonst liefe dieselbe Datei zweimal durch.
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

ROOTS = ['compiler/src', 'lib', 'bin', 'tools', 'demos', 'examples', 'tests', 'bench', 'testdata']


def tabelle(pfad):
    t = {}
    if not os.path.exists(pfad):
        return t
    for z in open(pfad, encoding='utf-8'):
        if not z.strip() or z.startswith('#'):
            continue
        d, e = z.rstrip('\n').split('\t')
        t[d] = e
    return t


def main(argv):
    probe = '--probe' in argv
    argv = [a for a in argv if not a.startswith('--')]
    wurzeln = argv or ROOTS

    abb = tabelle('tools/english/names.tsv')
    prodatei = {}
    for z in open('tools/english/manual_file.tsv', encoding='utf-8'):
        if not z.strip() or z.startswith('#'):
            continue
        d, a, n = z.rstrip('\n').split('\t')
        prodatei.setdefault(d, {})[a] = n

    files = []
    for r in wurzeln:
        if os.path.isfile(r):
            files.append(r)
            continue
        for d, _, fs in os.walk(r):
            if 'target' in d or '__pycache__' in d or '.test-work' in d:
                continue
            for f in fs:
                q = os.path.join(d, f)
                if f.endswith(('.rs', '.fi')) and not os.path.islink(q):
                    files.append(q)

    ndat = 0
    nersetz = 0
    for p in sorted(files):
        s = open(p, encoding='utf-8').read()
        tab = abb
        if p in prodatei:
            tab = dict(abb)
            tab.update(prodatei[p])
        neu, k = Q.ersetze(s, tab, p.endswith('.rs'))
        if k:
            nersetz += k
            ndat += 1
            if not probe:
                open(p, 'w', encoding='utf-8').write(neu)
    print(f"{'(probe) ' if probe else ''}{ndat} dateien, {nersetz} ersetzungen")


if __name__ == '__main__':
    main(sys.argv[1:])
