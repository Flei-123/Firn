#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/check_lengths.py — GEGENPROBE fuer die LAENGENANGABEN.

In Firn stehen Zeichenketten als `var w: [u8; N] = "…"`. Die Laenge wird an
der Aufrufstelle NOCH EINMAL als Zahl mitgegeben:

    var mn: [u8; 13] = "firn.package\\0"
    join(out, wp, wn, (&mn[0]) as u64, 12)

Beim Uebersetzen ins Englische aendert sich die Textlaenge — bleibt eine
dieser Zahlen stehen, laeuft das Programm scheinbar weiter und schneidet den
Text ab (`firn.packa`). Dieses Skript vergleicht jede Zahl, die zusammen mit
`(&x[0]) as u64` uebergeben wird, mit der wirklichen Laenge von `x`
(ohne abschliessendes `\\0`) und meldet jede Abweichung.

Erlaubt sind absichtliche Teillaengen NICHT — wo wirklich ein Ausschnitt
gemeint ist, steht eine Rechnung (`n - 1`) statt einer nackten Zahl.
"""
import re, sys, os, glob

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

DEKL = re.compile(r'var\s+(\w+)\s*:\s*\[u8;\s*(\d+)\s*\]\s*=\s*"((?:[^"\\]|\\.)*)"')
RUF = re.compile(r'\(&(\w+)\[0\]\)\s*as\s+u64\s*,\s*(\d+)')


def echte_laenge(roh):
    # OKTETTE, nicht Zeichen: '—' ist in UTF-8 drei Oktette lang. Fluchtfolgen
    # (\0, \n, \t, \\, \") zaehlen als EIN Oktett, ein abschliessendes \0
    # gehoert nicht zum Text.
    s = re.sub(r'\\.', 'X', roh)
    n = len(s.encode('utf-8'))
    return n - (1 if roh.endswith('\\0') else 0)


def main():
    treffer = []
    dateien = sorted(set(glob.glob('lib/**/*.fi', recursive=True)
                         + glob.glob('bin/*.fi')))
    for f in dateien:
        if os.path.islink(f) or f.endswith('gctext.fi'):
            continue
        zeilen = open(f, encoding='utf-8').read().split('\n')
        laenge = {}
        for i, z in enumerate(zeilen):
            for m in DEKL.finditer(z):
                laenge[m.group(1)] = (echte_laenge(m.group(3)), m.group(3))
            for m in RUF.finditer(z):
                name, zahl = m.group(1), int(m.group(2))
                if name not in laenge:
                    continue
                echt, roh = laenge[name]
                getrimmt = len(roh.rstrip().encode('utf-8'))
                # Endet der Text auf \0, darf die Zahl das Nulloktett
                # mitzaehlen (dann ist eine C-Zeichenkette gemeint).
                mitnull = echt + (1 if roh.endswith('\\0') else 0)
                if zahl not in (echt, getrimmt, mitnull):
                    treffer.append((f, i + 1, name, zahl, echt, roh))
    for t in treffer:
        print("LAENGE %s:%d  %s uebergibt %d, Text ist %d ('%s')" % t)
    print('wrong length values:', len(treffer))
    return 1 if treffer else 0


sys.exit(main())
