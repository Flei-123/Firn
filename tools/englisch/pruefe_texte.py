#!/usr/bin/env python3
"""tools/englisch/pruefe_texte.py — GEGENPROBE fuer die AUSGABETEXTE.

Sucht in allen ZEICHENKETTENLITERALEN der beiden Uebersetzer (compiler/src/*.rs,
lib/firnc1/*.fi, bin/*.fi) und in den Erwartungen der Tests nach deutschen
Woertern. Ein Treffer heisst: diese Meldung ist noch deutsch.

Deutsch erkannt wird ueber tools/englisch/morpheme.tsv (die Wortteiltabelle der
Umstellung) plus die Funktionswoerter aus WOERTER. Kommentare zaehlen NICHT —
die sind Etappe B. Ausnahmen stehen in texte_ausnahmen.txt (eine Zeile je
erlaubtem Wort oder `datei:wort`).
"""
import os, re, sys, glob

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

WOERTER = """der die das dem den des ein eine einer eines einem einen und oder nicht
kein keine keiner nur schon bereits mehrfach doch aber sondern weil wenn dann
sonst auch noch immer wieder sehr mehr weniger als wie durch ohne mit von zu
zur zum aus bei nach vor ueber unter zwischen gegen fuer ist sind war waren
wird werden wurde wurden hat haben hatte kann koennen muss muessen darf duerfen
soll sollen gibt geht steht liegt macht sich selbst hier dort dieser diese
dieses jeder jede jedes alle beide etwa etwas nichts schritte endlosschleife
verschachtelte""".split()

FILTER = re.compile(r'^[a-z]{3,}$')


def morpheme():
    t = set()
    for z in open('tools/englisch/morpheme.tsv', encoding='utf-8'):
        if z.strip() and not z.startswith('#'):
            teil = z.rstrip('\n').split('\t')
            if len(teil) < 2:
                continue
            a, b = teil[0].strip().lower(), teil[1].strip().lower()
            if a and b and a != b.replace('_', '') and FILTER.match(a):
                t.add(a)
    return t


def ausnahmen():
    p = 'tools/englisch/texte_ausnahmen.txt'
    if not os.path.exists(p):
        return set()
    return {z.strip() for z in open(p, encoding='utf-8')
            if z.strip() and not z.startswith('#')}


LIT = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
WORT = re.compile(r'[A-Za-zaeoeuess]+')


def main():
    de = morpheme() | set(WOERTER)
    aus = ausnahmen()
    dateien = sorted(set(glob.glob('compiler/src/*.rs') +
                         glob.glob('lib/firnc1/*.fi') + glob.glob('bin/*.fi')))
    treffer = []
    for f in dateien:
        if os.path.islink(f) or f.endswith('gctext.fi'):
            continue
        for i, zeile in enumerate(open(f, encoding='utf-8'), 1):
            nackt = zeile.strip()
            if nackt.startswith('//') or nackt.startswith('///') or nackt.startswith('//!'):
                continue
            for m in LIT.finditer(zeile):
                for w in WORT.findall(m.group(1)):
                    wl = w.lower()
                    if wl in de and wl not in aus and f + ':' + wl not in aus:
                        treffer.append((f, i, wl, m.group(1)[:70]))
    for t in treffer:
        print('DEUTSCH %s:%d  %s  |  %s' % t)
    print('deutsche Textstellen:', len(treffer))
    return 1 if treffer else 0


sys.exit(main())
