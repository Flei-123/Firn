#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/expectations.py — zieht die Testerwartungen nach.
#
#   --namen       Bezeichner in `// expect_error:` / `// expect_out:` ersetzen.
#                 Angefasst wird nur, was in 'einfachen Anfuehrungszeichen'
#                 steht (dort stehen die Namen) und was mit __ anfaengt —
#                 die Prosa der Meldung bleibt stehen.
#   --positionen  Spalten richtigstellen. Die Umbenennung verschiebt Spalten,
#                 aber nie Zeilen. Uebernommen wird eine neue Position NUR,
#                 wenn die ZEILE gleich bleibt und der erwartete TEXT weiter
#                 in der Meldung steht — sonst bleibt der Test rot.
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

FIRNC = 'compiler/target/release/firnc'
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
UNTER = re.compile(r'__[A-Za-z_][A-Za-z0-9_]*')
ZITAT = re.compile(r"'([^']*)'")
KOPF = re.compile(r'^// expect_(error|out):\s*(.*)$')


def tabelle(pfad):
    t = {}
    for z in open(pfad, encoding='utf-8'):
        if not z.strip() or z.startswith('#'):
            continue
        a, b = z.rstrip('\n').split('\t')
        t.setdefault(a, b)
    return t


def dateien():
    aus = []
    for w in ('tests', 'lib/rc/parts'):
        for d, _, fs in os.walk(w):
            for f in fs:
                p = os.path.join(d, f)
                if f.endswith('.fi') and not os.path.islink(p):
                    aus.append(p)
    return sorted(aus)


def namen():
    abb = tabelle('tools/english/names.tsv')
    for k, v in tabelle('tools/english/strings.tsv').items():
        abb.setdefault(k, v)
    n = 0
    for p in dateien():
        z = open(p, encoding='utf-8').read().split('\n')
        aend = False
        for i, zeile in enumerate(z[:3]):
            if not KOPF.match(zeile):
                continue

            def in_zitat(m):
                nonlocal n
                neu = IDENT.sub(lambda mm: abb.get(mm.group(0), mm.group(0)), m.group(1))
                if neu != m.group(1):
                    n += 1
                return "'" + neu + "'"

            neu = ZITAT.sub(in_zitat, zeile)
            neu = UNTER.sub(lambda mm: abb.get(mm.group(0), mm.group(0)), neu)
            if neu != zeile:
                z[i] = neu
                aend = True
        if aend:
            open(p, 'w', encoding='utf-8').write('\n'.join(z))
    print(f"{n} Namen in Erwartungen ersetzt")


def positionen():
    umw = 0
    rot = []
    umg = dict(os.environ, FIRNLIB=os.path.join(ROOT, 'lib'))
    for p in sorted(f for f in dateien() if '/neg' in f or '/parts' in f):
        z = open(p, encoding='utf-8').read().split('\n')
        m = KOPF.match(z[0])
        if not m or m.group(1) != 'error':
            continue
        rest = m.group(2)
        pos, _, msg = rest.partition(' ')
        if ':' not in pos:
            continue
        quelle = p
        if '/parts/' in p:                 # Rumpf: der erzeugte Test zaehlt
            name = os.path.basename(p)
            for kand in (f"tests/{name}", f"tests/neg/{name[4:]}" if name.startswith('neg_') else '',
                         f"tests/neg/{name[7:]}" if name.startswith('negarc_') else ''):
                if kand and os.path.exists(kand):
                    quelle = kand
                    break
            else:
                continue
        r = subprocess.run([FIRNC, '-o', '/tmp/neg.bin', quelle],
                           capture_output=True, text=True, env=umg)
        aus = r.stdout + r.stderr
        if msg not in aus:
            rot.append((p, 'TEXT', msg))
            continue
        if f":{pos}" in aus:
            continue
        zeile = pos.split(':')[0]
        treffer = re.findall(r':(' + re.escape(zeile) + r'):(\d+)', aus)
        if not treffer:
            rot.append((p, 'ZEILE', pos))
            continue
        neu = f"{treffer[0][0]}:{treffer[0][1]}"
        z[0] = z[0].replace(f"expect_error: {pos} ", f"expect_error: {neu} ")
        open(p, 'w', encoding='utf-8').write('\n'.join(z))
        print(f"  {p}: {pos} -> {neu}")
        umw += 1
    print(f"{umw} Positionen nachgezogen, {len(rot)} offen")
    for p, art, was in rot:
        print(f"  OFFEN {art:6s} {p}: {was}")


if __name__ == '__main__':
    if '--namen' in sys.argv:
        namen()
    if '--positionen' in sys.argv:
        positionen()
