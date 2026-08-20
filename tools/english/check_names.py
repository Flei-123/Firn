#!/usr/bin/env python3
"""tools/english/check_names.py — GEGENPROBE fuer DATEI- und ORDNERNAMEN.

check.py sieht nur in die Bezeichner INNERHALB der Quellen; die drei
uebersehenen Dateien der Runde 55 (841_gcvec_inkrementell.fi,
iface_parameterzahl.fi, impl_argumentzahl.fi) und die Arbeitsordner
(.gc-mess-work, .baum-work, .selbst-work) sind ihr deshalb entgangen.

Geprueft wird jeder von git verwaltete Pfad: Ordner und Dateistamm werden in
Wortteile zerlegt, jeder Teil gegen tools/english/morphemes.tsv gehalten.
Ein Treffer heisst: dieser Pfad ist noch deutsch. 0 Treffer = fertig.
"""
import os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)

# Ausgenommen: fremde Daten (testdata) und das Umstellungswerkzeug selbst,
# das laut Arbeitsanweisung deutsch bleibt.
AUS = ('testdata/', 'tools/english/')
# Ein angehaengter Zaehler versteckt das deutsche Wort: 'pfad2', 'teil1'.
ZIFFERNSCHWANZ = re.compile(r'[0-9]+$')
TEIL = re.compile(r'[a-z0-9]+|[A-Z]+(?![a-z])|[A-Z][a-z0-9]*')
# Wortteile, die in Pfaden richtig sind, obwohl die Morphemtabelle sie kennt.
ERLAUBT = {'kernel', 'start', 'core', 'min', 'max', 'lib', 'bin', 'src',
           'demos', 'app', 'pause', 'linker', 'mess'}
# Runde 65: englische GANZE Woerter, in denen ein deutsches Morphem als
# Zeichenfolge steckt ('absolute' enthaelt 'absolut'). Die Suche im
# Wortinneren darf hier nicht anschlagen; als ganzes Teil geprueft, nicht
# als Zeichenfolge.
ENGLISCH = {'absolute', 'relative', 'negative', 'signature',
            'aggregate', 'profile', 'surrogate', 'parameter',
            'alternative', 'imperative', 'declarative', 'iterative'}


def morpheme():
    t = set()
    for z in open('tools/english/morphemes.tsv', encoding='utf-8'):
        if z.strip() and not z.startswith('#'):
            a, b = (z.rstrip('\n').split('\t') + [''])[:2]
            a, b = a.strip().lower(), b.strip().lower()
            if a and b and a != b.replace('_', ''):
                t.add(a)
    return t


def pfade():
    aus = subprocess.run(['git', 'ls-files'], capture_output=True, text=True,
                         check=True).stdout.split('\n')
    return [p for p in aus if p and not p.startswith(AUS)]


def main():
    morph = morpheme()
    # Nur lange Morpheme taugen zur Suche im Wortinneren; kurze wie
    # 'art' oder 'wert' stecken auch in englischen Woertern.
    lang_morph = sorted((m for m in morph if len(m) >= 6 and m not in ERLAUBT),
                        key=len, reverse=True)
    treffer = []
    gesehen = set()
    for p in pfade():
        # Ordnerkette und Dateistamm einzeln pruefen
        stuecke = p.split('/')
        stuecke[-1] = os.path.splitext(stuecke[-1])[0]
        for i, s in enumerate(stuecke):
            weg = '/'.join(stuecke[:i + 1])
            if weg in gesehen:
                continue
            gesehen.add(weg)
            for t in TEIL.findall(s):
                t = ZIFFERNSCHWANZ.sub('', t.lower())
                if t in ERLAUBT:
                    continue
                if t in morph:
                    treffer.append((p, s, t))
                    break
                # Runde 65: ein ZUSAMMENGESCHRIEBENES Kompositum
                # ('wertsemantik', 'wiederholungsliteral') ist EIN Teil und
                # stand deshalb nie in der Tabelle. Lange Morpheme werden
                # deshalb auch INNERHALB eines Teils gesucht.
                if t in ENGLISCH:
                    continue
                d = next((m for m in lang_morph if m in t), None)
                if d is not None:
                    treffer.append((p, s, d))
                    break
    for p, s, t in treffer:
        print(f"{p}\t{s}\t{t}")
    print(f"German path names: {len(treffer)}")
    return 1 if treffer else 0


if __name__ == '__main__':
    sys.exit(main())
