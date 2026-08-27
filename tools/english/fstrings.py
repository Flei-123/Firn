#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/fstrings.py — in `f"...{name}..."` steht ein BEZEICHNER
# mitten in einem Zeichenkettenliteral. Der normale Umbenenner laesst
# Literale in Ruhe; dieses Werkzeug holt genau die Klammerinhalte nach.
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

AUS = ('.git', 'target', '__pycache__', '.test-work', 'testdata', '.gc-meas-work')
KLAM = re.compile(r'\{([^{}]*)\}')
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')


def main():
    abb = {}
    for z in open('tools/english/names.tsv', encoding='utf-8'):
        a, n = z.rstrip('\n').split('\t')
        abb[a] = n
    ges = 0
    for d, _, fs in os.walk('.'):
        if any(x in d.split(os.sep) for x in AUS):
            continue
        for f in fs:
            p = os.path.join(d, f)
            if not f.endswith('.fi') or os.path.islink(p):
                continue
            s = open(p, encoding='utf-8').read()
            if 'f"' not in s:
                continue
            zaehler = [0]

            def rf(mm):
                r = abb.get(mm.group(0))
                if r is None:
                    return mm.group(0)
                zaehler[0] += 1
                return r

            def kf(m):
                return '{' + IDENT.sub(rf, m.group(1)) + '}'

            teile = []
            pos = 0
            for art, text in Q.regionen(s, False):
                if art == 'skip' and text.startswith('"') and pos > 0 and s[pos - 1] == 'f':
                    text = KLAM.sub(kf, text)
                teile.append(text)
                pos += len(text)
            if zaehler[0]:
                open(p, 'w', encoding='utf-8').write(''.join(teile))
                ges += zaehler[0]
                print(f"  {p}: {zaehler[0]}")
    print(f"{ges} Namen in f-Zeichenketten ersetzt")


if __name__ == '__main__':
    main()
