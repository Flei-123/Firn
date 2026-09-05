#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/strings_tool.py — ersetzt Zeichenkettenliterale, deren
# INHALT genau ein Eintrag aus strings.tsv ist (mit oder ohne abschliessendes
# "\0"). Prosa bleibt unberuehrt; dafuer gibt es messages.tsv.
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

AUS = ('.git', 'target', '__pycache__', '.test-work', 'testdata', '.gc-meas-work')


def main(argv):
    probe = '--probe' in argv
    tab = {}
    for z in open('tools/english/strings.tsv', encoding='utf-8'):
        if not z.strip() or z.startswith('#'):
            continue
        a, b = z.rstrip('\n').split('\t')
        tab[a] = b
    n = 0
    ndat = 0
    for d, _, fs in os.walk('.'):
        if any(x in d.split(os.sep) for x in AUS):
            continue
        for f in fs:
            p = os.path.join(d, f)
            if not f.endswith(('.rs', '.fi')) or os.path.islink(p):
                continue
            s = open(p, encoding='utf-8').read()
            teile = []
            k = 0
            for art, text in Q.regionen(s, f.endswith('.rs')):
                if art == 'skip' and '"' in text[:2]:
                    q = text.index('"')
                    inner = text[q + 1:-1]
                    nul = inner.endswith('\\0')
                    kern = inner[:-2] if nul else inner
                    if kern in tab:
                        neu = tab[kern] + ('\\0' if nul else '')
                        text = text[:q + 1] + neu + '"'
                        k += 1
                teile.append(text)
            if k:
                n += k
                ndat += 1
                if not probe:
                    open(p, 'w', encoding='utf-8').write(''.join(teile))
    print(f"{'(probe) ' if probe else ''}{ndat} dateien, {n} literale ersetzt")


if __name__ == '__main__':
    main(sys.argv[1:])
