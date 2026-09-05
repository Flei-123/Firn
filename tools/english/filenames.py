#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/english/filenames.py — benennt Dateien und Verzeichnisse mit
# deutschen Namensbestandteilen um (git mv) und zieht ALLE Verweise auf den
# alten Pfad im ganzen Baum nach.
#
#   python3 tools/english/filenames.py --probe [wurzel ...]
#   python3 tools/english/filenames.py         [wurzel ...]
#
# Ersetzt werden nur VOLLE Pfade (`lib/firnc1/types.fi`) und, fuer Dateien
# ohne Verzeichnisanteil im Verweis, der volle Dateiname (`types.fi`) —
# niemals blosse Wortstaemme, sonst zerlegt es den halben Baum.
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.chdir(ROOT)
sys.path.insert(0, 'tools/english')
import suggest as V

AUS = ('.git', 'target', '__pycache__', '.test-work', '.gauntlet-shots',
       'testdata', '.gc-meas-work', '.dom-soak-work', 'node_modules',
       # Das Umstellungswerkzeug selbst behaelt seine deutschen Namen:
       # es steht in der Arbeitsanweisung und benennt sich sonst selbst um.
       'englisch')
# Verzeichnisse, die als GANZES umziehen (Pfadanfang, damit die deutsche
# Prosa in den Kommentaren unangetastet bleibt).
EXTRA = [('beispiele/', 'demos/')]
TEXT = ('.rs', '.fi', '.sh', '.py', '.md', '.toml', '.txt', '.tsv', '.s')


def neuer_stamm(stamm, morph, hand):
    # Ein Modul heisst wie seine Datei: `import err` braucht `err.fi`. Nur
    # dann zaehlt die Handentscheidung fuer den GANZEN Namen. Fuer alles
    # andere (Testdateien) entscheidet allein die Morphemtabelle — sonst
    # schlaegt z. B. `ist -> actual` (Bezeichner) in einen Dateinamen durch.
    if not re.fullmatch(r'[A-Za-z0-9_]+', stamm):
        return stamm            # Bindestriche u. ae. nicht anfassen
    if stamm in hand:
        return hand[stamm]
    teile = []
    for t in stamm.split('_'):
        n, _, _ = V.uebersetze(t, morph)
        teile.append(n)
    return '_'.join(teile)


def main(argv):
    probe = '--probe' in argv
    wurzeln = [a for a in argv if not a.startswith('--')] or \
        ['compiler/src', 'lib', 'bin', 'tools', 'tests', 'beispiele', 'examples', 'bench', 'testdata']
    morph = V.tabelle('tools/english/morphemes.tsv')
    hand = V.tabelle('tools/english/manual.tsv')
    namen = V.tabelle('tools/english/names.tsv')
    hand = dict(namen, **hand)

    # 1. Umbenennungen sammeln (Dateien zuerst, dann Verzeichnisse von innen)
    paare = []
    for w in wurzeln:
        for d, ds, fs in os.walk(w, topdown=False):
            if any(a in d.split(os.sep) for a in AUS):
                continue
            for f in fs:
                stamm, endung = os.path.splitext(f)
                if endung not in ('.fi', '.rs', '.sh'):
                    continue
                n = neuer_stamm(stamm, morph, hand)
                if n != stamm:
                    paare.append((os.path.join(d, f), os.path.join(d, n + endung)))
            for x in ds:
                if x in AUS:
                    continue
                n = neuer_stamm(x, morph, hand)
                if n != x:
                    paare.append((os.path.join(d, x), os.path.join(d, n)))
    if probe:
        for a, b in paare:
            print(f"{a}  ->  {b}")
        print(len(paare), 'umbenennungen')
        return

    # 2. Verweise im ganzen Baum nachziehen — VOR dem Verschieben, damit die
    #    Pfade noch stimmen.
    ersetz = []
    for a, b in paare:
        ersetz.append((a, b))
        if os.path.isfile(a):
            ersetz.append((os.path.basename(a), os.path.basename(b)))
    # lange Pfade zuerst, sonst frisst ein kurzer Dateiname den langen Pfad an
    ersetz.extend(EXTRA)
    ersetz.sort(key=lambda x: -len(x[0]))
    pat = re.compile('|'.join(re.escape(a) for a, _ in ersetz))
    tab = dict(ersetz)
    n_dat = 0
    for d, _, fs in os.walk('.'):
        if any(a in d.split(os.sep) for a in AUS):
            continue
        for f in fs:
            p = os.path.join(d, f)
            if os.path.islink(p) or not f.endswith(TEXT):
                continue
            if os.path.getsize(p) > 1 << 20:
                continue
            try:
                s = open(p, encoding='utf-8').read()
            except (UnicodeDecodeError, IsADirectoryError):
                continue
            neu = pat.sub(lambda m: tab[m.group(0)], s)
            if neu != s:
                open(p, 'w', encoding='utf-8').write(neu)
                n_dat += 1
    # test.sh liegt in der Wurzel und wird von os.walk('.') mit erfasst.

    # 3. verschieben
    for a, b in paare:
        subprocess.run(['git', 'mv', a, b], check=True)
    for a, b in EXTRA:
        a, b = a.rstrip('/'), b.rstrip('/')
        if os.path.exists(a):
            subprocess.run(['git', 'mv', a, b], check=True)

    # 4. symbolische Verweise, die jetzt ins Leere zeigen, neu setzen
    n_link = 0
    for d, _, fs in os.walk('.'):
        if any(a in d.split(os.sep) for a in AUS):
            continue
        for f in fs:
            p = os.path.join(d, f)
            if not os.path.islink(p) or os.path.exists(p):
                continue
            ziel = os.readlink(p)
            neu_ziel = pat.sub(lambda m: tab[m.group(0)], ziel)
            if neu_ziel != ziel:
                os.remove(p)
                os.symlink(neu_ziel, p)
                subprocess.run(['git', 'add', p], check=True)
                n_link += 1
    print(f"{n_link} symbolische verweise nachgezogen")
    print(f"{len(paare)} umbenannt, {n_dat} dateien mit angepassten verweisen")


if __name__ == '__main__':
    main(sys.argv[1:])
