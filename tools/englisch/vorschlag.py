#!/usr/bin/env python3
# tools/englisch/vorschlag.py — schlaegt fuer jeden deutschen Bezeichner
# einen englischen vor, aus der Morphemtabelle. Aendert NICHTS.
import re, os, sys, collections

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
MORPH = {}
for zeile in open('tools/englisch/morpheme.tsv', encoding='utf-8'):
    if not zeile.strip() or zeile.startswith('#'):
        continue
    d, e = zeile.rstrip('\n').split('\t')
    MORPH[d] = e

ROOTS = ['compiler/src', 'lib', 'bin', 'tools', 'beispiele', 'examples', 'tests']
files = []
for r in ROOTS:
    for d, _, fs in os.walk(r):
        if 'target' in d or '__pycache__' in d or '.test-work' in d:
            continue
        for f in fs:
            if f.endswith(('.rs', '.fi')):
                files.append(os.path.join(d, f))

ident = collections.Counter()
pat = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
for p in files:
    s = open(p, encoding='utf-8', errors='replace').read()
    for m in pat.finditer(s):
        ident[m.group(0)] += 1

def teile(name):
    """Bezeichner in Morpheme zerlegen: _-Grenzen und CamelCase."""
    stuecke = []
    for gruppe in name.split('_'):
        stuecke.append(re.findall(r'[a-z0-9]+|[A-Z][a-z0-9]*|[A-Z]+(?![a-z])', gruppe) or [gruppe])
    return stuecke

vorschlag = {}
offen = collections.Counter()
for name, c in ident.items():
    if len(name) < 3 or name.startswith('__'):
        pass
    gruppen = teile(name)
    neu_gruppen = []
    getroffen = False
    unbekannt = []
    for g in gruppen:
        neu = []
        for t in g:
            low = t.lower()
            if low in MORPH:
                e = MORPH[low]
                neu.append(e.capitalize() if t[:1].isupper() else e)
                getroffen = True
            else:
                neu.append(t)
                if len(low) >= 4 and re.fullmatch(r'[a-z]+', low):
                    unbekannt.append(low)
        neu_gruppen.append(''.join(neu) if len(g) > 1 else neu[0])
    if getroffen:
        vorschlag[name] = '_'.join(neu_gruppen)
        for u in unbekannt:
            offen[u] += c

with open('tools/englisch/vorschlag.tsv', 'w', encoding='utf-8') as f:
    for k in sorted(vorschlag):
        f.write(f"{k}\t{vorschlag[k]}\n")
print(f"Bezeichner gesamt: {len(ident)}")
print(f"mit deutschem Morphem: {len(vorschlag)}")
print(f"unbekannte Wortteile darin: {len(offen)}")
print("haeufigste unbekannte:", ' '.join(w for w, _ in offen.most_common(60)))
