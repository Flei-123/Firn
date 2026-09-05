#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/emitcode.py — Bezeichner in EMITTIERTEM Quelltext.

`comptime { emit_raw("fn tab_gross(c: i64) ...") }` erzeugt Firn-Quelltext
aus einer Zeichenkette. Der normale Umbenenner fasst Literale nicht an, die
Aufrufstelle des erzeugten Namens aber schon — danach passt beides nicht mehr
zusammen. Dieses Werkzeug ersetzt die Namen aus names.tsv INNERHALB der
Zeichenketten der Dateien, die `emit_raw`/`emit_number` benutzen.
"""
import os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

DATEIEN = sys.argv[1:]
tab = {}
for z in open('tools/english/names.tsv', encoding='utf-8'):
    if not z.strip() or z.startswith('#'):
        continue
    a, b = z.rstrip('\n').split('\t')[:2]
    tab[a] = b

LIT = re.compile(r'"(?:[^"\\]|\\.)*"')
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')

n = 0
for f in DATEIEN:
    s = open(f, encoding='utf-8').read()
    def lit(m):
        global n
        def one(w):
            global n
            v = tab.get(w.group(0))
            if v:
                n += 1
                return v
            return w.group(0)
        return IDENT.sub(one, m.group(0))
    neu = LIT.sub(lit, s)
    if neu != s:
        open(f, 'w', encoding='utf-8').write(neu)
        print(f)
print(n, 'Namen in Zeichenketten ersetzt')
