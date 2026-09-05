#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Zieht die ERWARTETEN MELDUNGSTEXTE der Negativtests auf Englisch nach.

Verfahren (mechanisch, ohne Raten): fuer jeden Test in tests/neg/ laeuft
  * der ALTE Uebersetzer (Basis-Commit, eigener Arbeitsbaum) auf der ALTEN
    Fassung des Tests  -> deutsche Meldung
  * der NEUE Uebersetzer auf der neuen Fassung                -> englische Meldung
Die Zeile, in der der alte Erwartungstext steht, wird in der neuen Ausgabe an
DERSELBEN Stelle gesucht; ihr Meldungsteil (hinter `error: ` bzw. `= note: `)
ist der neue Erwartungstext. Die Position `Z:S` bleibt unberuehrt.

Aufruf:  python3 tools/english/expectations_texts.py [<basis-worktree>]
"""
import os, re, subprocess, sys, glob

WURZEL = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BASIS = sys.argv[1] if len(sys.argv) > 1 else '/tmp/firn-base'
os.chdir(WURZEL)

NEU_C = os.path.join(WURZEL, 'compiler/target/release/firnc')
ALT_C = os.path.join(BASIS, 'compiler/target/release/firnc')


def alte_namen():
    """neuer Pfad -> alter Pfad (Umbenennungen aus Schritt 1)."""
    aus = subprocess.run(['git', 'diff', '-M', '--name-status', '370e6b9'],
                         capture_output=True, text=True).stdout
    m = {}
    for zeile in aus.strip().split('\n'):
        t = zeile.split('\t')
        if t[0].startswith('R'):
            m[t[2]] = t[1]
        elif t[0] in ('M', 'A'):
            m[t[1]] = t[1]
    return m


def lauf(compiler, wurzel, datei):
    r = subprocess.run([compiler, '-o', '/tmp/.negbin', datei],
                       cwd=wurzel, capture_output=True, text=True)
    return (r.stdout + r.stderr).split('\n')


def text_von(zeile):
    z = zeile.strip()
    for p in ('error: ', '= note: ', 'note: '):
        if z.startswith(p):
            return z[len(p):]
    return None


def main():
    alt = alte_namen()
    geaendert, offen = 0, []
    for f in sorted(glob.glob('tests/neg/*.fi')):
        kopf = open(f, encoding='utf-8').readline().rstrip('\n')
        if 'expect_error: ' not in kopf:
            continue
        rest = kopf.split('expect_error: ', 1)[1]
        pos, _, msg = rest.partition(' ')
        if not msg:
            continue
        neu_aus = lauf(NEU_C, WURZEL, f)
        if any(msg in z for z in neu_aus):
            continue                      # schon englisch/passend
        o = alt.get(f)
        alt_aus = []
        if o and os.path.exists(os.path.join(BASIS, o)):
            alt_aus = lauf(ALT_C, BASIS, o)
        treffer = [i for i, z in enumerate(alt_aus) if msg in z]
        if not treffer:
            # Rueckfall: der ALTE Uebersetzer auf der NEUEN Fassung des Tests.
            alt_aus = lauf(ALT_C, WURZEL, f)
            treffer = [i for i, z in enumerate(alt_aus) if msg in z]
        if not treffer:
            offen.append((f, 'alter Text nicht in der alten Ausgabe')); continue
        i = treffer[0]
        if i >= len(neu_aus):
            offen.append((f, 'neue Ausgabe kuerzer')); continue
        neu_text = text_von(neu_aus[i])
        if not neu_text:
            kand = [text_von(z) for z in neu_aus if text_von(z)]
            if not kand:
                offen.append((f, 'kein Meldungstext in der neuen Ausgabe')); continue
            neu_text = kand[0]
        zeilen = open(f, encoding='utf-8').read().split('\n')
        zeilen[0] = '// expect_error: %s %s' % (pos, neu_text)
        open(f, 'w', encoding='utf-8').write('\n'.join(zeilen))
        geaendert += 1
        print('TEXT %-42s %s' % (f, neu_text))
    print('nachgezogen:', geaendert, ' offen:', len(offen))
    for o in offen:
        print('   ?', o)


main()
