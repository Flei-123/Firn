#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/expectations_names.py — deutsche Namen in Erwartungen.

`// expect_error:` nennt Typen und Methoden auch OHNE Anfuehrungszeichen
("... verlangt *Punkt"). expectations.py fasst nur Zitiertes an. Hier wird
zusaetzlich jeder ALTE Name ersetzt, dessen NEUER Name im Quelltext derselben
Datei wirklich vorkommt — damit bleibt die Prosa der Meldung unberuehrt.
"""
import os, re, sys, glob
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import source as Q

tab = {}
for z in open('tools/english/names.tsv', encoding='utf-8'):
    if z.strip() and not z.startswith('#'):
        a, b = z.rstrip('\n').split('\t')[:2]
        tab[a] = b

PROSA = {'eine', 'einer', 'klasse', 'funktion', 'zahl', 'wert', 'datei', 'typ'}

n = 0
for f in sorted(glob.glob('tests/**/*.fi', recursive=True)):
    if os.path.islink(f):
        continue
    s = open(f, encoding='utf-8').read()
    im_code = set(Q.bezeichner(s))
    zeilen = s.split('\n')
    geaendert = False
    for i, z in enumerate(zeilen):
        if not re.match(r'^// expect_(error|out):', z):
            continue
        neu = z
        for alt, engl in tab.items():
            # Kurznamen und Wortteile, die auch PROSA sein koennen ('eine'->'a',
            # 'klasse'->'class'), bleiben aussen vor.
            if len(alt) < 5 or alt in PROSA:
                continue
            if engl in im_code and re.search(r'\b%s\b' % re.escape(alt), neu):
                neu = re.sub(r'\b%s\b' % re.escape(alt), engl, neu)
        if neu != z:
            zeilen[i] = neu
            geaendert = True
            n += 1
    if geaendert:
        open(f, 'w', encoding='utf-8').write('\n'.join(zeilen))
        print(f)
print(n, 'Erwartungszeilen gerichtet')
