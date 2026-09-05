#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/fstrings2.py — Bezeichner in f-Zeichenketten nachziehen.

In `f"... {quadrat(9)} ..."` steht ein AUSDRUCK mitten im Literal. Der
Umbenenner laesst Literale in Ruhe, die Deklaration wird aber umbenannt.
Dieses Werkzeug ersetzt in JEDEM Klammerinhalt einer f-Zeichenkette die
Namen aus names.tsv. Der Text zwischen den Klammern (die Prosa) bleibt.
"""
import os, re, sys
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
tab = {}
for z in open('tools/english/names.tsv', encoding='utf-8'):
    if z.strip() and not z.startswith('#'):
        a, b = z.rstrip('\n').split('\t')[:2]
        tab[a] = b
FSTR = re.compile(r"f\"(?:[^\"\\\n]|\\.)*\"")
KLAM = re.compile(r'\{([^{}]*)\}')
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
n = 0
ziele = sys.argv[1:]
if not ziele:
    import glob
    ziele = [p for pat in ('tests/**/*.fi','lib/**/*.fi','bin/*.fi','demos/**/*.fi','examples/**/*.fi')
             for p in glob.glob(pat, recursive=True)]
for f in ziele:
    if os.path.islink(f): continue
    s = open(f, encoding='utf-8').read()
    def fs(m):
        def kl(k):
            global n
            def one(w):
                global n
                v = tab.get(w.group(0))
                if v:
                    n += 1
                    return v
                return w.group(0)
            return '{' + IDENT.sub(one, k.group(1)) + '}'
        return KLAM.sub(kl, m.group(0))
    neu = FSTR.sub(fs, s)
    if neu != s:
        open(f, 'w', encoding='utf-8').write(neu)
        print(f)
print(n, 'Namen in f-Zeichenketten ersetzt')
