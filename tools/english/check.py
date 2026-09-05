#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/check.py — GEGENPROBE zur Englisch-Umstellung.

Sucht in ALLEN Bezeichnern (compiler/src, lib, bin, tools, tests, demos,
examples) nach deutschen Wortteilen aus tools/english/morphemes.tsv.
Ein Treffer heisst: dieser Name ist noch deutsch.

Kommentare, Zeichenketten und Dateikoepfe zaehlen NICHT — die sind Etappe B.
Bekannte, bewusst deutsch bleibende Namen stehen in exceptions.txt.
"""
import os, re, sys
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

ROOTS = ['compiler/src', 'lib', 'bin', 'tools', 'tests', 'demos', 'examples', 'bench']
# Ein angehaengter Zaehler versteckt das deutsche Wort: 'pfad2', 'teil1'.
ZIFFERNSCHWANZ = re.compile(r'[0-9]+$')
TEIL = re.compile(r'[a-z0-9]+|[A-Z]+(?![a-z])|[A-Z][a-z0-9]*')


def morpheme():
    t = set()
    for z in open('tools/english/morphemes.tsv', encoding='utf-8'):
        if z.strip() and not z.startswith('#'):
            a, b = (z.rstrip('\n').split('\t') + [''])[:2]
            a, b = a.strip().lower(), b.strip().lower()
            # Identitaetseintraege (register->register) sind KEIN Deutsch.
            if a and b and a != b.replace('_', ''):
                t.add(a)
    return t


def ausnahmen():
    p = 'tools/english/exceptions.txt'
    if not os.path.exists(p):
        return set()
    return {z.strip() for z in open(p, encoding='utf-8')
            if z.strip() and not z.startswith('#')}


def dateien():
    aus = []
    for r in ROOTS:
        for d, _, fs in os.walk(r):
            if any(x in d for x in ('target', '__pycache__', '.test-work', '.git')):
                continue
            for f in fs:
                q = os.path.join(d, f)
                if f.endswith(('.rs', '.fi')) and not os.path.islink(q):
                    aus.append(q)
    return sorted(aus)


def main():
    morph, aus = morpheme(), ausnahmen()
    treffer = {}
    for f in dateien():
        try:
            namen = Q.bezeichner(open(f, encoding="utf-8").read(), f.endswith(".rs"))
        except Exception as e:
            print('?', f, e)
            continue
        for n in namen:
            if n in aus:
                continue
            teile = [ZIFFERNSCHWANZ.sub('', t.lower()) for t in TEIL.findall(n)]
            schuld = [t for t in teile if t in morph and len(t) >= 4]
            if schuld:
                treffer.setdefault(n, (schuld, set()))[1].add(f)
    for n in sorted(treffer):
        s, fs = treffer[n]
        print('GERMAN  %-32s (%s)  %s' % (n, ','.join(sorted(set(s))),
                                          ' '.join(sorted(fs)[:3])))
    print('German identifiers: %d' % len(treffer))
    return 1 if treffer else 0


sys.exit(main())
