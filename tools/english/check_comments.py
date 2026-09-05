#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/english/check_comments.py — MASSSTAB fuer Etappe B.

Etappe A hat Bezeichner, Meldungen und Pfadnamen englisch gemacht. Etappe B
sind die KOMMENTARE und die Dokumentation. Prosa laesst sich nicht ueber die
Morphemtabelle erkennen (zu viele Fachwoerter sind in beiden Sprachen gleich),
deshalb entscheidet hier eine Liste deutscher Funktionswoerter: eine Zeile
gilt als deutsch, sobald eines davon als ganzes Wort darin vorkommt.

Die Liste enthaelt AUSDRUECKLICH keine Woerter, die es auch im Englischen
gibt (in, an, am, es, man, war, hat, die, name, wert, also) — die erste Fassung tat
das und hat englische Saetze als deutsch gemeldet; wer den Massstab dann
erfuellen will, schreibt verkrampftes Englisch.

  python3 tools/english/check_comments.py            Bilanz je Bereich
  python3 tools/english/check_comments.py --dateien  Bilanz je Datei
  python3 tools/english/check_comments.py --zeilen <datei>   die Zeilen

Rueckgabe 1, solange noch deutsche Zeilen uebrig sind.
"""
import os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

WORTE = """der das den dem des ein eine einen einem einer eines und oder
aber nicht kein keine keinen keiner nur noch schon auch sonst damit dass
weil wenn dann als wie sind waren wird werden wurde wurden sein haben hatte
hatten kann koennen konnte muss muessen musste soll sollen darf duerfen fuer
von vom mit ohne bei beim nach vor ueber unter zwischen durch gegen seit aus
zum zur sie ich wir sich jede jeder jedes alle alles beide dieser diese dieses
jenes dabei dafuer daraus davon dazu deshalb darum trotzdem immer nie oft
selten weniger sehr ganz genau erst zuerst danach spaeter zuvor wieder anders
richtig falsch wichtig moeglich noetig etwa etwas nichts jetzt heute zeile
zeilen datei dateien werte namen nummer nummern""".split()
RE_WORT = re.compile(r'\b(' + '|'.join(WORTE) + r')\b', re.I)

BEREICHE = [('compiler/src', ('.rs',)), ('lib', ('.fi',)), ('bin', ('.fi',)),
            ('tests', ('.fi',)), ('demos', ('.fi',)), ('bench', ('.fi',)),
            ('tools', ('.sh', '.py', '.fi')), ('docs', ('.md',))]
AUS = ('testdata/', 'tools/english/')


def dateien():
    aus = subprocess.run(['git', 'ls-files'], capture_output=True, text=True,
                         check=True).stdout.split('\n')
    return [p for p in aus if p and not p.startswith(AUS)]


def kommentarzeilen(pfad):
    """Nur Kommentare; in .md zaehlt jede Zeile."""
    try:
        inhalt = open(pfad, encoding='utf-8', errors='ignore').read().split('\n')
    except OSError:
        return []
    md = pfad.endswith('.md')
    aus = []
    for i, z in enumerate(inhalt, 1):
        s = z.strip()
        if md:
            if s:
                aus.append((i, s))
        elif s.startswith('//') or s.startswith('#'):
            aus.append((i, s))
    return aus


# Was in Rueckwaertsstrichen steht, ist Code oder ein zitierter Bezeichner --
# keine Prosa. `erst` in einem Satz ueber alte Namen ist kein deutscher Satz.
CODESPAN = re.compile(r'`[^`]*`')

# Runde B5: PFADE sind kein Deutsch. Die erste Zeile jeder Quelle in
# diesem Verzeichnis nennt ihren eigenen Pfad ohne Rueckwaertsstriche
# (`// lib/tls/der.fi -- ...`), und ein Dateistamm, der zufaellig wie ein
# Funktionswort aussieht, hat den Pruefer sonst genau dort anschlagen
# lassen, wo er am wenigsten hilft. Erkannt wird alles, was einen
# Schraegstrich oder eine bekannte Endung traegt.
PFAD = re.compile(r'\S*(?:/\S*|\.(?:fi|rs|py|sh|md|txt|tsv|toml|json|c|h))'
                  r'\S*')

# Runde 88: EIGENNAMEN, die zufaellig wie ein deutsches Funktionswort
# aussehen. `MIT` ist der Name der Lizenz, nicht die Praeposition `mit` --
# und weil RE_WORT ohne Ruecksicht auf Gross- und Kleinschreibung sucht,
# meldete `MIT -- see [LICENSE](LICENSE)` eine deutsche Zeile. Die Liste
# wird GROSS/KLEIN GENAU angewendet: `mit` faellt weiter auf.
#
# Runde B5: `DER` sind die DISTINGUISHED ENCODING RULES aus X.690 -- die
# Kodierung, in der jedes Zertifikat steht. GROSS geschrieben ist es das
# Fachwort der Norm und niemals der deutsche Artikel, der in Prosa klein
# steht. Ohne diesen Eintrag meldet der Pruefer elf Zeilen von
# `lib/tls/der.fi`, `x509.fi` und `rsa.fi` als deutsch, obwohl jede davon
# ein englischer Satz ueber ASN.1 ist.
NAMEN = re.compile(r'\b(?:MIT|DER)\b')


def deutsch(zeilen):
    return [(i, z) for i, z in zeilen
            if RE_WORT.search(NAMEN.sub(' ', PFAD.sub(' ',
                                              CODESPAN.sub(' ', z))))]


def bereich(pfad):
    for b, endungen in BEREICHE:
        if pfad.startswith(b + '/') and pfad.endswith(endungen):
            return b
    if pfad.endswith('.md'):
        return '*.md (Wurzel)'
    return None


def main():
    if len(sys.argv) > 2 and sys.argv[1] == '--zeilen':
        for i, z in deutsch(kommentarzeilen(sys.argv[2])):
            print(f"{sys.argv[2]}:{i}: {z}")
        return 0
    je_bereich, je_datei, summe = {}, {}, 0
    for p in dateien():
        b = bereich(p)
        if not b:
            continue
        n = len(deutsch(kommentarzeilen(p)))
        if n:
            je_bereich[b] = je_bereich.get(b, 0) + n
            je_datei[p] = n
            summe += n
    if len(sys.argv) > 1 and sys.argv[1] == '--dateien':
        for p, n in sorted(je_datei.items(), key=lambda x: -x[1]):
            print(f"{n:6d}  {p}")
    else:
        for b, n in sorted(je_bereich.items(), key=lambda x: -x[1]):
            print(f"{n:6d}  {b}")
    print(f"German comment/doc lines: {summe}")
    return 1 if summe else 0


if __name__ == '__main__':
    sys.exit(main())
